//! Row actions preserve exact identifiers and go through normal validation/confirmation.
use super::*;
use crate::tui::{collection, files::Task, runtime::Job};

fn jobs() -> App {
    let mut app = fresh();
    app.go(section::Section::Studio);
    app.store.fill(
        words("queue list"),
        result(json!({"jobs":[
            {"id":"first", "status":"queued"}, {"id":"second", "status":"queued"}
        ]})),
        0,
    );
    app
}

#[test]
fn queue_reorder_reviews_neighbours_and_cancel_dispatches_nothing() {
    let mut app = jobs();
    app.press(key(KeyCode::Char('J')));
    let Some(Page::Form(form)) = app.page() else {
        panic!("move form");
    };
    assert_eq!(form.value("id").as_deref(), Some("second"));
    assert_eq!(form.value("before").as_deref(), Some("first"));
    app.press(key(KeyCode::Esc));
    assert_eq!(app.jobs(), Vec::new());
    app.press(key(KeyCode::Down));
    app.press(key(KeyCode::Char('K')));
    let Some(Page::Form(form)) = app.page() else {
        panic!("move form");
    };
    assert_eq!(form.value("id").as_deref(), Some("second"));
}

#[test]
fn running_queue_neighbour_is_not_reordered() {
    let mut app = jobs();
    app.store.fill(
        words("queue list"),
        result(json!({"jobs":[
            {"id":"first", "status":"running"}, {"id":"second", "status":"queued"}
        ]})),
        0,
    );
    app.press(key(KeyCode::Char('J')));
    assert!(app.page().is_none());
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn audio_preview_uses_saved_directory_and_rejects_traversal() {
    let mut app = fresh();
    app.go(section::Section::Library);
    app.click_tab(1);
    let command = words(collection::MUSIC.list);
    app.store.fill(
        command.clone(),
        result(json!({"directory":"/library/audio/music", "items":[
            {"id":"song", "file_name":"संगीत.wav", "title":"Music"}
        ]})),
        0,
    );
    app.press(key(KeyCode::Char(' ')));
    assert!(
        matches!(&app.jobs()[0], Job::Local {task:Task::Audio(path), ..} if path.to_str() == Some("/library/audio/music/संगीत.wav"))
    );
    app.clear_jobs();
    app.store.fill(
        command,
        result(json!({"directory":"/library/audio/music", "items":[
            {"id":"song", "file_name":"../../secret"}
        ]})),
        0,
    );
    app.press(key(KeyCode::Char(' ')));
    assert_eq!(app.jobs(), Vec::new());
}

pub(super) fn series() -> App {
    let mut app = fresh();
    app.open_path("series show", &vec![("series".into(), "story".into())]);
    app.store.fill(
        words("series show story"),
        result(json!({"series":{
            "work":"Story", "parts":[{"id":"P01", "title":"One", "video_ref":"video-one"},
            {"id":"P02", "title":"Two", "video_ref":null}]
        }, "parts_progress":{"P01":[
            {"stage":"research", "decision":"reused_from_cache"},
            {"stage":"script", "decision":"requires_repair"},
            {"stage":"voice", "decision":"requires_execution_preflight"}
        ], "P02":null}})),
        0,
    );
    app
}

#[test]
fn series_add_parts_opens_the_update_form_for_this_series() {
    let mut app = series();
    app.press(key(KeyCode::Char('a')));
    let Some(Page::Form(form)) = app.page() else {
        panic!("series update");
    };
    assert_eq!(form.value("series").as_deref(), Some("story"));
}

#[test]
fn series_parts_select_plan_and_open_real_video_reference() {
    let mut app = series();
    for (width, height) in [COMPACT, NORMAL, WIDE] {
        assert!(screen(&app, width, height).contains("P01"));
    }
    app.press(key(KeyCode::Enter));
    assert!(matches!(app.page(),Some(Page::Video(video)) if video.reference == "video-one"));
    let mut app = series();
    app.press(key(KeyCode::Down));
    app.press(key(KeyCode::Char('p')));
    let Some(Page::Reading(reading)) = app.page() else {
        panic!("part plan");
    };
    assert_eq!(
        reading.form.as_ref().unwrap().value("parts").as_deref(),
        Some("P02")
    );
}

#[test]
fn music_detail_reports_score_mood_coverage_from_tags() {
    let mut app = fresh();
    app.go(section::Section::Library);
    app.click_tab(1);
    app.store.fill(
        words(collection::MUSIC.list),
        result(json!({"directory":"/library/audio/music", "items":[
            {"id":"song", "title":"Night", "tags":["Tense", "dark"]}
        ]})),
        0,
    );
    let shown = screen(&app, WIDE.0, WIDE.1);
    assert!(shown.contains("Mood coverage"), "{shown}");
    assert!(
        shown.contains("✓ tense") && shown.contains("✓ dark"),
        "{shown}"
    );
    assert!(shown.contains("✗ calm"), "{shown}");
    assert!(
        shown.contains("6 score moods have no tagged track"),
        "{shown}"
    );
}

const SUPERVISION_TAB: usize = 4;

pub(super) fn processes() -> App {
    let mut app = fresh();
    app.go(section::Section::Money);
    app.click_tab(SUPERVISION_TAB);
    app.store.fill(
        words(collection::PROCESSES.list),
        result(json!({
            "processes": [{"id":"owner-one", "status":"unconfirmed",
                "owner":{"pid":42,"purpose":"render job","since":"2026-10-22T00:00:00Z"},
                "worker":{"pid":43},"worker_observation":"different", "supervisor":null}],
            "next_after":"cursor with spaces"
        })),
        0,
    );
    app
}

#[test]
fn supervision_inspection_preserves_uncertainty_and_cannot_dispatch_actions() {
    let mut app = processes();
    for size in [COMPACT, NORMAL, WIDE] {
        let rendered = screen(&app, size.0, size.1);
        assert!(rendered.contains("unconfirmed"), "{rendered}");
        assert!(rendered.contains("different"), "{rendered}");
        assert!(rendered.contains("Inspection only"), "{rendered}");
    }
    for code in [KeyCode::Enter, KeyCode::Char('x'), KeyCode::Char('b')] {
        app.press(key(code));
        assert!(app.page().is_none());
        assert_eq!(app.jobs(), Vec::new());
    }
}

#[test]
fn supervision_and_attempt_pagination_are_independent_and_bounded() {
    let mut app = processes();
    app.account_pages.attempts.push("billing cursor".into());
    app.press(key(KeyCode::Char('>')));
    let command = app.collection_words(&collection::PROCESSES);
    assert_eq!(
        command,
        words("account processes --limit 100 --after")
            .into_iter()
            .chain(["cursor with spaces".into()])
            .collect::<Vec<_>>()
    );
    assert!(app.wants().iter().any(|want| want.words == command));
    app.store.fill(
        command,
        result(json!({"processes":[],"next_after":null})),
        0,
    );
    app.press(key(KeyCode::Char('>')));
    assert_eq!(app.account_pages.processes.len(), 1);
    app.press(key(KeyCode::Char('<')));
    assert_eq!(app.account_pages.processes, Vec::<String>::new());
    assert_eq!(app.account_pages.attempts, vec!["billing cursor"]);
}

#[test]
fn supervision_filter_keeps_selected_record_identity() {
    let mut app = processes();
    app.press(key(KeyCode::Char('/')));
    app = type_text(app, "owner-one");
    app.press(key(KeyCode::Enter));
    assert_eq!(app.selected_item().unwrap().key, "owner-one");
    assert_eq!(app.jobs(), Vec::new());
}
