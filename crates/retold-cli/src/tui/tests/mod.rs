//! Reducer, rendering and command-parity proof without providers or a real terminal.
use std::collections::BTreeSet;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use retold_app::protocol::{Outcome, Payload};
use serde_json::{Value, json};

use super::app::App;
use super::catalog;
use super::form::Form;
use super::input::Input;
use super::page::{Modal, Page};
use super::section;
use super::store::{Store, Want, words};
use super::theme::Theme;

const COMPACT: (u16, u16) = (80, 24);
const NORMAL: (u16, u16) = (120, 40);
const WIDE: (u16, u16) = (200, 60);
const INPUT_COLUMNS: u16 = 6;

fn result(contents: Value) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents,
    }
}

fn fresh() -> App {
    let mut app = App::new(Theme::NoColor);
    for (command, contents) in [
        (
            "providers list",
            json!({"providers": [{"id": "claude", "kind": "claude_cli"}]}),
        ),
        (
            "providers route",
            json!({"routing": {"text.smart": ["claude:smart"]}}),
        ),
        ("guides list", json!({"items": [], "total": 0})),
        ("channels list", json!({"items": [], "total": 0})),
        ("video list", json!({"items": [], "total": 0})),
        (
            "workspace show",
            json!({"workspace": {"name": "The Whole Plot", "registered": true}}),
        ),
    ] {
        app.store.fill(words(command), result(contents), 0);
    }
    app
}

fn screen(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(super::view::frame(app)).unwrap();
    terminal.backend().to_string()
}

const fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_text(mut app: App, text: &str) -> App {
    for letter in text.chars() {
        app.press(key(KeyCode::Char(letter)));
    }
    app
}

fn paths(command: &clap::Command, path: &str) -> BTreeSet<String> {
    let children: Vec<_> = command
        .get_subcommands()
        .filter(|child| !["help", "publish"].contains(&child.get_name()))
        .collect();
    let standalone = children.is_empty() || !command.is_subcommand_required_set();
    let own = (!path.is_empty() && standalone).then(|| path.to_owned());
    let nested = children.into_iter().flat_map(|child| {
        let child_path = format!("{path} {}", child.get_name());
        paths(child, child_path.trim())
    });
    own.into_iter().chain(nested).collect()
}

#[test]
fn every_available_command_has_a_tui_route_and_every_field_has_help() {
    let expected = paths(&crate::command(), "");
    let actions = catalog::actions();
    let actual: BTreeSet<_> = actions.iter().map(catalog::Action::command).collect();
    assert_eq!(actual, expected);
    assert!(actual.len() >= 124, "command coverage unexpectedly shrank");
    for action in actions {
        for field in &action.fields {
            assert!(
                !field.help.is_empty(),
                "{} :: {} has no help",
                action.command(),
                field.id
            );
        }
    }
}

#[test]
fn forms_preserve_arguments_and_cli_validation() {
    let action = catalog::actions()
        .into_iter()
        .find(|action| action.command() == "providers add")
        .unwrap();
    let form = Form::new(action)
        .with("id", "claude-personal")
        .with("kind", "claude_cli");
    let form = form
        .with("profile-dir", "/profiles/मेरी आवाज़")
        .with("tier", "fast=haiku;smart=sonnet")
        .with("dry-run", "true");
    assert!(form.complete());
    let words: Vec<_> = std::iter::once("retold".to_owned())
        .chain(form.arguments())
        .collect();
    let args = crate::command().try_get_matches_from(words).unwrap();
    let provider = args.subcommand_matches("providers").unwrap();
    let add = provider.subcommand_matches("add").unwrap();
    assert_eq!(
        add.get_one::<String>("profile-dir").unwrap(),
        "/profiles/मेरी आवाज़"
    );
    let tiers = add.get_many::<String>("tier").unwrap();
    assert_eq!(tiers.count(), 2);
    assert!(add.get_flag("dry-run"));
}

/// Walks a stepped form to its last button and presses it on each step.
fn through_every_step(mut app: App) -> App {
    const MOST_STEPS: usize = 6;
    for _ in 0..MOST_STEPS {
        while matches!(app.page(), Some(Page::Form(form)) if !form.on_button()) {
            app.press(key(KeyCode::Down));
        }
        app.press(key(KeyCode::Enter));
        let waiting = app.modal.is_some();
        if waiting {
            break;
        }
    }
    app
}

#[test]
fn writes_wait_for_confirmation_and_cancel_never_dispatches() {
    let mut app = fresh();
    app.open_path("guides create", &vec![("id".into(), "science".into())]);
    let mut app = through_every_step(app);
    assert!(matches!(app.modal, Some(Modal::Confirm(_))));
    assert_eq!(app.jobs(), Vec::new());
    app.press(key(KeyCode::Esc));
    assert_eq!(app.jobs(), Vec::new());
    assert!(matches!(app.page(), Some(Page::Form(form))
        if form.arguments().contains(&"science".to_owned())));
}

