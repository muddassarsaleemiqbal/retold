//! Status inspection refreshes offline evidence and retains action safety gates.
use super::{COMPACT, NORMAL, WIDE, fresh, key, result, screen, through_every_step, words};
use crate::tui::{
    app::App,
    page::{Modal, Page, Reading},
    runtime::Job,
};
use ratatui::crossterm::event::KeyCode;
use retold_app::protocol::{Diagnostic, ErrorCode};
use serde_json::json;

pub(super) fn status_app() -> App {
    let mut app = fresh();
    app.store.fill(
        words("providers health"),
        result(json!({"health": [{
            "id": "claude", "status": "cooldown", "observed_at_utc": "2026-10-21T10:00:00Z",
            "retry_at_utc": "2026-10-21T10:02:00Z", "observation": {"consecutive_failures": 2}
        }]})),
        0,
    );
    app.store.fill(
        Reading::provider("claude").words,
        result(json!({"login_status": {
            "id": "claude", "kind": "claude_cli", "status": "profile_missing",
            "detail": "Configure a profile folder.", "authenticated": false
        }})),
        0,
    );
    app.press(key(KeyCode::Char('7')));
    app.press(key(KeyCode::Char('l')));
    app
}

#[test]
fn provider_shortcut_opens_a_read_only_live_status_page() {
    let app = status_app();
    assert!(
        matches!(app.page(), Some(Page::Reading(reading)) if reading.provider_id() == Some("claude"))
    );
    assert_eq!(app.jobs(), Vec::<Job>::new());
    let wants = app.wants();
    for command in [words("providers health"), Reading::provider("claude").words] {
        assert!(wants.iter().any(|want| want.words == command && want.live));
    }
    assert_eq!(app.command(), "retold providers login --status claude");
}

#[test]
fn health_and_login_tabs_open_status_for_the_selected_provider() {
    let mut app = status_app();
    app.press(key(KeyCode::Esc));
    app.press(key(KeyCode::Tab));
    app.press(key(KeyCode::Tab));
    assert_eq!(app.tab().title(), "Health");
    assert_eq!(app.selected_item().unwrap().note, "cooldown");
    app.press(key(KeyCode::Enter));
    assert!(
        matches!(app.page(), Some(Page::Reading(reading)) if reading.provider_id() == Some("claude"))
    );
    app.press(key(KeyCode::Esc));
    app.press(key(KeyCode::Tab));
    assert_eq!(app.tab().title(), "Login");
    app.press(key(KeyCode::Enter));
    assert!(
        matches!(app.page(), Some(Page::Reading(reading)) if reading.provider_id() == Some("claude"))
    );
    assert_eq!(app.jobs(), Vec::<Job>::new());
}

#[test]
fn status_page_displays_recorded_timing_and_offline_login_evidence_at_every_size() {
    let app = status_app();
    for (width, height) in [COMPACT, NORMAL, WIDE] {
        let shown = screen(&app, width, height);
        for expected in [
            "Limited · cooling down",
            "2026-10-21T10:02:00Z",
            "profile_missing",
            "Configure a profile folder.",
            "no provider requests sent",
        ] {
            assert!(
                shown.contains(expected),
                "missing {expected} at {width}x{height}"
            );
        }
    }
}

#[test]
fn login_configuration_is_prefilled_and_cancellation_never_dispatches() {
    let mut app = status_app();
    app.press(key(KeyCode::Char('c')));
    assert!(
        matches!(app.page(), Some(Page::Form(form)) if form.value("id").as_deref() == Some("claude"))
    );
    let mut app = through_every_step(app);
    assert!(matches!(app.modal, Some(Modal::Confirm(_))));
    app.press(key(KeyCode::Esc));
    assert_eq!(app.jobs(), Vec::<Job>::new());
}

#[test]
fn test_shortcut_defaults_to_zero_and_waits_for_a_reviewed_plan() {
    let mut app = status_app();
    app.press(key(KeyCode::Char('t')));
    assert!(
        matches!(app.page(), Some(Page::Form(form)) if form.value("spend").as_deref() == Some("0"))
    );
    let mut app = through_every_step(app);
    assert!(
        matches!(&app.modal, Some(Modal::Plan(review)) if review.form.value("id").as_deref() == Some("claude"))
    );
    app.press(key(KeyCode::Enter));
    assert_eq!(app.jobs(), Vec::<Job>::new());
}

#[test]
fn failed_inspection_remains_visible_and_does_not_claim_success() {
    for mut app in [status_app(), inventory_app()] {
        let diagnostic = Diagnostic::new(ErrorCode::InputRequired, "Profile inspection refused.");
        app.store.fill(
            Reading::provider("claude").words,
            diagnostic.into_payload(),
            0,
        );
        let shown = screen(&app, COMPACT.0, COMPACT.1);
        assert!(shown.contains("RG-INPUT-REQUIRED"));
        assert!(shown.contains("Profile inspection"), "{shown}");
        assert!(shown.contains("refused."), "{shown}");
        assert!(!shown.contains("profile_missing"));
        assert_eq!(app.jobs(), Vec::<Job>::new());
    }
}

#[test]
fn refreshed_health_never_turns_unknown_into_a_success_claim() {
    for mut app in [status_app(), inventory_app()] {
        app.store.fill(
            words("providers health"),
            result(json!({"health": [{
                "id": "claude", "status": "not_checked", "observation": null
            }]})),
            0,
        );
        let shown = screen(&app, COMPACT.0, COMPACT.1);
        assert!(shown.contains("Not checked"));
        assert!(!shown.contains("Last check succeeded"));
        assert!(!shown.contains("Limited · cooling down"));
    }
}

fn inventory_app() -> App {
    let mut app = status_app();
    app.press(key(KeyCode::Esc));
    app.store.fill(
        words("providers list"),
        result(json!({"providers": [{
            "id": "claude", "kind": "claude_cli",
            "health": "not_checked", "authentication": "not_checked"
        }]})),
        0,
    );
    app
}

fn successful_inventory_app() -> App {
    let mut app = inventory_app();
    app.store.fill(
        Reading::provider("claude").words,
        result(json!({"login_status": {
            "id": "claude", "kind": "claude_cli", "status": "profile_found",
            "detail": "Profile directory exists.", "authenticated": false,
            "authentication_checked": false
        }})),
        0,
    );
    app.store.fill(
        words("providers health"),
        result(json!({"health": [{
            "id": "claude", "status": "ok", "observed_at_utc": "2026-10-23T07:19:35Z"
        }]})),
        0,
    );
    app
}

#[test]
fn providers_overview_uses_live_evidence_instead_of_inventory_placeholders() {
    let app = successful_inventory_app();
    for (width, height) in [COMPACT, NORMAL, WIDE] {
        let shown = screen(&app, width, height);
        assert!(shown.contains("Last check succeeded"), "{shown}");
        assert!(shown.contains("2026-10-23T07:19:35Z"), "{shown}");
        assert!(shown.contains("profile_found"), "{shown}");
        assert!(!shown.contains("not_checked"), "{shown}");
    }
    let wants = app.wants();
    for command in [words("providers health"), Reading::provider("claude").words] {
        assert_eq!(wants.iter().filter(|want| want.words == command).count(), 1);
        assert!(wants.iter().any(|want| want.words == command && want.live));
    }
    assert_eq!(app.jobs(), Vec::<Job>::new());
}
