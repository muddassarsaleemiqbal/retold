//! Read-only replay and live following retain the job across detach and output failures.
#![expect(
    clippy::unwrap_used,
    reason = "fixture and assertion failures must fail these tests"
)]
#[path = "queue_follow/support.rs"]
mod support;

use retold_app::queue::{EVENT_PAGE_SIZE, JobStatus};
use support::{Fixture, parse_lines};

#[test]
fn offline_terminal_jobs_replay_all_pages_before_one_saved_outcome() {
    const EXTRA_EVENTS: usize = 7;
    let fixture = Fixture::new();
    fixture.progress(EVENT_PAGE_SIZE * 2 + EXTRA_EVENTS);
    let fixture = fixture.completed().offline();
    let output = fixture.command(&["--json"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let messages = parse_lines(std::str::from_utf8(&output.stdout).unwrap());
    assert_eq!(messages.len(), EVENT_PAGE_SIZE * 2 + EXTRA_EVENTS + 3);
    assert_eq!(
        messages
            .iter()
            .filter(|message| message["type"] != "event")
            .count(),
        1
    );
    assert_eq!(
        messages.last().unwrap()["payload"]["data"]["proof"],
        "durable-fixture"
    );
    assert_progress_order(&messages, &fixture.id);
}

fn assert_progress_order(messages: &[serde_json::Value], id: &str) {
    let events = messages
        .iter()
        .filter(|message| message["type"] == "event")
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .all(|event| event["job_id"] == id && event["payload"]["event"] == "job_progress")
    );
    assert!(
        events
            .windows(2)
            .all(|pair| pair[0]["sequence"].as_u64() < pair[1]["sequence"].as_u64())
    );
    let request = &messages[0]["request_id"];
    assert!(
        messages
            .iter()
            .all(|message| &message["request_id"] == request)
    );
    let last = events.last().unwrap()["sequence"].as_u64().unwrap();
    assert_eq!(
        messages.last().unwrap()["payload"]["data"]["attachment"]["after_sequence"],
        last
    );
}

#[test]
fn replay_after_a_cursor_and_quiet_mode_preserve_the_saved_outcome() {
    let fixture = Fixture::new();
    fixture.progress(EVENT_PAGE_SIZE + 1);
    let fixture = fixture.completed().offline();
    let cursor = u64::try_from(EVENT_PAGE_SIZE).unwrap();
    let mut command = fixture.command(&["--json"]);
    command.args(["--after-sequence", &cursor.to_string()]);
    let output = command.output().unwrap();
    assert!(output.status.success());
    let messages = parse_lines(std::str::from_utf8(&output.stdout).unwrap());
    assert!(
        messages
            .iter()
            .filter(|message| message["type"] == "event")
            .all(|event| event["sequence"].as_u64().unwrap() > cursor)
    );
    let quiet = fixture.command(&["--json", "--quiet"]).output().unwrap();
    let messages = parse_lines(std::str::from_utf8(&quiet.stdout).unwrap());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["payload"]["outcome"], "completed");
}

#[test]
fn live_follow_receives_later_progress_and_the_persisted_terminal_outcome() {
    let fixture = Fixture::new();
    let (observer, pipe) = fixture.spawn().first_line();
    fixture.progress(1);
    let fixture = fixture.completed();
    assert!(observer.wait().success());
    let messages = pipe.remaining();
    assert_eq!(messages.len(), 4);
    assert_progress_order(&messages, &fixture.id);
    assert_eq!(fixture.status(), JobStatus::Completed);
}

#[test]
fn ctrl_c_detaches_with_a_cursor_and_leaves_the_job_running() {
    let fixture = Fixture::new();
    let (observer, pipe) = fixture.spawn().first_line();
    observer.interrupt();
    assert!(observer.wait().success());
    let messages = pipe.remaining();
    let terminal = messages.last().unwrap();
    assert_eq!(terminal["payload"]["outcome"], "detached");
    assert_eq!(terminal["payload"]["data"]["job_id"], fixture.id);
    assert_eq!(
        terminal["payload"]["data"]["after_sequence"],
        messages[0]["sequence"]
    );
    assert_eq!(fixture.status(), JobStatus::Running);
}

