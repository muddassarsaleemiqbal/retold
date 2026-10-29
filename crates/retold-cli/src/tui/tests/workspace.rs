//! Root changes cannot abandon tracked work or carry draft authority into another workspace.
use super::*;
use crate::tui::{files::Task, runtime::Job};
use std::path::PathBuf;

const DESTINATION: &str = "/fixture/second-workspace";
const OLD_ROOT: &str = "/fixture/first-workspace";
const NOW: u64 = 100;

fn pending_switch() -> App {
    let mut app = fresh();
    app.workspace = Some(OLD_ROOT.into());
    app.open_path(
        "workspace switch",
        &vec![("path".into(), DESTINATION.into())],
    );
    let mut app = through_every_step(app);
    assert!(screen(&app, NORMAL.0, NORMAL.1).contains("unsaved TUI drafts"));
    app.press(key(KeyCode::Enter));
    assert_eq!(app.running.len(), 1);
    app
}

fn switched() -> Payload {
    result(json!({"workspace":{"root":DESTINATION, "name":"Second workspace"}}))
}

#[test]
fn switching_blocks_new_mutations_and_preserves_the_rejected_form() {
    let mut app = pending_switch();
    app.open_path(
        "guides create",
        &vec![("id".into(), "preserved-draft".into())],
    );
    let mut app = through_every_step(app);
    app.press(key(KeyCode::Enter));
    assert_eq!(app.jobs().len(), 1);
    assert!(matches!(app.page(), Some(Page::Form(form))
        if form.value("id").as_deref() == Some("preserved-draft")));
    assert!(app.is_switching_workspace());
}

#[test]
fn a_switch_waits_for_pending_clipboard_completion_before_replacing_the_worker() {
    let mut app = pending_switch();
    let switching = app.jobs()[0].clone();
    app.press(key(KeyCode::Char('y')));
    let clipboard = app.jobs()[1].clone();
    app.receive(switching, switched());
    assert!(app.workspace_ready().is_none());
    assert_eq!(
        app.workspace.as_deref(),
        Some(std::path::Path::new(OLD_ROOT))
    );
    assert_eq!(app.running.len(), 1);
    app.receive(clipboard, result(json!({"copied":true})));
    assert_eq!(
        app.workspace_ready(),
        Some(std::path::Path::new(DESTINATION))
    );
}

#[test]
fn a_quit_waiting_for_the_switch_is_not_erased_by_reinitialization() {
    let mut app = pending_switch();
    let switching = app.jobs()[0].clone();
    app.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    app.press(key(KeyCode::Enter));
    app.receive(switching, switched());
    assert!(app.quit);
    assert!(app.workspace_ready().is_none());
}

fn dirty_workspace() -> App {
    let mut app = with_old_preview(display_ready());
    app.workspace = Some(OLD_ROOT.into());
    app.defaults = json!({"last":{"guide":"old-guide"}});
    app.selected_videos = BTreeSet::from(["old-video".into()]);
    app.account_pages = crate::tui::app::AccountPages {
        attempts: vec!["old-attempt-cursor".into()],
        processes: vec!["old-process-cursor".into()],
    };
    app.go(section::Section::Providers);
    app.open_path(
        "providers add",
        &vec![("id".into(), "old-provider-draft".into())],
    );
    app
}

#[test]
fn entering_the_new_root_resets_every_section_and_loads_only_its_preferences() {
    let mut app = dirty_workspace();
    app.enter_workspace(PathBuf::from(DESTINATION));
    assert_eq!(app.workspace, Some(PathBuf::from(DESTINATION)));
    assert_eq!(app.section, section::Section::Home);
    assert_eq!(app.theme, Theme::Light);
    assert_eq!(app.now, NOW);
    assert_eq!(app.area.width, NORMAL.0);
    assert_eq!(
        app.jobs(),
        vec![Job::Local {
            id: 0,
            task: Task::LoadDefaults(Some(DESTINATION.into())),
        }]
    );
    assert_workspace_reset(&app);
    for section in section::ALL {
        app.go(section);
        assert!(app.page().is_none());
        assert_eq!(app.landing().filter(), "");
        assert_eq!(app.landing().selected(), 0);
    }
}

fn assert_workspace_reset(app: &App) {
    assert_eq!(app.defaults, json!({}));
    assert_eq!(app.selected_videos, BTreeSet::new());
    assert_eq!(app.running, BTreeSet::new());
    assert_eq!(app.account_pages.attempts, Vec::<String>::new());
    assert_eq!(app.account_pages.processes, Vec::<String>::new());
    assert!(app.media.requested.is_none());
    assert!(app.media.preview.is_none());
    assert!(app.host.clipboard.is_none());
    assert!(app.switch_to.is_none());
    assert!(app.store.contents(&words("providers list")).is_none());
}

#[test]
fn failed_switch_preserves_old_workspace_drafts_and_preferences() {
    let mut app = dirty_workspace();
    app.go(section::Section::System);
    app.open_path(
        "workspace switch",
        &vec![("path".into(), DESTINATION.into())],
    );
    let mut app = through_every_step(app);
    app.press(key(KeyCode::Enter));
    let switching = app.jobs()[0].clone();
    let failure = retold_app::protocol::Diagnostic::new(
        retold_app::protocol::ErrorCode::InputRequired,
        "Workspace unavailable",
    )
    .into_payload();
    app.receive(switching, failure);
    assert!(!app.is_switching_workspace());
    assert!(app.workspace_ready().is_none());
    assert_eq!(app.defaults["last"]["guide"], "old-guide");
    assert!(app.selected_videos.contains("old-video"));
    app.go(section::Section::Providers);
    assert!(matches!(app.page(), Some(Page::Form(form))
        if form.value("id").as_deref() == Some("old-provider-draft")));
}

fn display_ready() -> App {
    let mut app = fresh();
    app.theme = Theme::Light;
    app.now = NOW;
    app.area = ratatui::layout::Rect::new(0, 0, NORMAL.0, NORMAL.1);
    app
}

fn with_old_preview(mut app: App) -> App {
    app.media.requested = Some("/old/preview.png".into());
    app.media.preview = Some(result(json!({"text":"old preview"})));
    app
}
