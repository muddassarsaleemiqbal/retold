//! Bounded child output and cancellation. Only explicitly owned preview children are stopped.

use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};

use crate::work::Cancellation;

pub const POLL: Duration = Duration::from_millis(10);

pub fn stop(child: &mut Child) -> anyhow::Result<()> {
    if child.try_wait()?.is_none() {
        child.kill().context("Could not stop the preview process")?;
    }
    child.wait().context("Could not reap the preview process")?;
    Ok(())
}

pub fn output(mut command: Command, cancel: &Cancellation, limit: usize) -> anyhow::Result<Output> {
    if cancel.is_cancelled() {
        bail!("Media inspection canceled");
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Install ffmpeg and ffprobe to preview existing media")?;
    let stdout = child
        .stdout
        .take()
        .context("Preview output pipe is unavailable")?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let result = loop {
        if cancel.is_cancelled() || started.elapsed() > Duration::from_secs(15) {
            let stopped = stop(&mut child);
            break stopped
                .and_then(|()| Err(anyhow::anyhow!("Media inspection canceled or timed out")));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(POLL),
            Err(error) => {
                let _ = stop(&mut child);
                break Err(error.into());
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| anyhow::anyhow!("Preview reader failed"))??;
    let status = result?;
    anyhow::ensure!(
        bytes.len() <= limit,
        "Preview output exceeded its memory limit"
    );
    anyhow::ensure!(
        status.success(),
        "This media could not be decoded (process {status})"
    );
    Ok(Output {
        status,
        stdout: bytes,
        stderr: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_child_output_is_rejected() {
        let result = output(Command::new("/usr/bin/yes"), &Cancellation::default(), 1024);
        assert!(result.is_err());
    }

    #[test]
    fn cancellation_reaps_a_child_with_no_output() -> anyhow::Result<()> {
        let cancel = Cancellation::default();
        let control = cancel.clone();
        let worker = thread::spawn(move || {
            let mut command = Command::new("/bin/sleep");
            command.arg("30");
            output(command, &cancel, 1024)
        });
        thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        control.cancel();
        let result = worker
            .join()
            .map_err(|_| anyhow::anyhow!("child worker failed"))?;
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        Ok(())
    }
}
