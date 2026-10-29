//! Route editing keeps a draft until confirmation and only offers capable providers.
use super::{COMPACT, NORMAL, WIDE, fresh, key, result, screen, type_text, words};
use crate::tui::page::Modal;
use crate::tui::routes::Editor;
use crate::tui::runtime::Job;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use serde_json::json;

fn providers() -> serde_json::Value {
    json!({"providers": [
        {"id": "claude", "supported_tiers": ["text.fast", "text.smart"]},
        {"id": "codex", "supported_tiers": ["text.fast"]},
        {"id": "voice", "supported_tiers": ["tts"]}
    ]})
}

fn editor() -> Editor {
    Editor::new(
        "text.fast",
        &json!(["claude:fast", "codex:fast"]),
        &providers(),
    )
}

impl Editor {
    fn key(&mut self, code: KeyCode) {
        self.press(KeyEvent::from(code));
    }
}

#[test]
fn route_entry_picker_filters_capabilities_and_rejects_invalid_or_duplicate_entries() {
    let mut editor = editor();
    assert_eq!(editor.providers, ["claude", "codex"]);
    editor.key(KeyCode::Char('+'));
    editor.key(KeyCode::Enter);
    assert!(editor.problem.is_some());
    for letter in "fast".chars() {
        editor.key(KeyCode::Char(letter));
    }
    editor.key(KeyCode::Enter);
    assert!(editor.problem.as_ref().unwrap().contains("already"));
    editor.key(KeyCode::Tab);
    for letter in "smart".chars() {
        editor.key(KeyCode::Char(letter));
    }
    editor.key(KeyCode::Enter);
    assert_eq!(editor.chain, ["claude:fast", "codex:fast", "codex:smart"]);
    assert!(editor.problem.is_none());
}

#[test]
fn reorder_and_remove_preserve_the_selected_entry_and_stop_at_boundaries() {
    let mut editor = editor();
    editor.key(KeyCode::Char('K'));
    assert_eq!(editor.selected, 0);
    editor.key(KeyCode::Char('J'));
    assert_eq!(editor.chain, ["codex:fast", "claude:fast"]);
    assert_eq!(editor.selected, 1);
    editor.key(KeyCode::Char('J'));
    assert_eq!(editor.selected, 1);
    editor.key(KeyCode::Char('-'));
    editor.key(KeyCode::Char('-'));
    editor.key(KeyCode::Char('-'));
    assert_eq!(editor.chain, Vec::<String>::new());
    assert_eq!(editor.selected, 0);
}

fn routes_app() -> super::App {
    let mut app = fresh();
    app.store
        .fill(words("providers list"), result(providers()), 0);
    app.store.fill(
        words("providers route"),
        result(json!({"routing": {
            "text.fast": ["claude:fast", "codex:fast"]
        }})),
        0,
    );
    app.press(key(KeyCode::Char('7')));
    app.press(key(KeyCode::Tab));
    assert_eq!(
        app.visible_items().len(),
        retold_app::providers::routing_tiers().len()
    );
    app.press(key(KeyCode::Enter));
    assert!(matches!(&app.modal, Some(Modal::Route(editor)) if editor.tier == "text.fast"));
    app
}

#[test]
fn routes_tab_opens_existing_chain_and_confirmation_dispatches_exact_order() {
    let mut app = routes_app();
    app.press(key(KeyCode::Char('J')));
    assert_eq!(app.jobs(), Vec::<Job>::new());
    app.press(key(KeyCode::Enter));
    assert!(matches!(&app.modal, Some(Modal::Confirm(_))));
    assert_eq!(app.jobs(), Vec::<Job>::new());
    assert!(app.command().contains("codex:fast,claude:fast"));
    app.press(key(KeyCode::Enter));
    let jobs = app.jobs();
    let [Job::Action { words, .. }] = jobs.as_slice() else {
        panic!("{jobs:?}");
    };
    let matches = crate::command()
        .try_get_matches_from(std::iter::once("retold".to_owned()).chain(words.clone()))
        .unwrap();
    let group = matches.subcommand_matches("providers").unwrap();
    let route = group.subcommand_matches("route").unwrap();
    assert_eq!(route.get_one::<String>("tier").unwrap(), "text.fast");
    assert_eq!(
        route.get_one::<String>("order").unwrap(),
        "codex:fast,claude:fast"
    );
    assert!(!route.get_flag("dry-run"));
}

#[test]
fn cancelling_discards_a_draft_and_empty_chains_cannot_be_submitted() {
    let mut app = fresh();
    app.modal = Some(Modal::Route(editor()));
    app.press(key(KeyCode::Char('J')));
    app.press(key(KeyCode::Esc));
    assert!(app.modal.is_none());
    assert_eq!(app.jobs(), Vec::<Job>::new());
    app.modal = Some(Modal::Route(Editor::new("tts", &json!([]), &providers())));
    app.press(key(KeyCode::Enter));
    assert!(matches!(app.modal, Some(Modal::Route(_))));
    assert_eq!(app.jobs(), Vec::<Job>::new());
    app.press(key(KeyCode::Char('+')));
    let mut app = type_text(app, "local");
    app.press(key(KeyCode::Esc));
    assert!(matches!(&app.modal, Some(Modal::Route(editor)) if editor.adding.is_none()));
}

#[test]
fn route_editor_and_entry_picker_render_at_all_supported_sizes() {
    let mut app = fresh();
    app.modal = Some(Modal::Route(editor()));
    for (width, height) in [COMPACT, NORMAL, WIDE] {
        let frame = screen(&app, width, height);
        assert!(frame.contains("Route · text.fast"));
        assert!(frame.contains("claude:fast"));
        assert!(frame.contains("reorder"));
    }
    app.press(key(KeyCode::Char('+')));
    assert!(screen(&app, COMPACT.0, COMPACT.1).contains("Tab picks a provider"));
}

/// The words the open form would run, or nothing when no form is open.
fn form_words(app: &crate::tui::app::App) -> Vec<String> {
    match app.page() {
        Some(crate::tui::page::Page::Form(form)) => form.arguments(),
        _ => Vec::new(),
    }
}

#[test]
fn routes_tab_new_opens_suggestions_without_running_them() {
    use crate::tui::collection::ROUTES;
    use crate::tui::landing::{Tab, tabs};
    use crate::tui::section::Section;
    let mut app = fresh();
    app.press(key(KeyCode::Char('7')));
    let position = tabs(Section::Providers)
        .iter()
        .position(|tab| *tab == Tab::Collection(&ROUTES))
        .unwrap();
    for _ in 0..position {
        app.press(key(KeyCode::Char(']')));
    }
    app.press(key(KeyCode::Char('n')));
    assert_eq!(form_words(&app), ["providers", "route", "--suggest"]);
    assert!(
        app.jobs()
            .iter()
            .all(|job| !matches!(job, Job::Action { .. }))
    );
}

#[test]
fn onboarding_routing_step_starts_from_suggestions() {
    let step = crate::tui::onboarding::STEPS
        .iter()
        .position(|(_, command)| *command == "providers route")
        .unwrap();
    let mut app = fresh();
    app.modal = Some(Modal::Setup(step));
    app.press(key(KeyCode::Enter));
    assert!(form_words(&app).contains(&"--suggest".to_owned()));
}
