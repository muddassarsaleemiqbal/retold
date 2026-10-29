//! Cached local acoustic phoneme alignment; never estimates phoneme durations from word length.
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock, time::Duration};

use retold_infer::{
    models::ModelStore,
    worker::{Worker, WorkerSpec},
};
use retold_lipsync::viseme::{Phoneme, SpokenWord};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    EngineError, context::Context, languages::Edition, media_runtime::MediaError, store::hash,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alignment {
    pub phonemes: Vec<Phoneme>,
    pub stride_seconds: f64,
    pub model_revision: String,
}

/// Acoustic timing backend, separated from caching so fixtures never need model weights.
#[async_trait::async_trait]
pub trait AcousticAligner: std::fmt::Debug + Send + Sync {
    fn identity(&self) -> Result<String, MediaError>;
    async fn phonemize(
        &self,
        language: &str,
        words: &[retold_lang::TimedWord],
    ) -> Result<Vec<SpokenWord>, MediaError> {
        let language = language.to_owned();
        let words = words.to_vec();
        tokio::task::spawn_blocking(move || {
            let espeak = std::env::var_os("RETOLD_ESPEAK")
                .map_or_else(|| PathBuf::from("espeak-ng"), PathBuf::from);
            words
                .into_iter()
                .map(|w| {
                    Ok(SpokenWord {
                        start: w.start,
                        end: w.end,
                        ipa: retold_lipsync::viseme::ipa(&espeak, &language, &w.text)
                            .map_err(setup)?,
                    })
                })
                .collect()
        })
        .await
        .map_err(setup)?
    }
    async fn align(
        &self,
        audio: &std::path::Path,
        words: &[SpokenWord],
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<Alignment, MediaError>;
}

#[derive(Debug, Default)]
pub struct LocalAligner;
#[async_trait::async_trait]
impl AcousticAligner for LocalAligner {
    fn identity(&self) -> Result<String, MediaError> {
        let manifest = ModelStore::discover()
            .map_err(setup)?
            .manifest("phonemes")
            .map_err(setup)?;
        hash(&(
            &manifest,
            include_str!("../../retold-infer/src/phoneme_worker.py"),
        ))
        .map_err(Into::into)
    }
    async fn align(
        &self,
        audio: &std::path::Path,
        words: &[SpokenWord],
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<Alignment, MediaError> {
        let response = worker()
            .await?
            .request(
                "align",
                serde_json::json!({"audio":audio,"words":words}),
                cancel,
            )
            .await
            .map_err(worker_error)?;
        serde_json::from_value(response).map_err(setup)
    }
}

fn worker_error(error: retold_infer::Error) -> MediaError {
    match error {
        retold_infer::Error::Invalid(ref detail) if detail.starts_with("worker: \"quality: ") => {
            MediaError::Quality(detail.clone())
        }
        retold_infer::Error::Cancelled => retold_providers::ProviderError::Cancelled.into(),
        other => setup(other),
    }
}

async fn worker() -> Result<Worker, MediaError> {
    static WORKERS: OnceLock<Mutex<BTreeMap<PathBuf, Worker>>> = OnceLock::new();
    let store = ModelStore::discover().map_err(setup)?;
    let manifest = store.manifest("phonemes").map_err(setup)?;
    let dir = store.directory(&manifest);
    let mut workers = WORKERS.get_or_init(Mutex::default).lock().await;
    if let Some(worker) = workers.get(&dir) {
        return Ok(worker.clone());
    }
    store.verify(&manifest).map_err(setup)?;
    let worker = Worker::new(WorkerSpec {
        program: dir.join("worker-env/bin/python"),
        args: vec![
            dir.join("phoneme_worker.py").display().to_string(),
            dir.display().to_string(),
        ],
        cwd: dir.clone(),
        timeout: Duration::from_secs(300),
        idle_timeout: Duration::from_secs(120),
    });
    workers.insert(dir, worker.clone());
    Ok(worker)
}

fn setup(error: impl std::fmt::Display) -> MediaError {
    EngineError::Setup(error.to_string()).into()
}

/// Times use the edition audio clock. Cache includes the audio, word windows,
/// IPA, model revision and embedded worker code, so repairs cannot reuse stale timing.
pub async fn align(
    context: &Context,
    edition: &Edition,
    index: usize,
    words: &[SpokenWord],
) -> Result<Alignment, MediaError> {
    let key = hash(&(
        1,
        &edition.audio.content_hash,
        words,
        context.phonemes.identity()?,
    ))?;
    let step = format!(
        "phonemes-{}-{index}-{}",
        edition.variant.narration,
        &key[..16]
    )
    .to_ascii_lowercase();
    edition.audio.verify()?;
    let saved = context
        .media
        .store
        .lock()
        .await
        .cached::<Alignment>(&step, &key)?;
    if let Some(saved) = saved {
        return Ok(saved);
    }
    context.media.store.lock().await.begin(&step)?;
    let result = context
        .phonemes
        .align(&edition.audio.path, words, &context.media.cancel)
        .await?;
    validate(&result, words)?;
    let path = context
        .layout
        .video_dir()
        .join(format!("work/mouths/{step}.json"));
    crate::fsx::write_json_atomic(&path, &result)?;
    context
        .media
        .store
        .lock()
        .await
        .commit(&step, &key, &result, None)?;
    Ok(result)
}

fn validate(alignment: &Alignment, words: &[SpokenWord]) -> Result<(), MediaError> {
    let mut confidence: Vec<_> = alignment.phonemes.iter().map(|p| p.confidence).collect();
    confidence.sort_by(f64::total_cmp);
    let valid = !confidence.is_empty()
        && confidence[confidence.len() / 2] >= 0.2
        && !alignment.model_revision.is_empty()
        && !alignment.phonemes.is_empty()
        && alignment.stride_seconds.is_finite()
        && alignment.stride_seconds > 0.0
        && alignment.stride_seconds <= 0.021
        && alignment.phonemes.iter().all(|p| {
            p.start.is_finite()
                && p.end.is_finite()
                && p.start >= 0.0
                && p.end > p.start
                && p.confidence.is_finite()
                && (0.0..=1.0).contains(&p.confidence)
                && !p.ipa.is_empty()
                && words
                    .iter()
                    .any(|w| p.start >= w.start - 0.13 && p.end <= w.end + 0.13)
        })
        && alignment
            .phonemes
            .windows(2)
            .all(|p| p[0].end <= p[1].start + 0.001);
    if !valid {
        return Err(MediaError::Quality(
            "invalid acoustic phoneme alignment".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_audio_rejections_allow_still_fallback() {
        assert!(matches!(
            worker_error(retold_infer::Error::Invalid(
                "worker: \"quality: audio is silent\"".into()
            )),
            MediaError::Quality(_)
        ));
        assert!(!matches!(
            worker_error(retold_infer::Error::Invalid(
                "worker: \"No module named torch\"".into()
            )),
            MediaError::Quality(_)
        ));
        assert!(!matches!(
            worker_error(retold_infer::Error::Cancelled),
            MediaError::Quality(_)
        ));
    }
    #[test]
    fn rejects_nonfinite_overlapping_and_outside_speech_timing() {
        let words = [SpokenWord {
            start: 1.0,
            end: 2.0,
            ipa: "ma".into(),
        }];
        let phone = Phoneme {
            ipa: "m".into(),
            start: 1.0,
            end: 1.2,
            confidence: 0.99,
        };
        let mut alignment = Alignment {
            phonemes: vec![phone.clone()],
            stride_seconds: 0.02,
            model_revision: "fixture".into(),
        };
        assert!(validate(&alignment, &words).is_ok());
        alignment.phonemes[0].start = f64::NAN;
        assert!(validate(&alignment, &words).is_err());
        alignment.phonemes = vec![phone.clone(), phone.clone()];
        assert!(validate(&alignment, &words).is_err());
        alignment.phonemes = vec![Phoneme {
            start: 4.0,
            end: 4.2,
            ..phone
        }];
        assert!(validate(&alignment, &words).is_err());
    }
}
