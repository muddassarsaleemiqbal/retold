//! Typed editing reaches a validated preview and cancellation grants no write authority.
use super::*;
const PICKER_KEY: u8 = 3;

pub(super) fn document() -> App {
    let mut app = fresh();
    app.open_path("guides document", &vec![("id".into(), "example".into())]);
    app.store.fill(words("guides document example"),result(json!({"document":{
        "kind":"guides","id":"example","hash":"review-hash","value":{"id":"example","name":"Original"}
    }})),0);
    app
}
#[test]
fn selecting_field_preserves_hash_and_reviews_typed_change() {
    let mut app = document();
    for (width, height) in [COMPACT, NORMAL, WIDE] {
        assert!(screen(&app, width, height).contains("Original"));
    }
    app.press(key(KeyCode::Enter));
    assert!(matches!(app.modal, Some(Modal::Document(_))));
    app.press(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    let app = type_text(app, "New name");
    let mut app = app;
    app.press(key(KeyCode::Enter));
    let Some(Modal::Preview(preview)) = &app.modal else {
        panic!("review diff");
    };
    assert_eq!(
        preview.form.value("expected-hash").as_deref(),
        Some("review-hash")
    );
    assert_eq!(preview.form.value("value").as_deref(), Some("\"New name\""));
    assert!(preview.words.contains(&"--dry-run".into()));
    assert_eq!(app.jobs(), Vec::new());
}
#[test]
fn cancelling_field_edit_keeps_document_open() {
    let mut app = document();
    app.press(key(KeyCode::Enter));
    app.press(key(KeyCode::Esc));
    assert!(app.modal.is_none());
    assert!(matches!(app.page(), Some(Page::Reading(_))));
    assert_eq!(app.jobs(), Vec::new());
}
#[test]
fn typed_numeric_fields_reject_type_changes() {
    let action = catalog::actions()
        .into_iter()
        .find(|action| action.command() == "guides edit")
        .unwrap();
    let form = Form::new(action);
    let mut editor = crate::tui::document::Editor::new(form, &json!(10));
    editor.input = Input::with("true");
    assert!(editor.reviewed().is_err());
    editor.input = Input::with("20");
    assert_eq!(
        editor.reviewed().unwrap().value("value").as_deref(),
        Some("20")
    );
}

#[test]
fn bundled_example_picker_preserves_draft_and_submits_explicit_choice() {
    let mut app = example_picker();
    assert!(matches!(app.modal, Some(Modal::Values(_))));
    assert_eq!(app.jobs(), Vec::new());
    app.press(key(KeyCode::Esc));
    assert!(
        matches!(app.page(), Some(Page::Form(form)) if form.value("id").as_deref() == Some("recap"))
    );
    app.press(key(KeyCode::F(PICKER_KEY)));
    app.press(key(KeyCode::Enter));
    let mut app = through_every_step(app);
    let Some(Modal::Confirm(form)) = &app.modal else {
        panic!("explicit example confirmation")
    };
    assert_eq!(form.value("example").as_deref(), Some("story-recap"));
    assert!(form.validation_problem().is_none());
    app.press(key(KeyCode::Esc));
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn list_fields_include_empty_arrays_and_preserve_list_type() {
    let fields = crate::tui::document::fields(&json!({"value": {
        "writing":{"examples":[]}, "packaging":{"platforms":["youtube"]}
    }}));
    assert_eq!(
        fields
            .iter()
            .map(|field| field.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "/packaging/platforms",
            "/packaging/platforms/0",
            "/writing/examples"
        ]
    );
    let action = catalog::actions()
        .into_iter()
        .find(|action| action.command() == "guides edit")
        .unwrap();
    let form = Form::new(action).with("field", "/packaging/platforms");
    let mut editor = crate::tui::document::Editor::new(form, &json!(["youtube"]));
    editor.input = Input::with("{\"wrong\": true}");
    assert!(editor.reviewed().is_err());
    editor.input = Input::with("[\"youtube\",\"tiktok\"]");
    assert_eq!(
        editor.reviewed().unwrap().value("value").as_deref(),
        Some("[\"youtube\",\"tiktok\"]")
    );
}

pub(super) fn example_picker() -> App {
    let mut app = fresh();
    app.open_path("guides create", &vec![("id".into(), "recap".into())]);
    while matches!(app.page(), Some(Page::Form(form)) if form.focused_field().is_some_and(|field| field.id != "example"))
    {
        app.press(key(KeyCode::Tab));
    }
    app.press(key(KeyCode::F(PICKER_KEY)));
    app
}

fn omitted_document() -> App {
    let mut app = document();
    app.store.fill(
        words("guides document example"),
        result(json!({"document":{
            "hash":"review-hash", "value":{"name":"Original"},
            "missing":[{"path":"/visuals/panels", "value":0}]
        }})),
        0,
    );
    app
}

#[test]
fn omitted_fields_are_marked_unset_and_keep_the_reviewed_type_and_hash() {
    let mut app = omitted_document();
    for size in [COMPACT, NORMAL, WIDE] {
        assert!(screen(&app, size.0, size.1).contains("visuals/panels  (not set)"));
    }
    app.press(key(KeyCode::Down));
    app.press(key(KeyCode::Enter));
    for size in [COMPACT, NORMAL, WIDE] {
        assert!(screen(&app, size.0, size.1).contains("input is a suggestion"));
    }
    app.press(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    let mut app = type_text(app, "2");
    app.press(key(KeyCode::Enter));
    let Some(Modal::Preview(preview)) = &app.modal else {
        panic!("review optional field");
    };
    assert_eq!(preview.form.value("value").as_deref(), Some("2"));
    assert_eq!(
        preview.form.value("expected-hash").as_deref(),
        Some("review-hash")
    );
    app.press(key(KeyCode::Esc));
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn omitted_object_editor_preserves_object_type() {
    let action = catalog::actions()
        .into_iter()
        .find(|action| action.command() == "guides edit")
        .unwrap();
    let mut editor = crate::tui::document::Editor::new(Form::new(action), &json!({}));
    editor.input = Input::with(r#"{"work_input":"topic","part_input":"topic"}"#);
    assert!(editor.reviewed().is_ok());
    editor.input = Input::with("[]");
    assert!(editor.reviewed().is_err());
}

fn list_document() -> App {
    let mut app = document();
    app.store.fill(words("guides document example"), result(json!({"document":{
        "hash":"review-hash", "value":{"name":"Original", "packaging":{"platforms":["one","two"]}}
    }})), 0);
    app.press(key(KeyCode::Down));
    app.press(key(KeyCode::Down));
    app
}
#[test]
fn list_item_addition_reviews_the_whole_list_and_keeps_existing_items() {
    let mut app = list_document();
    app.press(key(KeyCode::Insert));
    app.press(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    let mut app = type_text(app, "three");
    app.press(key(KeyCode::Enter));
    let Some(Modal::Preview(preview)) = &app.modal else {
        panic!("list review");
    };
    assert_eq!(
        preview.form.value("field").as_deref(),
        Some("/packaging/platforms")
    );
    assert_eq!(
        preview.form.value("value").as_deref(),
        Some(r#"["one","two","three"]"#)
    );
    app.press(key(KeyCode::Esc));
    assert_eq!(app.jobs(), Vec::new());
}
#[test]
fn list_item_removal_reviews_the_remaining_items_without_dispatch() {
    let mut app = list_document();
    app.press(key(KeyCode::Delete));
    app.press(key(KeyCode::Enter));
    let Some(Modal::Preview(preview)) = &app.modal else {
        panic!("list review");
    };
    assert_eq!(preview.form.value("value").as_deref(), Some(r#"["two"]"#));
    app.press(key(KeyCode::Esc));
    assert_eq!(app.jobs(), Vec::new());
}
#[test]
fn list_item_moves_are_reviewed_and_stop_at_the_ends() {
    let mut app = list_document();
    app.press(key(KeyCode::Char('J')));
    app.press(key(KeyCode::Enter));
    let Some(Modal::Preview(preview)) = &app.modal else {
        panic!("list review");
    };
    assert_eq!(
        preview.form.value("value").as_deref(),
        Some(r#"["two","one"]"#)
    );
    app.press(key(KeyCode::Esc));
    assert_eq!(app.jobs(), Vec::new());
    let mut app = list_document();
    app.press(key(KeyCode::Char('K')));
    assert!(app.modal.is_none(), "the first item cannot move up");
}
