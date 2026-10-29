//! Host clipboard work must leave navigation and the normal quit controls responsive.
use super::*;

#[test]
fn copy_queues_background_work_without_lending_the_terminal() {
    let mut app = fresh();
    app.press(key(KeyCode::Char('3')));
    app.press(key(KeyCode::Char('y')));
    assert!(app.host.handoff.is_none());
    assert_eq!(app.jobs().len(), 1);
    assert_eq!(app.running.len(), 1);
    app.press(key(KeyCode::Char('7')));
    assert_eq!(app.section, section::Section::Providers);
    app.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(matches!(app.modal, Some(Modal::Quit)));
}

#[test]
fn repeated_copy_is_coalesced_and_failure_finishes_a_waiting_quit() {
    use retold_app::protocol::{Diagnostic, ErrorCode};
    let mut app = fresh();
    app.press(key(KeyCode::Char('y')));
    let jobs = app.jobs();
    let job = jobs.first().unwrap().clone();
    app.press(key(KeyCode::Char('y')));
    assert_eq!(app.jobs().len(), 1);
    app.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    app.press(key(KeyCode::Enter));
    assert!(app.quit_when_idle);
    assert!(!app.quit);
    let failure = Diagnostic::new(ErrorCode::ExecutionFailed, "Clipboard timed out").into_payload();
    app.receive(job, failure);
    assert!(app.quit);
    assert_eq!(app.running, BTreeSet::new());
    assert!(
        app.toasts
            .iter()
            .any(|toast| toast.text == "Clipboard timed out")
    );
}
