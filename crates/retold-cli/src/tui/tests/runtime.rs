//! Synchronization proves reads remain responsive while actions are serialized.
use std::sync::Mutex;
use std::time::Duration;

use super::{Arc, Dispatch, Job, Worker, mpsc};
use retold_app::protocol::{Outcome, Payload};

const WAIT: Duration = Duration::from_secs(3);

fn completed() -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({}),
    }
}

fn blocked_worker() -> (Worker, mpsc::Sender<()>, mpsc::Receiver<()>) {
    let (entered, starts) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let dispatch: Dispatch = Arc::new(move |words: Vec<String>| {
        let writing = words.last().is_some_and(|word| word == "write");
        if writing {
            entered.send(()).unwrap();
            let gate = released.lock().unwrap();
            gate.recv_timeout(WAIT).unwrap();
        }
        completed()
    });
    let worker = Worker::start(Vec::new(), dispatch, || {});
    (worker, release, starts)
}

#[test]
fn blocked_action_does_not_block_reads_or_allow_another_action_to_overlap() {
    let (worker, release, starts) = blocked_worker();
    worker.submit(Job::Action {
        id: 1,
        words: vec!["write".into()],
    });
    starts.recv_timeout(WAIT).unwrap();
    worker.submit(Job::Action {
        id: 2,
        words: vec!["write".into()],
    });
    worker.submit(Job::Query(vec!["read".into()]));
    let reply = worker.receiver.recv_timeout(WAIT).unwrap();
    assert!(matches!(reply.job, Job::Query(_)));
    assert!(
        starts.try_recv().is_err(),
        "second write overlapped the first"
    );
    release.send(()).unwrap();
    starts.recv_timeout(WAIT).unwrap();
    release.send(()).unwrap();
    let first = worker.receiver.recv_timeout(WAIT).unwrap();
    let second = worker.receiver.recv_timeout(WAIT).unwrap();
    assert!(matches!(first.job, Job::Action { id: 1, .. }));
    assert!(matches!(second.job, Job::Action { id: 2, .. }));
}

#[test]
fn failed_request_is_reported_and_the_lane_accepts_the_next_request() {
    let dispatch: Dispatch = Arc::new(|words: Vec<String>| {
        let failing = words.last().is_some_and(|word| word == "fail");
        if failing {
            std::panic::resume_unwind(Box::new("fixture failure"));
        }
        completed()
    });
    let worker = Worker::start(Vec::new(), dispatch, || {});
    worker.submit(Job::Query(vec!["fail".into()]));
    worker.submit(Job::Query(vec!["read".into()]));
    assert!(matches!(
        worker.receiver.recv_timeout(WAIT).unwrap().payload,
        Payload::Error { .. }
    ));
    assert!(matches!(
        worker.receiver.recv_timeout(WAIT).unwrap().payload,
        Payload::Result { .. }
    ));
}

#[path = "runtime_terminal.rs"]
mod terminal;
