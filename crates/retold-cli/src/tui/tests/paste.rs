//! Pasted text belongs to the focused input and never grants execution authority.
use super::*;
use ratatui::crossterm::event::Event;

const BROWSER_ID: u64 = 7;
const STALE_SELECTION: usize = 8;
const OVERSIZED_BYTES: usize = 65_537;
const MAX_INPUT_BYTES: usize = 65_536;

fn paste(mut app: App, text: &str) -> App {
    app.react(&Event::Paste(text.into()));
    app
}

fn provider_form() -> App {
    let mut app = fresh();
    app.open_path("providers add", &Vec::new());
    app
}

#[test]
fn paste_edits_the_form_without_navigation_or_submission() {
    let mut app = provider_form();
    app = paste(app, "मेरी آواز़\r\nnext\t\x03\x13\x1b\0");
    let Some(Page::Form(form)) = app.page() else {
        panic!("the form must remain open");
    };
    assert_eq!(form.value("id").as_deref(), Some("मेरी آواز़ next"));
    assert_eq!(form.focus, 0);
    assert!(app.modal.is_none());
    assert!(!app.quit);
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn pasted_unicode_inserts_at_the_grapheme_cursor() {
    let mut app = provider_form();
    app = paste(app, "A👨‍👩‍👧‍👦Z");
    app.press(key(KeyCode::Left));
    app = paste(app, "e\u{301}اردو");
    app.press(key(KeyCode::Backspace));
    let Some(Page::Form(form)) = app.page() else {
        panic!("form");
    };
    assert_eq!(form.value("id").as_deref(), Some("A👨‍👩‍👧‍👦e\u{301}اردZ"));
    assert!(screen(&app, COMPACT.0, COMPACT.1).contains("👨‍👩‍👧‍👦"));
}

#[test]
fn pasted_palette_and_value_filters_reset_selection_without_choosing() {
    let mut app = fresh();
    for modal in [
        Modal::Palette(crate::tui::page::Palette::new(Vec::new())),
        Modal::Values(crate::tui::page::Palette::new(Vec::new())),
    ] {
        app.modal = Some(modal);
        app = paste(app, "providers\n");
        let Some(Modal::Palette(palette) | Modal::Values(palette)) = &app.modal else {
            panic!("pasting must keep the picker open");
        };
        assert_eq!(palette.query.text(), "providers ");
        assert_eq!(palette.selected, 0);
        assert_eq!(app.jobs(), Vec::new());
    }
}

#[test]
fn pasted_document_draft_keeps_review_hash_and_requires_review() {
    let mut app = super::document::document();
    app.press(key(KeyCode::Enter));
    app.press(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app = paste(app, "New\nname");
    let Some(Modal::Document(editor)) = &app.modal else {
        panic!("document editor");
    };
    assert_eq!(editor.input.text(), "New name");
    let form = editor.reviewed().unwrap();
    assert_eq!(form.value("expected-hash").as_deref(), Some("review-hash"));
    assert_eq!(form.value("value").as_deref(), Some("\"New name\""));
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn pasted_route_only_edits_an_active_addition() {
    let mut app = fresh();
    let mut editor = crate::tui::routes::Editor::new("text.smart", &json!([]), &json!({}));
    editor.adding = Some(Input::default());
    app.modal = Some(Modal::Route(editor));
    app = paste(app, "claude:smart\n");
    let Some(Modal::Route(editor)) = &app.modal else {
        panic!("route editor");
    };
    assert_eq!(editor.adding.as_ref().unwrap().text(), "claude:smart ");
    assert_eq!(editor.chain, Vec::<String>::new());
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn pasted_browser_filter_discards_stale_preview_and_selection() {
    let mut app = fresh();
    let mut browser = crate::tui::files::Browser::new(BROWSER_ID, ".".into(), Option::default());
    browser.selected = STALE_SELECTION;
    browser.preview = Some(result(json!({"text":"old preview"})));
    app.modal = Some(Modal::Browser(browser));
    app = paste(app, "o \n");
    let Some(Modal::Browser(browser)) = &app.modal else {
        panic!("browser");
    };
    assert_eq!(browser.query.text(), "o  ");
    assert_eq!(browser.selected, 0);
    assert!(browser.preview.is_none());
    assert!(
        app.jobs().is_empty(),
        "paste must never open a host program"
    );
}

#[test]
fn paste_filters_landing_and_stream_only_when_their_input_is_active() {
    let mut app = fresh();
    app = paste(app, "7ny\n");
    assert_eq!(app.section, section::Section::Home);
    assert_eq!(app.jobs(), Vec::new());
    app.press(key(KeyCode::Char('7')));
    app.press(key(KeyCode::Char('/')));
    app = paste(app, "needle");
    assert_eq!(app.landing().filter(), "needle");
    app.press(key(KeyCode::Esc));
    app.open_path("queue attach", &vec![("id".into(), "example".into())]);
    app.press(key(KeyCode::Char('/')));
    app = paste(app, "event\n");
    let Some(Page::Reading(reading)) = app.page() else {
        panic!("event stream");
    };
    let stream = reading.stream.as_ref().unwrap();
    assert!(stream.filtering);
    assert_eq!(stream.filter.text(), "event ");
}

#[test]
fn paste_never_confirms_a_write_or_bypasses_a_typed_confirmation() {
    let mut app = provider_form();
    let Some(Page::Form(form)) = app.page().cloned() else {
        panic!("form");
    };
    app.modal = Some(Modal::Confirm(form.clone()));
    app = paste(app, "y\n\x13");
    assert!(matches!(app.modal, Some(Modal::Confirm(_))));
    app.modal = Some(Modal::Destroy {
        form,
        typed: Input::default(),
    });
    app = paste(app, "DELETE\n");
    let Some(Modal::Destroy { typed, .. }) = &app.modal else {
        panic!("typed confirmation");
    };
    assert_eq!(typed.text(), "DELETE ");
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn oversized_paste_is_refused_atomically_and_explained() {
    let mut app = provider_form();
    app = paste(app, "kept");
    app = paste(app, &"a".repeat(OVERSIZED_BYTES));
    let Some(Page::Form(form)) = app.page() else {
        panic!("form");
    };
    assert_eq!(form.value("id").as_deref(), Some("kept"));
    assert!(app.toasts.iter().any(|toast| toast.text.contains("64 KiB")));
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn repeated_pastes_cannot_exceed_the_total_field_bound() {
    let mut app = provider_form();
    let maximum = "a".repeat(MAX_INPUT_BYTES);
    app = paste(app, &maximum);
    app = paste(app, "b");
    let Some(Page::Form(form)) = app.page() else {
        panic!("form");
    };
    assert_eq!(form.value("id").as_deref(), Some(maximum.as_str()));
    assert!(app.toasts.iter().any(|toast| toast.text.contains("64 KiB")));
}

#[test]
fn paste_preserves_filename_spaces_and_joined_graphemes() {
    let mut input = Input::with("👩");
    input.paste("\u{200d}💻\u{a0}name\u{202f}end");
    assert_eq!(input.text(), "👩‍💻\u{a0}name\u{202f}end");
    input.edit(key(KeyCode::Home));
    input.edit(key(KeyCode::Delete));
    assert_eq!(input.text(), "\u{a0}name\u{202f}end");
}

#[test]
fn nonediting_modals_own_paste_and_leave_the_underlying_draft_untouched() {
    let mut app = provider_form();
    app = paste(app, "kept");
    for modal in [Modal::Help, Modal::Quit, Modal::Setup(0)] {
        app.modal = Some(modal);
        app = paste(app, "y\n\x13");
        assert!(app.modal.is_some());
        assert!(!app.quit);
        assert_eq!(app.jobs(), Vec::new());
    }
    let Some(Page::Form(form)) = app.page() else {
        panic!("form");
    };
    assert_eq!(form.value("id").as_deref(), Some("kept"));
    app.modal = Some(Modal::BulkTag(Input::default()));
    app = paste(app, "tag\n");
    assert!(matches!(&app.modal, Some(Modal::BulkTag(input)) if input.text() == "tag "));
    assert_eq!(app.jobs(), Vec::new());
}
