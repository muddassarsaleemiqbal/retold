//! Experimental, bounded FFmpeg preview adapter. It decodes existing media; it never renders a video.

mod playback;
mod process;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, ensure};
use serde::Deserialize;

use crate::work::Cancellation;

pub use playback::play;

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 360;
pub const FRAME_BYTES: usize = (WIDTH * HEIGHT * 4) as usize;
pub const FILTER: &str =
    "scale=640:360:force_original_aspect_ratio=decrease,pad=640:360:(ow-iw)/2:(oh-ih)/2";
pub type FrameSlot = Arc<Mutex<Option<Vec<u8>>>>;

#[derive(Debug, Clone)]
pub struct Media {
    pub path: PathBuf,
    pub duration: f64,
    pub video: bool,
    pub audio: bool,
}

#[derive(Debug)]
pub struct Inspection {
    pub media: Media,
    pub frame: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
struct Probe {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Option<Format>,
}

#[derive(Debug, Deserialize)]
struct Stream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Format {
    duration: Option<String>,
}

pub fn inspect(path: &Path, cancel: &Cancellation) -> anyhow::Result<Inspection> {
    ensure!(path.is_file(), "Choose an existing local media file");
    let mut command = Command::new("ffprobe");
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file,pipe",
            "-probesize",
            "5000000",
            "-analyzeduration",
            "5000000",
            "-show_entries",
            "stream=codec_type,width,height:format=duration",
            "-of",
            "json",
        ])
        .arg(path);
    let output = process::output(command, cancel, 64 * 1024)?;
    let probe: Probe =
        serde_json::from_slice(&output.stdout).context("Unreadable media metadata")?;
    let video = probe
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("video"));
    let audio = probe
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("audio"));
    ensure!(
        video || audio,
        "This file contains no supported audio or video stream"
    );
    for stream in &probe.streams {
        ensure!(
            stream.width.unwrap_or(0) <= 8192 && stream.height.unwrap_or(0) <= 8192,
            "This preview supports media up to 8192 pixels per side"
        );
    }
    let duration = probe
        .format
        .and_then(|f| f.duration)
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);
    ensure!(
        duration.is_finite() && (0.0..=43_200.0).contains(&duration),
        "Unsupported media duration"
    );
    let media = Media {
        path: path.into(),
        duration,
        video,
        audio,
    };
    let frame = if video {
        Some(frame(&media, 0.0, cancel)?)
    } else {
        None
    };
    Ok(Inspection { media, frame })
}

pub fn frame(media: &Media, position: f64, cancel: &Cancellation) -> anyhow::Result<Vec<u8>> {
    let mut command = decoder(media, position);
    command.args([
        "-frames:v",
        "1",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "bgra",
        "pipe:1",
    ]);
    let result = process::output(command, cancel, FRAME_BYTES)?;
    ensure!(
        result.stdout.len() == FRAME_BYTES,
        "The selected frame is incomplete"
    );
    Ok(result.stdout)
}

pub(super) fn decoder(media: &Media, position: f64) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-protocol_whitelist",
            "file,pipe",
            "-threads",
            "1",
            "-ss",
        ])
        .arg(position.max(0.0).to_string())
        .arg("-i")
        .arg(&media.path)
        .args(["-an", "-vf", FILTER, "-filter_threads", "1"]);
    command
}
