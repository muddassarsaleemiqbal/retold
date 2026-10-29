//! Cancellation retains stage ownership until an uncancellable writer actually finishes.
use std::sync::Arc;

use retold_engine::{EngineError, RunLayout, local_stages, store::Store};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

struct HeldStep {
    started: oneshot::Receiver<()>,
    release: oneshot::Sender<()>,
    task: JoinHandle<Result<String, EngineError>>,
}

#[tokio::test]
async fn cancellation_drains_the_writer_and_preserves_its_completed_cache() -> TestResult<()> {
    let (_temporary, store) = fixture()?;
    let cancel = CancellationToken::new();
    let step = held_step(Arc::clone(&store), cancel.clone());
    step.started.await?;
    cancel.cancel();
    tokio::task::yield_now().await;
    assert!(
        !step.task.is_finished(),
        "stage ownership was released while a writer remained active"
    );
    step.release.send(()).map_err(|()| "writer disappeared")?;
    let result = step.task.await?;
    assert!(matches!(result, Err(EngineError::Cancelled)));
    assert!(store.lock().await.state.completed.contains_key("shots-0"));
    Ok(())
}

fn fixture() -> TestResult<(tempfile::TempDir, Arc<Mutex<Store>>)> {
    let temporary = tempfile::tempdir()?;
    let layout = RunLayout::from_video_dir(
        temporary.path(),
        "channel",
        &temporary.path().join("channel/films/fixture"),
        2,
    )
    .ok_or("invalid fixture layout")?;
    let store = Arc::new(Mutex::new(Store::open(&layout)?));
    Ok((temporary, store))
}

fn held_step(store: Arc<Mutex<Store>>, cancel: CancellationToken) -> HeldStep {
    let (started, started_receiver) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let task = tokio::spawn(async move {
        local_stages::run_stage_step(
            &store,
            "shots-0",
            &serde_json::json!({"fixture":"held"}),
            &cancel,
            || async {
                let _ = started.send(());
                released
                    .await
                    .map_err(|_| EngineError::Setup("release signal disappeared".into()))?;
                Ok("finished artifact".into())
            },
        )
        .await
    });
    HeldStep {
        started: started_receiver,
        release,
        task,
    }
}
