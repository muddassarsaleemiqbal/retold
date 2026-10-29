//! Discoveries prefill editable forms; only reviewed definitions are dispatched.
use super::{COMPACT, NORMAL, WIDE, fresh, key, result, screen, through_every_step, words};
use crate::tui::{
    app::App,
    collection, discovery,
    page::{Modal, Page},
    runtime::Job,
};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use retold_app::protocol::{Diagnostic, ErrorCode};
use serde_json::{Value, json};

const DISCOVERY_TAB_STEPS: usize = 4;

fn inventory() -> Value {
    json!({"inventory": {
        "tools": [
            {"name": "claude", "path": "/tools with spaces/claude"},
            {"name": "codex", "path": null},
            {"name": "ffmpeg", "path": "/tools/ffmpeg"}
        ],
        "profiles": ["/home/.claude-work", "/home/.codex-मेरी आवाज़"],
        "credential_variables": [
            {"name": "OPENROUTER_API_KEY", "present": true},
            {"name": "OPENAI_API_KEY", "present": true}
        ]
    }})
}

pub(super) fn discovery_app() -> App {
    let mut app = fresh();
    app.store
        .fill(words("providers scan"), result(inventory()), 0);
    app.press(key(KeyCode::Char('7')));
    for _ in 0..DISCOVERY_TAB_STEPS {
        app.press(key(KeyCode::Tab));
    }
    assert_eq!(app.tab().title(), "Discover");
    app
}

#[test]
fn picker_only_offers_supported_present_evidence_without_credential_values() {
    let items = collection::items(&collection::DISCOVER, &inventory());
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].raw["kind"], "claude_cli");
    assert_eq!(items[1].raw["binary"], "/tools with spaces/claude");
    assert_eq!(items[1].raw["profile-dir"], "/home/.claude-work");
    assert_eq!(items[2].raw["kind"], "codex_cli");
    assert!(items[2].raw["binary"].is_null());
    assert_eq!(items[3].raw["key-env"], "OPENROUTER_API_KEY");
    assert_eq!(discovery::fill(&items[3]).len(), 3);
}

#[test]
fn adoption_preserves_paths_as_single_arguments_and_waits_for_confirmation() {
    let mut app = discovery_app();
    app.press(key(KeyCode::Down));
    app.press(key(KeyCode::Enter));
    assert!(matches!(app.page(), Some(Page::Form(form))
        if form.value("profile-dir").as_deref() == Some("/home/.claude-work")));
    let mut app = through_every_step(app);
    let Some(Modal::Confirm(form)) = &app.modal else {
        panic!("missing confirmation")
    };
    let args = crate::command()
        .try_get_matches_from(std::iter::once("retold".to_owned()).chain(form.arguments()))
        .unwrap();
    let providers = args.subcommand_matches("providers").unwrap();
    let add = providers.subcommand_matches("add").unwrap();
    assert_eq!(
        add.get_one::<String>("binary").unwrap(),
        "/tools with spaces/claude"
    );
    assert_eq!(app.jobs(), Vec::<Job>::new());
    let expected = form.arguments();
    app.press(key(KeyCode::Enter));
    assert!(matches!(app.jobs().as_slice(), [Job::Action { words, .. }] if words == &expected));
}

#[test]
fn cancelling_adoption_keeps_the_discovery_and_never_dispatches() {
    let mut app = discovery_app();
    app.press(key(KeyCode::Enter));
    let mut app = through_every_step(app);
    app.press(key(KeyCode::Esc));
    assert!(matches!(app.page(), Some(Page::Form(_))));
    app.press(key(KeyCode::Esc));
    assert!(app.page().is_none());
    assert_eq!(app.tab().title(), "Discover");
    assert_eq!(app.jobs(), Vec::<Job>::new());
}

#[test]
fn unicode_profile_paths_are_preserved_and_suggested_aliases_are_cli_safe() {
    let items = discovery::items(&inventory());
    let profile = &items[2];
    let id = profile.raw["id"].as_str().unwrap();
    assert!(
        id.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
    );
    assert_eq!(profile.raw["profile-dir"], "/home/.codex-मेरी आवाज़");
    assert_eq!(discovery::items(&json!({"inventory": {}})), []);
}

#[test]
fn picker_is_offline_and_legible_at_every_breakpoint() {
    let app = discovery_app();
    for (width, height) in [COMPACT, NORMAL, WIDE] {
        let shown = screen(&app, width, height);
        assert!(
            shown.contains("Discover · Offline"),
            "{width}x{height}: {shown}"
        );
        assert!(shown.contains("claude-work"));
    }
    assert_eq!(app.command(), "retold providers scan");
    assert_eq!(app.jobs(), Vec::<Job>::new());
    assert!(
        app.wants()
            .iter()
            .any(|want| want.words == words("providers scan"))
    );
}

#[test]
fn empty_and_failed_scans_remove_stale_adoptions() {
    let failed = Diagnostic::new(ErrorCode::InputRequired, "Could not inspect profiles.");
    for (payload, expected) in [
        (result(json!({"inventory": {}})), "No supported providers"),
        (failed.into_payload(), "Could not inspect profiles."),
    ] {
        let mut app = discovery_app();
        app.store.fill(words("providers scan"), payload, 0);
        assert_eq!(app.visible_items(), []);
        assert!(screen(&app, WIDE.0, WIDE.1).contains(expected));
        app.press(key(KeyCode::Enter));
        assert!(app.page().is_none());
        assert_eq!(app.jobs(), Vec::<Job>::new());
    }
}

#[test]
fn mouse_selects_and_opens_the_visible_profile_row() {
    let mut app = discovery_app();
    app.area = ratatui::layout::Rect::new(0, 0, COMPACT.0, COMPACT.1);
    let body = crate::tui::view::regions(app.area).body;
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: body.x + 1,
        row: body.y + 2,
        modifiers: KeyModifiers::NONE,
    });
    app.react(&click);
    assert_eq!(
        app.selected_item().unwrap().raw["profile-dir"],
        "/home/.claude-work"
    );
    app.react(&click);
    assert!(matches!(app.page(), Some(Page::Form(form))
        if form.value("profile-dir").as_deref() == Some("/home/.claude-work")));
}
