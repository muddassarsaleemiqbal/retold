//! Single-slot video delivery; preview processes are canceled and reaped off the UI thread.

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, ensure};

use super::{FRAME_BYTES, FrameSlot, Media, decoder, process};
use crate::work::Cancellation;

pub fn play(
    media: &Media,
    position: f64,
    cancel: &Cancellation,
    frames: FrameSlot,
) -> anyhow::Result<()> {
    if cancel.is_cancelled() {
        return Ok(());
    }
    let mut audio = if media.audio {
        let mut command = Command::new("ffplay");
        command
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-nostats",
                "-nodisp",
                "-autoexit",
                "-vn",
                "-ss",
            ])
            .arg(position.max(0.0).to_string())
            .arg(&media.path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Some(
            command
                .spawn()
                .context("Install ffplay for audio playback")?,
        )
    } else {
        None
    };
    let result = if media.video {
        video(media, position, cancel, frames)
    } else if let Some(child) = audio.as_mut() {
        wait_audio(child, cancel, (media.duration - position).max(0.0))
    } else {
        Err(anyhow::anyhow!("No playable streams"))
    };
    if let Some(child) = audio.as_mut() {
        process::stop(child)?;
    }
    result
}

fn video(
    media: &Media,
    position: f64,
    cancel: &Cancellation,
    frames: FrameSlot,
) -> anyhow::Result<()> {
    let mut command = decoder(media, position);
    command.args(["-r", "12", "-f", "rawvideo", "-pix_fmt", "bgra", "pipe:1"]);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Could not start the video decoder")?;
    let stdout = child.stdout.take().context("Video output is unavailable")?;
    let reader_cancel = cancel.clone();
    let reader = thread::spawn(move || read_frames(stdout, &reader_cancel, &frames));
    let deadline =
        Instant::now() + Duration::from_secs_f64((media.duration - position).max(0.0) + 15.0);
    let result = loop {
        if cancel.is_cancelled() {
            break process::stop(&mut child);
        }
        if Instant::now() > deadline {
            cancel.cancel();
            let stopped = process::stop(&mut child);
            break stopped.and_then(|()| Err(anyhow::anyhow!("Video preview timed out")));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Video decoder exited: {status}"))
                };
            }
            Ok(None) => thread::sleep(process::POLL),
            Err(error) => {
                let _ = process::stop(&mut child);
                break Err(error.into());
            }
        }
    };
    let decoded = reader
        .join()
        .map_err(|_| anyhow::anyhow!("Video reader failed"))?;
    if cancel.is_cancelled() {
        return result;
    }
    result.and(decoded)
}

fn read_frames(
    mut stdout: impl Read,
    cancel: &Cancellation,
    frames: &FrameSlot,
) -> anyhow::Result<()> {
    let started = Instant::now();
    let mut number = 0_u32;
    while !cancel.is_cancelled() {
        let mut bytes = vec![0; FRAME_BYTES];
        match stdout.read_exact(&mut bytes) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let due = Duration::from_secs_f64(f64::from(number) / 12.0);
        while started.elapsed() < due && !cancel.is_cancelled() {
            thread::sleep(process::POLL);
        }
        if cancel.is_cancelled() {
            break;
        }
        *frames
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview buffer is unavailable"))? = Some(bytes);
        number = number.saturating_add(1);
    }
    Ok(())
}

fn wait_audio(
    child: &mut std::process::Child,
    cancel: &Cancellation,
    remaining: f64,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs_f64(remaining + 15.0);
    while !cancel.is_cancelled() {
        if Instant::now() > deadline {
            process::stop(child)?;
            anyhow::bail!("Audio preview timed out");
        }
        if let Some(status) = child.try_wait().inspect_err(|_| {
            let _ = process::stop(child);
        })? {
            ensure!(status.success(), "Audio playback failed: {status}");
            return Ok(());
        }
        thread::sleep(process::POLL);
    }
    process::stop(child)
}
