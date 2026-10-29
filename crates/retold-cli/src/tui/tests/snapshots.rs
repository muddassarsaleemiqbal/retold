//! Golden terminal frames at each supported breakpoint.
use super::{COMPACT, NORMAL, WIDE, fresh, screen};
use crate::tui::app::App;
use crate::tui::theme::Theme;

fn provider_form() -> App {
    let mut app = fresh();
    app.open_path("providers add", &Vec::new());
    app
}

fn snapshot(name: &str, actual: &str, expected: &str) {
    if std::env::var_os("RETOLD_UPDATE_TUI_SNAPSHOTS").is_some() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = root.join("src/tui/tests/snapshots").join(name);
        std::fs::write(path, actual).unwrap();
    } else {
        assert_eq!(actual, expected, "terminal snapshot changed: {name}");
    }
}

#[test]
fn compact_home() {
    let app = fresh();
    let actual = screen(&app, COMPACT.0, COMPACT.1);
    snapshot(
        "home-80.txt",
        &actual,
        include_str!("snapshots/home-80.txt"),
    );
}

#[test]
fn normal_provider_form() {
    let app = provider_form();
    let actual = screen(&app, NORMAL.0, NORMAL.1);
    snapshot(
        "provider-120.txt",
        &actual,
        include_str!("snapshots/provider-120.txt"),
    );
}

#[test]
fn wide_dark_provider_form() {
    let mut app = provider_form();
    app.theme = Theme::Default;
    let actual = screen(&app, WIDE.0, WIDE.1);
    snapshot(
        "provider-200.txt",
        &actual,
        include_str!("snapshots/provider-200.txt"),
    );
}

#[test]
fn compact_provider_status() {
    let app = super::provider_status::status_app();
    let actual = screen(&app, COMPACT.0, COMPACT.1);
    snapshot(
        "provider-status-80.txt",
        &actual,
        include_str!("snapshots/provider-status-80.txt"),
    );
}

#[test]
fn compact_provider_discovery() {
    let app = super::discovery::discovery_app();
    let actual = screen(&app, COMPACT.0, COMPACT.1);
    snapshot(
        "provider-discovery-80.txt",
        &actual,
        include_str!("snapshots/provider-discovery-80.txt"),
    );
}

#[test]
fn compact_money_limits() {
    let mut app = fresh();
    app.go(crate::tui::section::Section::Money);
    app.click_tab(1);
    app.store.fill(super::words("account show"),super::result(serde_json::json!({"limits":[
        {"scope":{"kind":"account"},"window":"day","resources":{"usd_micros":5_000_000,"requests":100}}
    ]})),0);
    snapshot(
        "money-80.txt",
        &screen(&app, COMPACT.0, COMPACT.1),
        include_str!("snapshots/money-80.txt"),
    );
}
#[test]
fn normal_structured_document() {
    let app = super::document::document();
    snapshot(
        "document-120.txt",
        &screen(&app, NORMAL.0, NORMAL.1),
        include_str!("snapshots/document-120.txt"),
    );
}
#[test]
fn wide_series_parts() {
    let mut app = super::operations::series();
    app.theme = Theme::Default;
    snapshot(
        "series-200.txt",
        &screen(&app, WIDE.0, WIDE.1),
        include_str!("snapshots/series-200.txt"),
    );
}

#[test]
fn guide_example_picker_at_supported_breakpoints() {
    let app = super::document::example_picker();
    for (size, name, expected) in [
        (
            COMPACT,
            "guide-example-80.txt",
            include_str!("snapshots/guide-example-80.txt"),
        ),
        (
            NORMAL,
            "guide-example-120.txt",
            include_str!("snapshots/guide-example-120.txt"),
        ),
        (
            WIDE,
            "guide-example-200.txt",
            include_str!("snapshots/guide-example-200.txt"),
        ),
    ] {
        snapshot(name, &screen(&app, size.0, size.1), expected);
    }
}

/// The Videos list in each data state a person can meet: still loading, empty and failed.
fn videos(state: Option<retold_app::protocol::Payload>) -> App {
    let mut app = App::new(Theme::Default);
    app.go(crate::tui::section::Section::Videos);
    if let Some(payload) = state {
        app.store.fill(super::words("video list"), payload, 0);
    }
    app
}

#[test]
fn compact_videos_loading_empty_and_failed_states() {
    use retold_app::protocol::{Diagnostic, ErrorCode};
    let failed = Diagnostic::new(ErrorCode::InputRequired, "The library could not be read.");
    for (state, name, expected) in [
        (
            None,
            "videos-loading-80.txt",
            include_str!("snapshots/videos-loading-80.txt"),
        ),
        (
            Some(super::result(serde_json::json!({"items":[]}))),
            "videos-empty-80.txt",
            include_str!("snapshots/videos-empty-80.txt"),
        ),
        (
            Some(failed.into_payload()),
            "videos-failed-80.txt",
            include_str!("snapshots/videos-failed-80.txt"),
        ),
    ] {
        let mut app = videos(state);
        app.theme = Theme::Default;
        snapshot(name, &screen(&app, COMPACT.0, COMPACT.1), expected);
    }
}

/// The README capture is short enough to read inline.
const README: (u16, u16) = (100, 12);

/// The capture shown in the README; regenerate it with the other snapshots.
#[test]
fn readme_series_capture() {
    let mut app = super::operations::series();
    app.theme = Theme::Default;
    snapshot(
        "readme-series-100.txt",
        &screen(&app, README.0, README.1),
        include_str!("snapshots/readme-series-100.txt"),
    );
}

#[test]
fn compact_supervision_evidence() {
    let app = super::operations::processes();
    snapshot(
        "processes-80.txt",
        &screen(&app, COMPACT.0, COMPACT.1),
        include_str!("snapshots/processes-80.txt"),
    );
}
