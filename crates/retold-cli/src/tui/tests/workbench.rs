//! Common workbench paths and render budgets on a thousand-video library.
use super::*;
use crate::tui::runtime::Job;
const LIBRARY_SIZE: usize = 1_000;
const FRAME_SAMPLES: usize = 30;
const FIRST_FRAME_MILLIS: u128 = 150;
const FRAME_MICROS: u128 = 16_000;
/// Shared CI runners are slower and noisier than a developer host.
const CI_ALLOWANCE: u128 = 3;

fn budget(local: u128) -> u128 {
    if std::env::var_os("CI").is_some() {
        local * CI_ALLOWANCE
    } else {
        local
    }
}

fn videos() -> App {
    let mut app = fresh();
    app.go(section::Section::Videos);
    app.store.fill(
        words("video list"),
        result(
            json!({"items":[{"ref":"first","title":"First"},{"ref":"second","title":"Second"}]}),
        ),
        0,
    );
    app
}
#[test]
fn bulk_selection_survives_cancel_and_dispatches_exact_reviewed_references() {
    let mut app = videos();
    app.press(key(KeyCode::Char(' ')));
    app.press(key(KeyCode::Down));
    app.press(key(KeyCode::Char(' ')));
    app.press(key(KeyCode::Char('A')));
    assert!(screen(&app, COMPACT.0, COMPACT.1).contains("2 selected videos"));
    app.press(key(KeyCode::Esc));
    assert_eq!(app.selected_videos.len(), 2);
    assert_eq!(app.jobs(), Vec::new());
    app.press(key(KeyCode::Char('A')));
    app.press(key(KeyCode::Enter));
    let references: Vec<_> = app
        .jobs()
        .into_iter()
        .filter_map(|job| match job {
            Job::Action { words, .. } => words.last().cloned(),
            _ => None,
        })
        .collect();
    assert_eq!(references, vec!["first", "second"]);
    assert_eq!(app.selected_videos.len(), 0);
}
#[test]
fn system_reports_are_read_only_until_repair_is_explicitly_confirmed() {
    let mut app = fresh();
    app.go(section::Section::System);
    app.click_tab(1);
    app.store.fill(
        words("doctor"),
        result(json!({"report":{"checks":[
            {"name":"workspace","status":"warn","message":"Missing folder","fixable":true}
        ]}})),
        0,
    );
    assert!(screen(&app, COMPACT.0, COMPACT.1).contains("repair available"));
    assert_eq!(app.jobs(), Vec::new());
    app.press(key(KeyCode::Char('f')));
    assert!(
        matches!(app.page(),Some(Page::Form(form)) if form.arguments() == words("doctor --fix"))
    );
    assert_eq!(app.jobs(), Vec::new());
}
#[test]
fn attempt_pagination_uses_exact_cursor_and_returns_to_first_page() {
    let mut app = fresh();
    app.go(section::Section::Money);
    app.click_tab(2);
    app.store.fill(
        words("account attempts --limit 200"),
        result(json!({"account_attempts":{"attempts":[],"next_after":"cursor with spaces"}})),
        0,
    );
    app.press(key(KeyCode::Char('>')));
    let command = app.collection_words(&crate::tui::collection::ATTEMPTS);
    assert_eq!(
        command.last().map(String::as_str),
        Some("cursor with spaces")
    );
    assert!(app.wants().iter().any(|want| want.words == command));
    app.press(key(KeyCode::Char('<')));
    assert_eq!(app.account_pages.attempts.len(), 0);
}
#[test]
fn thousand_video_render_stays_within_documented_host_budgets() {
    let mut app = fresh();
    app.go(section::Section::Videos);
    let items: Vec<_> = (0..LIBRARY_SIZE)
        .map(|index| json!({"ref":format!("video-{index}"),"title":format!("Video {index}")}))
        .collect();
    app.store
        .fill(words("video list"), result(json!({"items":items})), 0);
    let started = std::time::Instant::now();
    screen(&app, NORMAL.0, NORMAL.1);
    let first = started.elapsed().as_millis();
    assert!(first < budget(FIRST_FRAME_MILLIS), "first frame {first} ms");
    let mut durations = Vec::new();
    for _ in 0..FRAME_SAMPLES {
        let started = std::time::Instant::now();
        app.press(key(KeyCode::Down));
        screen(&app, NORMAL.0, NORMAL.1);
        durations.push(started.elapsed().as_micros());
    }
    durations.sort_unstable();
    let median = durations[FRAME_SAMPLES / 2];
    eprintln!("TUI host budget: first={first}ms median={median}us samples={FRAME_SAMPLES}");
    assert!(median < budget(FRAME_MICROS), "median frame {median} us");
}

#[test]
fn mouse_wheel_moves_document_selection_and_keeps_modal_ownership() {
    use ratatui::crossterm::event::{Event, MouseEvent, MouseEventKind};
    let mut app = super::document::document();
    app.store.fill(
        words("guides document example"),
        result(json!({"document": {
            "kind":"guides", "id":"example", "hash":"review-hash",
            "value":{"name":"Original", "purpose":"Explain"}
        }})),
        0,
    );
    let wheel = Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: NORMAL.0 / 2,
        row: NORMAL.1 / 2,
        modifiers: KeyModifiers::NONE,
    });
    app.react(&wheel);
    assert!(matches!(app.page(),Some(Page::Reading(reading)) if reading.scroll == 1));
    app.press(key(KeyCode::Enter));
    assert!(matches!(app.modal, Some(Modal::Document(_))));
    app.react(&wheel);
    assert!(matches!(app.page(),Some(Page::Reading(reading)) if reading.scroll == 1));
    assert_eq!(app.jobs(), Vec::new());
}
