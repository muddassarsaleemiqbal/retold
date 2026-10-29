//! Real local processes, with test-only JSON fixtures (never a production fallback).
use retold_infer::worker::{Worker, WorkerSpec};
use serde_json::json;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
fn worker(dir: &std::path::Path, script: &str) -> Result<Worker> {
    let path = dir.join("fixture.py");
    std::fs::write(&path, script)?;
    Ok(Worker::new(WorkerSpec {
        program: "python3".into(),
        args: vec![path.display().to_string()],
        cwd: dir.into(),
        timeout: Duration::from_secs(2),
        idle_timeout: Duration::from_millis(100),
    }))
}
#[tokio::test]
async fn crash_retries_once_and_idle_shutdown_respawns_fresh_worker() -> Result {
    let dir = tempfile::tempdir()?;
    let worker = worker(
        dir.path(),
        r"
import sys,os,json
from pathlib import Path
if not Path('crashed').exists():
    Path('crashed').write_text('once'); sys.exit(1)
for line in sys.stdin:
    q=json.loads(line)
    print(json.dumps(dict(version=1,id=q['id'],result=dict(pid=os.getpid()))),flush=True)
",
    )?;
    let token = CancellationToken::new();
    let first = worker.health(&token).await?;
    let second = worker.health(&token).await?;
    assert_eq!(first, second);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_ne!(first, worker.health(&token).await?);
    worker.shutdown().await;
    Ok(())
}
#[tokio::test]
async fn cancellation_kills_helpers_within_one_second() -> Result {
    let dir = tempfile::tempdir()?;
    let worker = worker(
        dir.path(),
        r"
import sys,subprocess,time
from pathlib import Path
p=subprocess.Popen(['sleep','30'])
Path('helper').write_text(str(p.pid))
for line in sys.stdin: time.sleep(30)
",
    )?;
    let token = CancellationToken::new();
    let cloned = token.clone();
    let pid_file = dir.path().join("helper");
    tokio::spawn(async move {
        for _ in 0..100 {
            if pid_file.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        cloned.cancel();
    });
    let started = std::time::Instant::now();
    assert!(matches!(
        worker.request("wait", json!({}), &token).await,
        Err(retold_infer::Error::Cancelled)
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
    let pid = std::fs::read_to_string(dir.path().join("helper"))?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?
            .success()
    );
    Ok(())
}
#[tokio::test]
async fn repeated_crash_and_wrong_protocol_fail() -> Result {
    for script in [
        "import sys\nsys.exit(1)",
        "import sys,json\nfor line in sys.stdin: print(json.dumps(dict(version=2,id=999,result={})),flush=True)",
    ] {
        let dir = tempfile::tempdir()?;
        let worker = worker(dir.path(), script)?;
        assert!(worker.health(&CancellationToken::new()).await.is_err());
    }
    Ok(())
}
