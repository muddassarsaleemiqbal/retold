//! Delayed saved choices fill untouched draft fields without overwriting intentional edits.
use super::*;
use crate::tui::{defaults, files::Task, runtime::Job};

fn preference_reply(mut app: App) -> App {
    app.receive(
        Job::Local {
            id: 0,
            task: Task::LoadDefaults(Option::default()),
        },
        result(json!({"defaults":{"last":{"guide":"saved-guide", "channel":"saved-channel"}}})),
    );
    app
}

fn draft() -> App {
    let mut app = fresh();
    app.open_path("new", &vec![("work".into(), "Work in progress".into())]);
    app
}

#[test]
fn late_preferences_fill_a_draft_that_opened_before_the_read_finished() {
    let mut app = draft();
    app.press(key(KeyCode::Tab));
    let focus = match app.page() {
        Some(Page::Form(form)) => form.focus,
        _ => panic!("draft"),
    };
    app = preference_reply(app);
    let Some(Page::Form(form)) = app.page() else {
        panic!("draft")
    };
    assert_eq!(form.value("guide").as_deref(), Some("saved-guide"));
    assert_eq!(form.value("channel").as_deref(), Some("saved-channel"));
    assert_eq!(form.value("work").as_deref(), Some("Work in progress"));
    assert_eq!(form.focus, focus);
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn late_preferences_preserve_a_guide_the_user_intentionally_cleared() {
    let app = type_text(draft(), "temporary-guide");
    let mut app = app;
    app.press(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app = preference_reply(app);
    let Some(Page::Form(form)) = app.page() else {
        panic!("draft")
    };
    assert_eq!(form.value("guide"), None);
    assert_eq!(form.value("channel").as_deref(), Some("saved-channel"));
    assert_eq!(app.jobs(), Vec::new());
}

#[test]
fn a_failed_preference_read_is_visible_and_keeps_the_draft_editable() {
    let mut app = draft();
    let error = retold_app::protocol::Diagnostic::new(
        retold_app::protocol::ErrorCode::DataInvalid,
        "Saved draft choices could not be read",
    )
    .into_payload();
    app.receive(
        Job::Local {
            id: 0,
            task: Task::LoadDefaults(Option::default()),
        },
        error,
    );
    assert!(
        app.toasts
            .iter()
            .any(|toast| toast.text.contains("Saved draft choices"))
    );
    let app = type_text(app, "manual-guide");
    assert!(matches!(app.page(), Some(Page::Form(form))
        if form.value("guide").as_deref() == Some("manual-guide")));
}

#[test]
fn a_preference_symlink_is_rejected_without_changing_its_target() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("source.json");
    let saved = br#"{"last":{"guide":"synthetic-private-value"}}"#;
    std::fs::write(&target, saved).unwrap();
    std::os::unix::fs::symlink(&target, root.path().join(".tui-defaults.json")).unwrap();
    assert!(defaults::load(Some(root.path())).is_err());
    assert_eq!(std::fs::read(target).unwrap(), saved);
}

#[test]
fn a_preference_fifo_is_rejected_without_waiting_for_a_writer() {
    use std::{sync::mpsc, time::Duration};
    const PREVIEW_WAIT: Duration = Duration::from_secs(1);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".tui-defaults.json");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .unwrap()
            .success()
    );
    let (send, receive) = mpsc::channel();
    let root_path = root.path().to_path_buf();
    std::thread::spawn(move || send.send(defaults::load(Some(&root_path))).unwrap());
    assert!(
        receive
            .recv_timeout(PREVIEW_WAIT)
            .expect("a FIFO must not block the read lane")
            .is_err()
    );
}

#[test]
fn a_nonobject_preference_file_is_rejected_and_preserved() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".tui-defaults.json");
    std::fs::write(&path, b"[1,2,3]").unwrap();
    assert!(defaults::load(Some(root.path())).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"[1,2,3]");
}