#[test]
fn a_stalled_output_pipe_cannot_trap_the_attachment_client() {
    const BACKPRESSURE_EVENTS: usize = 512;
    let fixture = Fixture::new();
    fixture.large_progress(BACKPRESSURE_EVENTS);
    let (observer, pipe) = fixture.spawn().first_line();
    std::thread::sleep(std::time::Duration::from_millis(100));
    fixture.progress(1);
    observer.interrupt();
    assert!(observer.wait().success());
    drop(pipe);
    assert_eq!(fixture.status(), JobStatus::Running);
}

#[test]
fn a_closed_output_pipe_detaches_without_stopping_the_job() {
    let fixture = Fixture::new();
    fixture.large_progress(1);
    assert!(fixture.spawn().close_reader().wait().success());
    assert_eq!(fixture.status(), JobStatus::Running);
}

#[test]
fn offline_once_returns_one_bounded_page_and_a_resume_cursor() {
    let fixture = Fixture::new();
    fixture.progress(EVENT_PAGE_SIZE + 1);
    let fixture = fixture.completed().offline();
    let mut command = fixture.command(&["--json"]);
    command.arg("--once");
    let output = command.output().unwrap();
    let messages = parse_lines(std::str::from_utf8(&output.stdout).unwrap());
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0]["payload"]["data"]["events"]
            .as_array()
            .unwrap()
            .len(),
        EVENT_PAGE_SIZE
    );
    assert_eq!(messages[0]["payload"]["data"]["is_terminal"], false);
    assert_eq!(
        messages[0]["payload"]["data"]["after_sequence"],
        EVENT_PAGE_SIZE
    );
}

#[test]
fn plain_attachment_keeps_progress_and_outcome_readable() {
    let fixture = Fixture::new();
    fixture.progress(1);
    let fixture = fixture.completed().offline();
    let output = fixture.command(&["--plain"]).output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("[seq 1] running:"));
    assert!(text.contains("research: Saved 0"));
    assert!(text.ends_with("Stored outcome fixture\n"));
    assert!(!text.contains('\u{1b}'));
}

#[test]
fn losing_the_studio_reports_the_existing_job_and_cursor_without_retrying() {
    let fixture = Fixture::new();
    let (observer, pipe) = fixture.spawn().first_line();
    let fixture = fixture.offline();
    assert_eq!(observer.wait().code(), Some(3));
    let messages = pipe.remaining();
    let terminal = messages.last().unwrap();
    assert_eq!(terminal["payload"]["code"], "RG-STUDIO-REQUIRED");
    assert_eq!(terminal["payload"]["context"]["job_id"], fixture.id);
    assert_eq!(
        terminal["payload"]["context"]["after_sequence"],
        messages[0]["sequence"]
    );
    assert_eq!(fixture.status(), JobStatus::Running);
    assert!(fixture.queue.intent(&fixture.id).unwrap().is_none());
}

#[test]
fn a_slow_consumer_receives_the_complete_large_terminal_outcome() {
    const EXPECTED_BYTES: usize = 200_000;
    const READER_DELAY: std::time::Duration = std::time::Duration::from_millis(250);
    let fixture = Fixture::new().large_outcome().offline();
    let (observer, pipe) = fixture.spawn().first_line();
    std::thread::sleep(READER_DELAY);
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = send.send(pipe.remaining());
    });
    let messages = receive.recv_timeout(support::WAIT_LIMIT).unwrap();
    assert!(observer.wait().success());
    let outcome = &messages.last().unwrap()["payload"]["data"];
    assert_eq!(outcome["proof"].as_str().unwrap().len(), EXPECTED_BYTES);
}

#[test]
fn two_observers_receive_the_same_committed_events_without_duplicating_work() {
    let fixture = Fixture::new();
    let (first, first_pipe) = fixture.spawn().first_line();
    let (second, second_pipe) = fixture.spawn().first_line();
    fixture.progress(1);
    let fixture = fixture.completed();
    assert!(first.wait().success());
    assert!(second.wait().success());
    let first_messages = first_pipe.remaining();
    let second_messages = second_pipe.remaining();
    assert_eq!(first_messages.len(), 4);
    assert_eq!(second_messages.len(), 4);
    assert_progress_order(&first_messages, &fixture.id);
    assert_progress_order(&second_messages, &fixture.id);
    assert_eq!(fixture.queue.events(&fixture.id, 0).unwrap().len(), 3);
}
