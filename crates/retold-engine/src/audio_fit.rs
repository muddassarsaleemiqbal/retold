//! Bounded, pitch-preserving timing fit of saved narration. Makes no provider calls.
use crate::{
    EngineError,
    context::Context,
    fsx,
    media_runtime::{Asset, MediaError},
    store::hash,
};
use retold_providers::{media::audio_duration, process};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct Report {
    source: Asset,
    output: Asset,
    tempo: f64,
    original_seconds: f64,
    result_seconds: f64,
}

pub struct FittedAudio {
    pub asset: Asset,
    pub time_scale: f64,
    pub duration: f64,
}

pub async fn fit(context: &Context, audio: Asset) -> Result<FittedAudio, MediaError> {
    audio.verify()?;
    let bytes = std::fs::read(&audio.path).map_err(|e| EngineError::io(&audio.path, e))?;
    let original_seconds = audio_duration(&bytes)?;
    let m = context.guide.manifest();
    let Some(tempo) = plan(
        original_seconds,
        m.format.target_seconds,
        m.edit.ending.seconds,
    )?
    else {
        return Ok(FittedAudio {
            asset: audio,
            time_scale: 1.0,
            duration: original_seconds,
        });
    };
    let key = hash(
        &serde_json::json!({"version":1,"source":audio.content_hash,"range":m.format.target_seconds,"hold":m.edit.ending.seconds,"rate":tempo}),
    )?;
    let parent = audio
        .path
        .parent()
        .ok_or_else(|| EngineError::Setup("narration has no parent directory".into()))?;
    let report_path = parent.join(format!("fit-{key}.json"));
    if report_path.exists() {
        let report: Report = serde_json::from_slice(
            &std::fs::read(&report_path).map_err(|e| EngineError::io(&report_path, e))?,
        )
        .map_err(|e| EngineError::Setup(e.to_string()))?;
        if report.source.content_hash != audio.content_hash || (report.tempo - tempo).abs() > 1e-9 {
            return Err(MediaError::Quality(
                "saved audio-fit provenance changed".into(),
            ));
        }
        report.output.verify()?;
        return Ok(FittedAudio {
            asset: report.output,
            time_scale: report.result_seconds / report.original_seconds,
            duration: report.result_seconds,
        });
    }
    let bytes = render(context, &audio.path, tempo).await?;
    let result_seconds = audio_duration(&bytes)?;
    if plan(
        result_seconds,
        m.format.target_seconds,
        m.edit.ending.seconds,
    )?
    .is_some()
    {
        return Err(MediaError::Quality(
            "fitted narration still misses the guide duration".into(),
        ));
    }
    let mut output = audio.clone();
    output.path = parent.join(format!("narration-fit-{key}.wav"));
    output.content_hash = blake3::hash(&bytes).to_hex().to_string();
    fsx::write_atomic(&output.path, &bytes)?;
    fsx::write_json_atomic(
        &report_path,
        &Report {
            source: audio,
            output: output.clone(),
            tempo,
            original_seconds,
            result_seconds,
        },
    )?;
    Ok(FittedAudio {
        asset: output,
        time_scale: result_seconds / original_seconds,
        duration: result_seconds,
    })
}

async fn render(context: &Context, source: &Path, tempo: f64) -> Result<Vec<u8>, MediaError> {
    let temp = tempfile::tempdir().map_err(|e| EngineError::Setup(e.to_string()))?;
    let wav = temp.path().join("fitted.wav");
    let binary = std::env::var_os("RETOLD_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let result = process::run(
        Path::new(&binary),
        &[
            "-nostdin".into(),
            "-v".into(),
            "error".into(),
            "-i".into(),
            source.display().to_string(),
            "-vn".into(),
            "-af".into(),
            format!("atempo={tempo:.12}"),
            "-ar".into(),
            "24000".into(),
            "-ac".into(),
            "1".into(),
            "-c:a".into(),
            "pcm_s16le".into(),
            wav.display().to_string(),
        ],
        "",
        temp.path(),
        &process::Limits::default(),
        &context.media.cancel,
    )
    .await?;
    if result.status != Some(0) {
        return Err(MediaError::Quality(format!(
            "local narration timing fit failed: {}",
            result.stderr
        )));
    }
    let bytes = std::fs::read(&wav).map_err(|e| EngineError::io(&wav, e))?;
    Ok(bytes)
}

const MIN_TEMPO: f64 = 0.92;
const MAX_TEMPO: f64 = 1.08;

fn plan(seconds: f64, range: [f64; 2], hold: f64) -> Result<Option<f64>, MediaError> {
    let [lo, hi] = range;
    if !seconds.is_finite()
        || seconds <= 0.0
        || !hold.is_finite()
        || hold < 0.0
        || hi <= hold
        || lo > hi
    {
        return Err(MediaError::Quality(
            "invalid narration timing inputs".into(),
        ));
    }
    if (lo..=hi).contains(&(seconds + hold)) {
        return Ok(None);
    }
    let margin = 0.25_f64.min((hi - lo) / 4.0);
    let target = if seconds + hold > hi {
        hi - hold - margin
    } else {
        lo - hold + margin
    };
    let tempo = seconds / target;
    // A pitch-preserving tempo change of up to 8% is not noticeable in narration.
    if !(MIN_TEMPO..=MAX_TEMPO).contains(&tempo) {
        return Err(MediaError::Quality(format!(
            "narration needs tempo {tempo:.3}, outside the allowed {MIN_TEMPO}–{MAX_TEMPO}; revise the voice or script"
        )));
    }
    Ok(Some(tempo))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_small_timing_corrections_are_accepted() {
        assert!(plan(94.92, [95.0, 115.0], 1.2).unwrap().is_none());
        let tempo = plan(114.7, [95.0, 115.0], 1.2).unwrap().unwrap();
        assert!(tempo > 1.0 && tempo < 1.02);
        assert!(plan(114.7 / tempo, [95.0, 115.0], 1.2).unwrap().is_none());
        assert!(plan(150.0, [95.0, 115.0], 1.2).is_err());
        assert!(plan(f64::NAN, [95.0, 115.0], 1.2).is_err());
    }
}
