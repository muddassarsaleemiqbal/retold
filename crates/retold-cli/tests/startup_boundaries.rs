//! Startup checks the selected library, explicit state location and account maintenance gate.
#![expect(
    clippy::unwrap_used,
    reason = "isolated fixture and assertion failures must fail these tests"
)]
#[path = "startup_boundaries/support.rs"]
mod support;

use support::Fixture;

#[test]
fn an_inactive_library_cannot_start_a_studio_in_an_external_active_state_directory() {
    let fixture = Fixture::new().inactive_library();
    let (response, code) = fixture.invoke(&["studio", "start"]);
    assert_eq!(code, 3, "{response}");
    assert_eq!(response["payload"]["code"], "RG-NOT-READY");
    assert!(!fixture.state.exists());
}

#[test]
fn an_inactive_state_directory_is_rejected_before_any_startup_write() {
    let fixture = Fixture::new().inactive_state();
    let (response, code) = fixture.invoke(&["studio", "start", "--foreground"]);
    assert_eq!(code, 3, "{response}");
    assert_eq!(response["payload"]["code"], "RG-NOT-READY");
    assert!(!fixture.state.exists());
    let (status, code) = fixture.invoke(&["studio", "status"]);
    assert_eq!(code, 0, "{status}");
    assert!(!fixture.state.exists());
}

#[test]
fn maintenance_blocks_startup_and_an_idle_studio_releases_the_startup_barrier() {
    let fixture = Fixture::new();
    let (_account, directory) = fixture.account();
    let maintenance = retold_app::account::barrier::exclusive(&directory).unwrap();
    let (busy, code) = fixture.invoke(&["studio", "start", "--foreground"]);
    assert_eq!(code, 3, "{busy}");
    assert_eq!(busy["payload"]["code"], "RG-ACCOUNT-BUSY");
    assert!(!fixture.state.exists());
    drop(maintenance);
    let (started, code) = fixture.invoke(&["studio", "start"]);
    assert_eq!(code, 0, "{started}");
    assert!(started["payload"]["data"]["running"].as_bool().unwrap());
    let _idle = retold_app::account::barrier::exclusive(&directory).unwrap();
}

#[test]
fn inactive_history_remains_available_without_schema_or_queue_mutations() {
    let fixture = Fixture::new();
    let queue = retold_app::queue::QueueStore::open(&fixture.state.join("queue.sqlite")).unwrap();
    let job = queue
        .enqueue(
            retold_app::queue::JobSpec {
                idempotency_key: "historical",
                video_ref: "video",
                plan_hash: "hash",
            },
            "2026-10-20T00:00:00Z",
        )
        .unwrap();
    let before = std::fs::read(fixture.state.join("queue.sqlite")).unwrap();
    fixture.mark_inactive();
    let (listed, code) = fixture.invoke(&["queue", "list"]);
    assert_eq!(code, 0, "{listed}");
    assert_eq!(listed["payload"]["data"]["jobs"][0]["id"], job.id);
    assert_eq!(
        std::fs::read(fixture.state.join("queue.sqlite")).unwrap(),
        before
    );
}