#[test]
fn control_s_continues_each_step_then_waits_for_confirmation() {
    let mut app = fresh();
    app.open_path("guides create", &vec![("id".into(), "science".into())]);
    let continue_key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
    let steps = match app.page() {
        Some(Page::Form(form)) => form.steps.len(),
        _ => 0,
    };
    assert!(steps > 1);
    for _ in 1..steps {
        app.press(continue_key);
        assert!(app.modal.is_none());
    }
    app.press(continue_key);
    assert!(matches!(app.modal, Some(Modal::Confirm(_))));
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn invalid_provider_options_stay_in_the_form_for_correction() {
    let mut app = fresh();
    app.open_path("providers add", &vec![("id".into(), "personal".into())]);
    let app = through_every_step(app);
    assert!(app.modal.is_none());
    assert_eq!(app.jobs(), Vec::new());
    assert!(matches!(app.page(), Some(Page::Form(form))
        if form.problem.is_some() && form.arguments().contains(&"personal".to_owned())));
}

#[test]
fn compact_form_keeps_the_selected_provider_kind_visible() {
    let mut app = fresh();
    app.open_path(
        "providers add",
        &vec![("kind".into(), "local_whisper".into())],
    );
    assert!(screen(&app, COMPACT.0, COMPACT.1).contains("local_whisper"));
}

#[test]
fn palette_search_and_section_navigation_are_keyboard_accessible() {
    let mut app = fresh();
    app.press(key(KeyCode::Char('7')));
    assert_eq!(app.section, section::Section::Providers);
    app.press(key(KeyCode::Char(':')));
    let mut app = type_text(app, "providers add");
    app.press(key(KeyCode::Enter));
    assert!(
        matches!(app.page(), Some(Page::Form(form)) if form.action.command() == "providers add")
    );
    app.press(key(KeyCode::Esc));
    assert!(app.page().is_none());
    assert!(!app.quit);
}

#[test]
fn refreshes_are_coalesced_and_preserve_visible_results() {
    let mut store = Store::default();
    let wants = [Want::live("providers list")];
    let first = store.due(&wants, 0);
    store.mark_loading(&first);
    assert_eq!(store.due(&wants, 0), Vec::<Vec<String>>::new());
    store.fill(words("providers list"), result(json!({"providers": []})), 0);
    store.invalidate();
    assert!(store.contents(&words("providers list")).is_some());
    assert_eq!(store.due(&wants, 1), first);
}

#[test]
fn unicode_editing_and_horizontal_scroll_preserve_graphemes() {
    let mut input = Input::with("A👩‍💻हिन्दी");
    input.edit(key(KeyCode::Home));
    input.edit(key(KeyCode::Right));
    input.edit(key(KeyCode::Delete));
    assert_eq!(input.text(), "Aहिन्दी");
    let input = Input::with("界界界界界界");
    let (text, column) = super::view::widgets::input_view(&input, INPUT_COLUMNS);
    assert!(ratatui::text::Span::raw(text).width() < usize::from(INPUT_COLUMNS));
    assert!(column < INPUT_COLUMNS);
}

#[test]
fn every_section_and_theme_renders_at_supported_sizes() {
    for (width, height) in [COMPACT, NORMAL, WIDE] {
        for theme in [
            Theme::Default,
            Theme::Light,
            Theme::HighContrast,
            Theme::NoColor,
        ] {
            let mut app = fresh();
            app.theme = theme;
            for section in section::ALL {
                app.go(section);
                assert!(screen(&app, width, height).contains("Retold"));
            }
        }
    }
}

#[test]
fn long_form_input_keeps_the_cursor_inside_a_compact_terminal() {
    let mut app = fresh();
    let text = "界".repeat(100);
    app.open_path("providers add", &vec![("id".into(), text)]);
    let mut terminal = Terminal::new(TestBackend::new(COMPACT.0, COMPACT.1)).unwrap();
    terminal.draw(super::view::frame(&app)).unwrap();
    let (x, y) = terminal.get_cursor_position().unwrap().into();
    assert!(x < COMPACT.0 && y < COMPACT.1, "cursor escaped to {x},{y}");
}

mod routes;
mod snapshots;

mod discovery;
mod provider_status;

mod money;

mod operations;

mod review;

mod document;

mod workbench;

mod completion;

mod clipboard;
mod files;
mod freshness;
mod geometry;
mod paste;
mod preferences;
mod workspace;

#[test]
fn colour_themes_paint_the_canvas_and_modal_independently_of_terminal_defaults() {
    use ratatui::style::{Color, Style};
    assert_eq!(Theme::NoColor.canvas(), Style::default());
    for theme in [Theme::Default, Theme::Light, Theme::HighContrast] {
        let mut app = fresh();
        app.theme = theme;
        app.modal = Some(Modal::Quit);
        let mut terminal = Terminal::new(TestBackend::new(COMPACT.0, COMPACT.1)).unwrap();
        terminal.draw(super::view::frame(&app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_ne!(buffer[(0, 2)].bg, Color::Reset, "{theme:?} canvas");
        let modal = &buffer[(COMPACT.0 / 2, COMPACT.1 / 2)];
        assert_ne!(modal.bg, Color::Reset, "{theme:?} modal");
        assert_ne!(modal.fg, modal.bg, "{theme:?} contrast");
    }
}
