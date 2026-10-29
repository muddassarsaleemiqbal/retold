//! One long-lived worker per handle. Versioned bounded JSON-lines; crash retry once.
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio_util::sync::CancellationToken;

use crate::Error;
const MAX_LINE: u64 = 1024 * 1024;
#[derive(Debug, Clone)]
pub struct WorkerSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub idle_timeout: Duration,
}
#[derive(Debug)]
struct Running {
    child: Child,
    group: retold_providers::process::Group,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    stderr: tokio::task::JoinHandle<()>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.group.kill();
        self.stderr.abort();
    }
}
#[derive(Debug, Clone)]
pub struct Worker {
    spec: WorkerSpec,
    state: std::sync::Arc<tokio::sync::Mutex<Option<Running>>>,
    sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
}
impl Worker {
    #[must_use]
    pub fn new(spec: WorkerSpec) -> Self {
        Self {
            spec,
            state: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            sequence: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }
    fn spawn(&self) -> Result<Running, Error> {
        let mut command = Command::new(&self.spec.program);
        command
            .args(&self.spec.args)
            .current_dir(&self.spec.cwd)
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn()?;
        let group = retold_providers::process::Group::new(child.id());
        let input = child
            .stdin
            .take()
            .ok_or_else(|| Error::Invalid("worker stdin missing".into()))?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| Error::Invalid("worker stdout missing".into()))?,
        );
        let stderr = child.stderr.take();
        let stderr = tokio::spawn(async move {
            if let Some(mut s) = stderr {
                let _ = tokio::io::copy(&mut s, &mut tokio::io::sink()).await;
            }
        });
        Ok(Running {
            child,
            group,
            input,
            output,
            stderr,
        })
    }
    pub async fn request(
        &self,
        operation: &str,
        params: Value,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        let mut state = tokio::select! { r = self.state.lock() => r, () = cancel.cancelled() => return Err(Error::Cancelled) };
        let id = self
            .sequence
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let request = json!({"version":1,"id":id,"operation":operation,"params":params});
        let mut bytes = serde_json::to_vec(&request)?;
        if bytes.len() > usize::try_from(MAX_LINE).map_err(|e| Error::Invalid(e.to_string()))? {
            return Err(Error::Invalid("worker input too large".into()));
        }
        bytes.push(b'\n');
        for attempt in 0..2 {
            if cancel.is_cancelled() {
                state.take();
                return Err(Error::Cancelled);
            }
            if state.is_none() {
                *state = Some(self.spawn()?);
            }
            let running = state
                .as_mut()
                .ok_or_else(|| Error::Invalid("worker missing".into()))?;
            let exchange = async {
                running.input.write_all(&bytes).await?;
                running.input.flush().await?;
                let mut line = Vec::new();
                let n = (&mut running.output)
                    .take(MAX_LINE + 1)
                    .read_until(b'\n', &mut line)
                    .await?;
                if n == 0 {
                    return Err(Error::Io(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "worker exited",
                    )));
                }
                if u64::try_from(n).unwrap_or(u64::MAX) > MAX_LINE || line.last() != Some(&b'\n') {
                    return Err(Error::Invalid("worker response exceeds limit".into()));
                }
                let response: Value = serde_json::from_slice(&line)?;
                if response["version"] != 1 || response["id"] != id {
                    return Err(Error::Invalid("worker protocol version/id mismatch".into()));
                }
                Ok(response)
            };
            let result = tokio::select! { r = exchange => r, () = cancel.cancelled() => Err(Error::Cancelled), () = tokio::time::sleep(self.spec.timeout) => Err(Error::Invalid("worker timed out; inspect the run log and resume".into())) };
            match result {
                Ok(response) => {
                    let sequence = std::sync::Arc::clone(&self.sequence);
                    let idle_state = std::sync::Arc::downgrade(&self.state);
                    let idle = self.spec.idle_timeout;
                    tokio::spawn(async move {
                        tokio::time::sleep(idle).await;
                        if sequence.load(std::sync::atomic::Ordering::SeqCst) == id + 1 {
                            let Some(idle_state) = idle_state.upgrade() else {
                                return;
                            };
                            let mut state = idle_state.lock().await;
                            if sequence.load(std::sync::atomic::Ordering::SeqCst) == id + 1 {
                                state.take();
                            }
                        }
                    });
                    if let Some(error) = response.get("error") {
                        return Err(Error::Invalid(format!("worker: {error}")));
                    }
                    return response
                        .get("result")
                        .cloned()
                        .ok_or_else(|| Error::Invalid("worker returned no result".into()));
                }
                Err(error) => {
                    let crashed = running.child.try_wait()?.is_some()
                        || matches!(&error, Error::Io(e) if matches!(e.kind(),std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe));
                    state.take();
                    if !crashed || attempt == 1 {
                        return Err(error);
                    }
                }
            }
        }
        Err(Error::Invalid("worker retry exhausted".into()))
    }
    pub async fn health(&self, cancel: &CancellationToken) -> Result<Value, Error> {
        self.request("health", json!({}), cancel).await
    }
    pub async fn shutdown(&self) {
        self.state.lock().await.take();
    }
}
