use retold_engine::{RunLayout, local_stages, store::Store};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
#[path = "local_stages/drain.rs"]
mod drain;
#[tokio::test]
async fn every_stage_recovers_commits_and_redo_invalidates_only_its_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let layout = RunLayout::from_video_dir(
        temp.path(),
        "channel",
        &temp.path().join("channel/films/fixture"),
        2,
    )
    .ok_or("invalid layout")?;
    let cancel = CancellationToken::new();
    for stage in ["depth", "voices", "mouths", "shots", "deliver", "metrics"] {
        let step = format!("{stage}-0");
        let store = Arc::new(Mutex::new(Store::open(&layout)?));
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let inputs = serde_json::json!({"image":"recorded-fixture"});
        let value: String =
            local_stages::run_stage_step(&store, &step, &inputs, &cancel, || async {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok("fixture-result".into())
            })
            .await?;
        drop(store);
        let store = Arc::new(Mutex::new(Store::open(&layout)?));
        let resumed: String =
            local_stages::run_stage_step(&store, &step, &inputs, &cancel, || async {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok("should-not-run".into())
            })
            .await?;
        assert_eq!(value, resumed);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        store.lock().await.bump_revision(&step)?;
        let fresh: String =
            local_stages::run_stage_step(&store, &step, &inputs, &cancel, || async {
                Ok("new-result".into())
            })
            .await?;
        assert_ne!(fresh, value);
    }
    let store = Arc::new(Mutex::new(Store::open(&layout)?));
    cancel.cancel();
    let result: Result<String, _> = local_stages::run_stage_step(
        &store,
        "shots-1",
        &serde_json::json!({}),
        &cancel,
        || async { Ok("never".into()) },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(store.lock().await.revision("shots-1"), 0);
    Ok(())
}

#[test]
fn killed_stage_helper() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(root) = std::env::var("RETOLD_KILL_FIXTURE") else {
        return Ok(());
    };
    let stage = std::env::var("RETOLD_KILL_STAGE")?;
    let root = std::path::Path::new(&root);
    let layout = RunLayout::from_video_dir(root, "channel", &root.join("channel/films/fixture"), 2)
        .ok_or("invalid layout")?;
    let mut store = Store::open(&layout)?;
    let inputs = serde_json::json!({"fixture":1});
    let key =
        retold_engine::store::hash(&serde_json::json!({"version":1,"revision":0,"inputs":inputs}))?;
    store.commit(&format!("{stage}-0"), &key, &"completed-before-kill", None)?;
    store.begin(&format!("{stage}-1"))?;
    std::fs::write(root.join("kill-ready"), b"ready")?;
    std::thread::sleep(std::time::Duration::from_secs(30));
    Ok(())
}
#[tokio::test]
async fn real_sigkill_mid_stage_preserves_finished_items_for_every_hook()
-> Result<(), Box<dyn std::error::Error>> {
    for stage in ["depth", "voices", "mouths", "shots", "deliver", "metrics"] {
        let root = tempfile::tempdir()?;
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "killed_stage_helper"])
            .env("RETOLD_KILL_FIXTURE", root.path())
            .env("RETOLD_KILL_STAGE", stage)
            .stdout(std::process::Stdio::null())
            .spawn()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !root.path().join("kill-ready").exists() {
            if std::time::Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                return Err("stage helper did not become ready".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        child.kill()?;
        child.wait()?;
        let layout = RunLayout::from_video_dir(
            root.path(),
            "channel",
            &root.path().join("channel/films/fixture"),
            2,
        )
        .ok_or("invalid layout")?;
        let store = Arc::new(Mutex::new(Store::open(&layout)?));
        let token = CancellationToken::new();
        let finished: String = local_stages::run_stage_step(
            &store,
            &format!("{stage}-0"),
            &serde_json::json!({"fixture":1}),
            &token,
            || async {
                Err(retold_engine::EngineError::Setup(
                    "finished item repeated".into(),
                ))
            },
        )
        .await?;
        assert_eq!(finished, "completed-before-kill");
        let resumed: String = local_stages::run_stage_step(
            &store,
            &format!("{stage}-1"),
            &serde_json::json!({"fixture":2}),
            &token,
            || async { Ok("resumed".into()) },
        )
        .await?;
        assert_eq!(resumed, "resumed");
        store.lock().await.finish()?;
    }
    Ok(())
}
