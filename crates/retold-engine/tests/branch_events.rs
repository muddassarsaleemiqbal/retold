//! The first branch failure is observed while a peer is still running.

use std::sync::Arc;

use retold_engine::EngineError;
use retold_engine::events::{Detail, Event, Events, observe_branch};

const OBSERVATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const SUCCESS_VALUE: u8 = 7;

fn observed_log(
    path: &std::path::Path,
) -> (Arc<Events>, tokio::sync::mpsc::UnboundedReceiver<Event>) {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let observer = Arc::new(move |event: &Event| {
        let _ = sender.send(event.clone());
    });
    let extra_log = None;
    (
        Arc::new(Events::new(path, extra_log, Some(observer))),
        receiver,
    )
}

#[tokio::test]
async fn branch_failure_is_durable_and_immediate_without_cancelling_peer() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("events.log");
    let (events, mut receiver) = observed_log(&path);
    let (release, wait) = tokio::sync::oneshot::channel();
    let joined = tokio::spawn(async move {
        tokio::join!(
            observe_branch(
                async { Err::<(), _>(EngineError::Setup("secret provider details".into())) },
                async || { events.emit(&Event::branch_failed("languages", Some("hi"))) }
            ),
            async {
                wait.await.unwrap();
                "peer completed"
            }
        )
    });
    let event = tokio::time::timeout(OBSERVATION_TIMEOUT, receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(!joined.is_finished());
    assert_saved_failure(&event, &path);
    release.send(()).unwrap();
    let (failure, peer) = joined.await.unwrap();
    assert!(failure.is_err());
    assert_eq!(peer, "peer completed");
}

#[expect(
    clippy::unwrap_used,
    reason = "fixture helper assertions must fail on missing log data"
)]
fn assert_saved_failure(event: &Event, path: &std::path::Path) {
    assert!(
        matches!(event.detail, Some(Detail::StepFailed { ref stage, item: None, .. }) if stage == "languages")
    );
    let saved = std::fs::read_to_string(path).unwrap();
    assert_eq!(saved.lines().count(), 1);
    assert!(!saved.contains("secret provider details"));
}

#[tokio::test]
async fn successful_branch_emits_no_failure() {
    let value = observe_branch::<_, EngineError, _, _>(async { Ok(SUCCESS_VALUE) }, async || {
        Err(EngineError::Setup("unexpected failure notification".into()))
    })
    .await
    .unwrap();
    assert_eq!(value, SUCCESS_VALUE);
}

#[test]
fn old_event_logs_remain_readable() {
    let event: Event = serde_json::from_str(r#"{"timestamp":"2026-10-18T00:00:00Z","task":"voice","status":"running","elapsed_seconds":null,"message":null}"#).unwrap();
    assert!(event.detail.is_none());
}
