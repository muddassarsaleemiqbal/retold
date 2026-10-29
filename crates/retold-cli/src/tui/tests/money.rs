//! Financial actions cannot dispatch until a successful read-only preview is visible.
use super::*;
const WINDOW_COUNT: usize = 3;
const GROUP_COUNT: usize = 4;
use crate::tui::{collection, landing::Tab, runtime::Job};

#[test]
fn limits_keep_scope_and_window_and_open_editor() {
    let mut app = fresh();
    app.go(section::Section::Money);
    app.click_tab(1);
    app.store.fill(
        words("account show"),
        result(json!({"limits": [
            {"scope": {"kind":"provider", "id":"openrouter:KEY"}, "window":"month",
             "resources":{"usd_micros": 5_000_000, "requests": 100}}
        ]})),
        0,
    );
    assert_eq!(app.visible_items()[0].key, "provider:openrouter:KEY");
    app.press(key(KeyCode::Enter));
    let Some(Page::Form(form)) = app.page() else {
        panic!("limit editor");
    };
    assert_eq!(
        form.value("scope").as_deref(),
        Some("provider:openrouter:KEY")
    );
    assert_eq!(form.value("window").as_deref(), Some("month"));
}

#[test]
fn attempts_open_reconciliation_with_exact_id() {
    let mut app = fresh();
    app.go(section::Section::Money);
    app.click_tab(2);
    app.store.fill(
        words(collection::ATTEMPTS.list),
        result(json!({"account_attempts": {
            "attempts": [{"id":"a-1", "state":"dispatched"}], "next_after": null
        }})),
        0,
    );
    assert_eq!(app.tab(), Tab::Collection(&collection::ATTEMPTS));
    app.press(key(KeyCode::Enter));
    let Some(Page::Form(form)) = app.page() else {
        panic!("reconcile editor");
    };
    assert_eq!(form.value("attempt").as_deref(), Some("a-1"));
}

fn reconciliation_form() -> Form {
    let action = catalog::actions()
        .into_iter()
        .find(|action| action.command() == "account reconcile")
        .unwrap();
    Form::new(action)
        .with("attempt", "a-1")
        .with("cost", "1.25")
}

#[test]
fn financial_preview_blocks_loading_and_failure_and_preserves_cancelled_form() {
    let form = reconciliation_form();
    let preview = crate::tui::preview::Preview::new(&form).unwrap();
    let mut app = fresh();
    app.modal = Some(Modal::Preview(preview.clone()));
    app.press(key(KeyCode::Enter));
    assert_eq!(app.jobs(), Vec::new());
    assert!(matches!(app.modal, Some(Modal::Preview(_))));
    app.store.fill(
        preview.words,
        retold_app::protocol::Diagnostic::new(
            retold_app::protocol::ErrorCode::AccountRequired,
            "Missing attempt",
        )
        .into_payload(),
        0,
    );
    app.press(key(KeyCode::Enter));
    assert_eq!(app.jobs(), Vec::new());
    app.press(key(KeyCode::Esc));
    assert!(matches!(app.page(), Some(Page::Form(saved)) if saved.arguments() == form.arguments()));
}

#[test]
fn successful_preview_dispatches_only_the_reviewed_mutation() {
    let form = reconciliation_form();
    let preview = crate::tui::preview::Preview::new(&form).unwrap();
    let mut app = fresh();
    app.modal = Some(Modal::Preview(preview.clone()));
    app.store.fill(
        preview.words,
        result(json!({"reconcile_preview": {"dry_run":true}})),
        0,
    );
    app.press(key(KeyCode::Enter));
    assert!(matches!(&app.jobs()[0], Job::Action {words, ..} if *words == form.arguments()));
}

#[test]
fn spend_windows_and_groups_use_shared_cli_and_render_in_all_sizes() {
    let mut app = fresh();
    app.go(section::Section::Money);
    for _ in 0..WINDOW_COUNT {
        for _ in 0..GROUP_COUNT {
            let words = app.spend.words();
            assert!(
                crate::command()
                    .try_get_matches_from(std::iter::once("retold".into()).chain(words.clone()))
                    .is_ok()
            );
            app.store
                .fill(words, result(json!({"spend":{"rows":[],"total":{}}})), 0);
            for (width, height) in [COMPACT, NORMAL, WIDE] {
                assert!(screen(&app, width, height).contains("Spend"));
            }
            app.press(key(KeyCode::Char('b')));
        }
        app.press(key(KeyCode::Char('w')));
    }
}

#[test]
fn explicit_dry_runs_skip_the_apply_preview() {
    let form = reconciliation_form().with("dry-run", "true");
    assert_eq!(form.value("dry-run").as_deref(), Some("true"));
    assert!(crate::tui::preview::Preview::new(&form).is_none());
    assert_eq!(
        form.arguments(),
        words("account reconcile --attempt a-1 --cost 1.25 --dry-run")
    );
    assert!(crate::tui::preview::Preview::new(&form.with("dry-run", "false")).is_some());
}
