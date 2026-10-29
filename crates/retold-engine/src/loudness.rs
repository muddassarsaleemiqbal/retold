//! Measure FilmCraft's normalized audio before rendering video. Its limiter makes requested
//! and delivered LUFS differ; previews calibrate that difference without another AI call.
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use retold_filmcraft::{Command, editor, session::Session};
use serde::{Deserialize, Serialize};
use tokio::process::Command as Process;

use crate::context::Context;
use crate::workflow::{Delivery, WorkflowError};
use crate::{EngineError, Stage, fsx};

const TARGET: f64 = -14.0;
#[derive(Debug, Serialize, Deserialize)]
struct Preview {
    requested: f64,
    measured: f64,
    path: PathBuf,
    hash: String,
}
#[derive(Debug, Serialize, Deserialize)]
struct Calibration {
    export_lufs: f64,
    previews: Vec<Preview>,
}

pub async fn calibrate(
    context: &Context,
    session: &Session,
    built: &editor::BuiltSequence,
    delivery: &Delivery,
    key: &str,
) -> Result<f64, WorkflowError> {
    let step = format!("loudness-{}", delivery.id);
    let cached: Option<Calibration> = context.media.store.lock().await.cached(&step, key)?;
    if let Some(record) = cached {
        let last = record
            .previews
            .last()
            .ok_or_else(|| EngineError::Setup("empty loudness calibration".into()))?;
        if blake3::hash(&std::fs::read(&last.path).map_err(|e| EngineError::io(&last.path, e))?)
            .to_hex()
            .as_str()
            != last.hash
        {
            return Err(EngineError::Setup("saved loudness preview changed".into()).into());
        }
        return Ok(record.export_lufs);
    }
    context.media.store.lock().await.begin(&step)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| EngineError::Setup(e.to_string()))?
        .as_nanos();
    let mut requested = TARGET;
    let mut previews = Vec::new();
    for attempt in 0..8 {
        let path = context.layout.stage(Stage::Edit).join(format!(
            "{}.preview-{key}-{nonce}-{attempt}.wav",
            delivery.id
        ));
        let result = session
            .client
            .export_approved(
                &Command::ExportAudio {
                    path: path.clone(),
                    sequence: built.sequence.clone(),
                    loudness_lufs: requested,
                },
                &context.media.cancel,
            )
            .await?;
        let job = result
            .get("job")
            .ok_or_else(|| EngineError::Setup("audio preview returned no job".into()))?;
        editor::wait_job(&session.client, job, &context.media.cancel).await?;
        let measured = measure(context, &path).await?;
        let bytes = std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?;
        previews.push(Preview {
            requested,
            measured,
            path,
            hash: blake3::hash(&bytes).to_hex().to_string(),
        });
        if (measured - TARGET).abs() <= 0.05 {
            let record = Calibration {
                export_lufs: requested,
                previews,
            };
            fsx::write_json_atomic(
                &context
                    .layout
                    .stage(Stage::Edit)
                    .join(format!("{}.loudness.json", delivery.id)),
                &record,
            )?;
            context
                .media
                .store
                .lock()
                .await
                .commit(&step, key, &record, None)?;
            return Ok(requested);
        }
        requested = correction(requested, measured)?;
    }
    Err(EngineError::Setup(
        "FilmCraft loudness did not converge to -14 LUFS in eight audio-only previews".into(),
    )
    .into())
}

fn correction(requested: f64, measured: f64) -> Result<f64, EngineError> {
    let next = requested + TARGET - measured;
    if !measured.is_finite() || !(-20.0..=-8.0).contains(&next) {
        return Err(EngineError::Setup(
            "loudness correction exceeds the bounded export range".into(),
        ));
    }
    Ok(next)
}

pub async fn measure(context: &Context, path: &Path) -> Result<f64, WorkflowError> {
    let binary = std::env::var_os("RETOLD_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let mut process = Process::new(binary);
    process
        .args(["-hide_banner", "-i"])
        .arg(path)
        .args(["-vn", "-af", "ebur128=peak=true", "-f", "null", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::select! {
        ()=context.media.cancel.cancelled()=>return Err(retold_filmcraft::Error::Cancelled.into()),
        output=tokio::time::timeout(Duration::from_secs(120),process.output())=>output.map_err(|_|EngineError::Setup("loudness measurement timed out".into()))?.map_err(|e|EngineError::io(path,e))?,
    };
    if !output.status.success() {
        return Err(
            EngineError::Setup("FFmpeg could not measure the exported audio".into()).into(),
        );
    }
    let log = String::from_utf8_lossy(&output.stderr);
    fsx::write_atomic(&path.with_extension("loudness.log"), log.as_bytes())?;
    parse(&log).map_err(Into::into)
}

fn parse(log: &str) -> Result<f64, EngineError> {
    let measured = log
        .lines()
        .filter_map(|line| line.trim().strip_prefix("I:"))
        .filter_map(|line| line.split_whitespace().next()?.parse::<f64>().ok())
        .next_back();
    measured
        .filter(|n| n.is_finite() && (-70.0..=0.0).contains(n))
        .ok_or_else(|| EngineError::Setup("FFmpeg returned no finite integrated loudness".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_summary_and_bounded_feedback_are_required() {
        assert_eq!(parse("I: -20.0 LUFS\nI: -14.0 LUFS").unwrap(), -14.0);
        assert!(parse("I: NaN LUFS").is_err());
        assert!(parse("I: -inf LUFS").is_err());
        assert!((correction(-14.0, -14.6).unwrap() + 13.4).abs() < 1e-9);
        assert!(correction(-14.0, -40.0).is_err());
        assert!(correction(-14.0, f64::NAN).is_err());
    }
}
