//! Review filters preserve the selected artifact's identity through actions.
use super::*;
use crate::tui::{app::review::asset_words, page::VideoPage};

pub(super) fn review() -> App {
    let mut app = fresh();
    app.open_video("story");
    app.store.fill(asset_words("story"), result(json!({"items":[
        {"id":"one", "name":"One", "path":"frames/one.png", "is_immutable":false, "sha256":"same", "bytes":10},
        {"id":"two", "name":"Two", "path":"frames/two.png", "is_immutable":true, "sha256":"other", "bytes":20}
    ]})),0);
    app.store.fill(
        words("video review story"),
        result(json!({"items":[
            {"item":"frames/one.png", "locked":true, "stale":true}
        ]})),
        0,
    );
    let mut video = VideoPage::new("story");
    video.tab = crate::tui::app::review::REVIEW_TAB;
    // Public key routing enters the same Review tab as a user.
    app.press(key(KeyCode::Tab));
    app.press(key(KeyCode::Tab));
    assert_eq!(app.page().unwrap().title(), video.reference);
    app
}

#[test]
fn filters_show_saved_stale_pending_and_immutable_take_evidence() {
    let mut app = review();
    assert_eq!(app.review_assets().len(), 2);
    app.press(key(KeyCode::Char('f')));
    assert_eq!(app.review_assets().len(), 2);
    app.press(key(KeyCode::Char('f')));
    assert_eq!(app.review_assets()[0]["id"], "one");
    assert_eq!(app.review_assets().len(), 1);
    app.press(key(KeyCode::Char('f')));
    assert_eq!(app.review_assets()[0]["id"], "two");
    app.press(key(KeyCode::Char('a')));
    let Some(Page::Form(form)) = app.page() else {
        panic!("approval");
    };
    assert_eq!(form.value("target").as_deref(), Some("frames/two.png"));
    assert_eq!(app.running.len(), 0);
}

#[test]
fn comparison_survives_selection_and_renders_at_all_breakpoints() {
    let mut app = review();
    app.press(key(KeyCode::Char('v')));
    app.press(key(KeyCode::Down));
    for (width, height) in [COMPACT, NORMAL, WIDE] {
        let actual = screen(&app, width, height);
        assert!(actual.contains("Compare with One"));
        assert!(actual.contains("different file content"));
    }
    app.press(key(KeyCode::Char('v')));
    assert!(matches!(app.page(),Some(Page::Video(video)) if video.comparison.is_none()));
}

fn multilingual_review() -> App {
    let mut app = review();
    app.store.fill(
        words("video show story"),
        result(json!({"video":{"editions":["en", "ur", "hi"]}})),
        0,
    );
    app.store.fill(
        asset_words("story"),
        result(json!({"items":[
            {"id":"english", "path":"work/06-voice/en/line.wav"},
            {"id":"urdu", "path":"work/06-voice/ur/line.wav"},
            {"id":"shared", "path":"work/04-images/frame.png"}
        ]})),
        0,
    );
    app.store.fill(
        words("video paths story"),
        result(json!({"paths":{"root":"/saved"}})),
        0,
    );
    app
}

#[test]
fn language_filter_cycles_editions_and_actions_keep_the_selected_path() {
    let mut app = multilingual_review();
    app.press(key(KeyCode::Char('L')));
    assert_eq!(app.review_assets()[0]["id"], "english");
    assert_eq!(app.review_assets().len(), 1);
    for size in [COMPACT, NORMAL, WIDE] {
        assert!(screen(&app, size.0, size.1).contains("language en"));
    }
    app.press(key(KeyCode::Char('L')));
    assert_eq!(app.review_assets()[0]["id"], "urdu");
    app.press(key(KeyCode::Char('n')));
    let Some(Page::Form(form)) = app.page() else {
        panic!("note form");
    };
    assert_eq!(
        form.value("item").as_deref(),
        Some("work/06-voice/ur/line.wav")
    );
}

#[test]
fn empty_language_filter_clears_preview_and_rejects_late_worker_reply() {
    let mut app = multilingual_review();
    app.press(key(KeyCode::Char('L')));
    let old_path = app.media.requested.clone();
    let old_path = old_path.unwrap();
    app.media.preview = Some(result(json!({"text":"old English preview"})));
    app.press(key(KeyCode::Char('L')));
    app.press(key(KeyCode::Char('L')));
    assert_eq!(app.review_assets(), Vec::<Value>::new());
    assert!(app.media.preview.is_none());
    assert!(app.media.requested.is_none());
    app.receive(
        crate::tui::runtime::Job::Local {
            id: 0,
            task: crate::tui::files::Task::Preview(old_path),
        },
        result(json!({"text":"late English preview"})),
    );
    assert!(app.media.preview.is_none());
    assert!(screen(&app, NORMAL.0, NORMAL.1).contains("No artifacts match"));
    app.press(key(KeyCode::Char('a')));
    assert!(matches!(app.page(), Some(Page::Video(_))));
    app.press(key(KeyCode::Char('L')));
    assert_eq!(app.review_assets().len(), 3);
}

#[test]
fn redo_scope_names_the_saved_stage_and_item() {
    use crate::tui::app::review::redo_scope;
    let scope = |path| redo_scope(path);
    assert_eq!(
        scope("work/05-images/shot-003.png"),
        vec![
            ("stage".into(), "images".into()),
            ("items".into(), "3".into())
        ]
    );
    assert_eq!(
        scope("/lib/v/work/07-voice/line.wav"),
        vec![("stage".to_owned(), "voice".to_owned())]
    );
    assert_eq!(scope("frames/one.png"), Vec::new());
}

#[test]
fn redo_from_review_previews_descendants_before_applying() {
    let mut app = review();
    app.store.fill(
        asset_words("story"),
        result(json!({"items":[
            {"id":"one", "name":"One", "path":"work/05-images/shot-002.png", "is_immutable":false}
        ]})),
        0,
    );
    app.press(key(KeyCode::Char('r')));
    let Some(Page::Form(form)) = app.page() else {
        panic!("redo form");
    };
    assert_eq!(form.value("stage").as_deref(), Some("images"));
    assert_eq!(form.value("items").as_deref(), Some("2"));
    assert!(crate::tui::preview::Preview::new(form).is_some());
}

#[test]
fn edit_from_review_targets_the_selected_frame() {
    let mut app = review();
    app.store.fill(
        asset_words("story"),
        result(json!({"items":[
            {"id":"one", "name":"One", "path":"work/05-images/shot-004.png", "is_immutable":false}
        ]})),
        0,
    );
    app.press(key(KeyCode::Char('e')));
    let Some(Page::Form(form)) = app.page() else {
        panic!("edit form");
    };
    assert_eq!(form.action.command(), "video edit shot");
    assert_eq!(form.value("frame").as_deref(), Some("4"));
}
