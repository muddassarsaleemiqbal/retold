//! Integration tests for `retold queue` commands.

#![expect(clippy::unwrap_used, reason = "test assertions unwrap")]

use std::process::Command;

use retold_app::queue::{JobSpec, QueueStore};

fn run_queue(state_dir: &std::path::Path, args: &[&str]) -> (serde_json::Value, i32) {
    let exe = assert_cmd::cargo::cargo_bin("retold");
    let mut cmd = Command::new(exe);
    cmd.env("RETOLD_STATE", state_dir);
    cmd.arg("--json");
    cmd.arg("queue");
    cmd.args(args);
    let output = cmd.output().unwrap();
    let json_value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    (json_value, output.status.code().unwrap_or(-1))
}

#[test]
fn queue_list_empty_and_offline() {
    let test_dir = tempfile::tempdir().unwrap();
    let (json_value, code) = run_queue(test_dir.path(), &["list"]);
    assert_eq!(code, 0, "{json_value}");
    assert_eq!(json_value["payload"]["data"]["total"], 0);
    assert_eq!(json_value["payload"]["data"]["jobs"], serde_json::json!([]));
    assert!(!test_dir.path().join("queue.sqlite").exists());
}

#[test]
fn queue_control_without_studio_fails_blocked() {
    let test_dir = tempfile::tempdir().unwrap();
    let (json_value, code) = run_queue(test_dir.path(), &["pause", "job-nonexistent"]);
    assert_eq!(code, 3, "{json_value}");
    assert_eq!(json_value["payload"]["code"], "RG-STUDIO-REQUIRED");

    let (stop_val, stop_code) = run_queue(test_dir.path(), &["stop", "job-nonexistent"]);
    assert_eq!(stop_code, 3, "{stop_val}");
    assert_eq!(stop_val["payload"]["code"], "RG-STUDIO-REQUIRED");

    let (unpause_val, unpause_code) = run_queue(test_dir.path(), &["unpause", "job-nonexistent"]);
    assert_eq!(unpause_code, 3, "{unpause_val}");
    assert_eq!(unpause_val["payload"]["code"], "RG-STUDIO-REQUIRED");

    let (attach_val, attach_code) = run_queue(test_dir.path(), &["attach", "job-nonexistent"]);
    assert_eq!(attach_code, 2, "{attach_val}");
    assert_eq!(attach_val["payload"]["code"], "RG-REF-NOT-FOUND");
}

fn busy_studio_reply(listener: &std::os::unix::net::UnixListener) {
    use std::io::{BufRead, BufReader, Write};
    let (mut stream, _) = listener.accept().unwrap();
    let mut request = String::new();
    BufReader::new(&stream).read_line(&mut request).unwrap();
    let reply = retold_app::studio::StudioResponse::Error {
        code: retold_app::protocol::ErrorCode::AccountBusy,
        message: "Queue is busy; retry after its writer finishes.".into(),
    };
    let mut bytes = serde_json::to_vec(&reply).unwrap();
    bytes.push(b'\n');
    stream.write_all(&bytes).unwrap();
}

#[test]
fn queue_controls_preserve_a_live_studios_busy_diagnostic() {
    let commands: &[&[&str]] = &[
        &["pause", "job-first"],
        &["stop", "job-first"],
        &["unpause", "job-first"],
        &["move", "job-first", "--before", "job-second"],
    ];
    for arguments in commands {
        let root = tempfile::tempdir().unwrap();
        let listener =
            std::os::unix::net::UnixListener::bind(root.path().join("studio.sock")).unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| busy_studio_reply(&listener));
            let (reply, code) = run_queue(root.path(), arguments);
            assert_eq!(code, 3, "{reply}");
            assert_eq!(reply["payload"]["code"], "RG-ACCOUNT-BUSY", "{reply}");
            assert!(
                reply["payload"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Queue is busy")
            );
        });
    }
}

#[test]
fn queue_show_missing_job_returns_not_found() {
    let test_dir = tempfile::tempdir().unwrap();
    let (json_value, code) = run_queue(test_dir.path(), &["show", "job-missing-123"]);
    assert_eq!(code, 2, "{json_value}");
    assert_eq!(json_value["payload"]["code"], "RG-REF-NOT-FOUND");
}

fn enqueue_pair(store: &QueueStore) -> (String, String) {
    let job1 = store
        .enqueue(
            JobSpec {
                idempotency_key: "k1",
                video_ref: "v1",
                plan_hash: "h1",
            },
            "2026-10-19T00:00:00Z",
        )
        .unwrap();
    let job2 = store
        .enqueue(
            JobSpec {
                idempotency_key: "k2",
                video_ref: "v2",
                plan_hash: "h2",
            },
            "2026-10-19T00:01:00Z",
        )
        .unwrap();
    (job1.id, job2.id)
}

#[test]
fn queue_move_without_studio_preserves_pending_order() {
    let test_dir = tempfile::tempdir().unwrap();
    let db_path = test_dir.path().join("queue.sqlite");
    let store = QueueStore::open(&db_path).unwrap();
    let (id1, id2) = enqueue_pair(&store);

    let (move_val, move_code) = run_queue(test_dir.path(), &["move", &id2, "--before", &id1]);
    assert_eq!(move_code, 3, "{move_val}");
    assert_eq!(move_val["payload"]["code"], "RG-STUDIO-REQUIRED");

    let next = store.next_queued().unwrap().unwrap();
    assert_eq!(next.id, id1);
}

#[test]
fn queue_pages_are_bounded_and_changed_filters_invalidate_the_cursor() {
    let root = tempfile::tempdir().unwrap();
    let store = QueueStore::open(&root.path().join("queue.sqlite")).unwrap();
    let (first, second) = enqueue_pair(&store);
    let (page, code) = run_queue(root.path(), &["list", "--limit", "1"]);
    assert_eq!(code, 0, "{page}");
    let page_contents = &page["payload"]["data"];
    assert_eq!(page_contents["jobs"][0]["id"], first);
    assert_eq!(page_contents["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(page_contents["total"], 2);
    let cursor = page_contents["next_cursor"].as_str().unwrap();
    let (next, code) = run_queue(root.path(), &["list", "--limit", "1", "--cursor", cursor]);
    assert_eq!(code, 0, "{next}");
    assert_eq!(next["payload"]["data"]["jobs"][0]["id"], second);
    assert!(next["payload"]["data"]["next_cursor"].is_null());
    let (changed, code) = run_queue(
        root.path(),
        &["list", "--cursor", cursor, "--status", "queued"],
    );
    assert_eq!(code, 2, "{changed}");
    assert_eq!(changed["payload"]["code"], "RG-CURSOR-INVALID");
}
