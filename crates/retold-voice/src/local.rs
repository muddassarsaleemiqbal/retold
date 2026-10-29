//! `LocalVoice` uses the managed Chatterbox or `IndicF5` workers, or in-process Kokoro ONNX inference.
use crate::{EmotionalTag, Error, audio::Wave};
use async_trait::async_trait;
use retold_core::num;
use retold_core::{LanguageTag, Usd};
use retold_infer::{
    models::ModelStore,
    onnx::{Backend, Input, OnnxSession, TensorData},
    worker::{Worker, WorkerSpec},
};
use retold_providers::{
    Billing, ProviderError,
    media::{Media, MediaProvider, MediaUsage, Quote, SpeechRequest},
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Controls {
    pub sample: Option<PathBuf>,
    pub sample_hash: Option<String>,
    /// `IndicF5` conditioning: a ≤15 s cut of `sample` and its transcript.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<(PathBuf, String)>,
    pub exaggeration: f64,
    pub speed: f64,
    pub seed: u32,
    pub cfg_weight: f64,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
}
const fn default_temperature() -> f64 {
    0.8
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            sample: None,
            sample_hash: None,
            reference: None,
            exaggeration: 0.5,
            speed: 1.0,
            seed: 42,
            cfg_weight: 0.5,
            temperature: default_temperature(),
        }
    }
}
const EMOTION_CONTROLS: &[(EmotionalTag, f64, f64)] = &[
    (EmotionalTag::Furious, 0.8, 1.1),
    (EmotionalTag::Terrified, 0.8, 1.1),
    (EmotionalTag::Whispering, 0.25, 0.9),
    (EmotionalTag::Solemn, 0.4, 0.9),
    (EmotionalTag::Melancholy, 0.4, 0.9),
    (EmotionalTag::Joyful, 0.7, 1.05),
    (EmotionalTag::Sarcastic, 0.6, 0.95),
    (EmotionalTag::Neutral, 0.5, 1.0),
];

impl Controls {
    #[must_use]
    pub fn emotion(tag: &EmotionalTag) -> Self {
        let (exaggeration, speed) = EMOTION_CONTROLS
            .iter()
            .find(|(tag_entry, _, _)| tag_entry == tag)
            .map_or((0.5, 1.0), |(_, exaggeration, speed)| {
                (*exaggeration, *speed)
            });
        Self {
            exaggeration,
            speed,
            ..Self::default()
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        if !self.exaggeration.is_finite()
            || !(0.0..=1.0).contains(&self.exaggeration)
            || !self.speed.is_finite()
            || !(0.75..=1.25).contains(&self.speed)
            || !self.cfg_weight.is_finite()
            || !(0.0..=1.0).contains(&self.cfg_weight)
            || !self.temperature.is_finite()
            || !(0.2..=1.0).contains(&self.temperature)
        {
            return Err(Error::Invalid("invalid voice controls".into()));
        }
        if let Some(path) = &self.sample {
            Wave::decode(&std::fs::read(path)?)?.reference()?;
            if self.sample_hash.as_deref()
                != Some(retold_infer::models::file_sha256(path)?.as_str())
            {
                return Err(Error::Invalid(
                    "voice sample changed; refresh casting before resume".into(),
                ));
            }
        }
        Ok(())
    }
}
/// Languages `IndicF5` was trained on.
pub const INDICF5_LANGUAGES: [&str; 11] = [
    "as", "bn", "gu", "hi", "kn", "ml", "mr", "or", "pa", "ta", "te",
];
#[derive(Debug, Clone)]
pub struct LocalVoice {
    id: String,
    store: ModelStore,
    worker: Arc<tokio::sync::Mutex<Option<(String, Worker)>>>,
    // Separate slot: speaker embeddings stay on Chatterbox, so sharing one
    // slot would reload both models on every IndicF5 turn.
    indic: Arc<tokio::sync::Mutex<Option<Worker>>>,
    onnx: OnnxSession,
    pub espeak: PathBuf,
    pub timeout: Duration,
}
impl LocalVoice {
    pub fn new(id: impl Into<String>, espeak: PathBuf, timeout: Duration) -> Result<Self, Error> {
        Ok(Self {
            id: id.into(),
            store: ModelStore::discover()?,
            worker: Arc::new(tokio::sync::Mutex::new(None)),
            indic: Arc::new(tokio::sync::Mutex::new(None)),
            onnx: OnnxSession::default(),
            espeak,
            timeout,
        })
    }
    pub async fn worker(&self) -> Result<Worker, Error> {
        let cached = self.worker.lock().await;
        if let Some((_, worker)) = cached.as_ref() {
            return Ok(worker.clone());
        }
        drop(cached);
        self.worker_for("chatterbox").await
    }
    async fn worker_for(&self, model: &str) -> Result<Worker, Error> {
        let mut cached = self.worker.lock().await;
        if let Some((kind, worker)) = cached.as_ref() {
            if kind == model {
                return Ok(worker.clone());
            }
            worker.shutdown().await;
        }
        let dir = self.store.verify(&self.store.manifest("voice")?)?;
        let mut args = vec![dir.join("worker.py").display().to_string()];
        if model == "chatterbox-hi" {
            let pack = self.store.verify(&self.store.manifest("voice-hi")?)?;
            args.push(pack.display().to_string());
        }
        let worker = Worker::new(WorkerSpec {
            program: dir.join("worker-env/bin/python"),
            args,
            cwd: dir,
            timeout: self.timeout,
            idle_timeout: Duration::from_secs(300),
        });
        *cached = Some((model.into(), worker.clone()));
        Ok(worker)
    }
    async fn indic_worker(&self) -> Result<Worker, Error> {
        let mut cached = self.indic.lock().await;
        if let Some(worker) = cached.as_ref() {
            return Ok(worker.clone());
        }
        let dir = self.store.verify(&self.store.manifest("voice-indic")?)?;
        let worker = Worker::new(WorkerSpec {
            program: dir.join("worker-env/bin/python"),
            args: vec![
                dir.join("indic_worker.py").display().to_string(),
                dir.display().to_string(),
            ],
            cwd: dir,
            timeout: self.timeout,
            idle_timeout: Duration::from_secs(300),
        });
        *cached = Some(worker.clone());
        Ok(worker)
    }
    pub async fn embedding(
        &self,
        path: &std::path::Path,
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, Error> {
        let v = self
            .worker()
            .await?
            .request(
                "embedding",
                serde_json::json!({"path":path.canonicalize()?}),
                cancel,
            )
            .await?;
        Ok(serde_json::from_value(v["embedding"].clone())?)
    }
    pub async fn synthesize(
        &self,
        model: &str,
        req: &SpeechRequest,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>, Error> {
        if req.text.trim().is_empty() {
            return Err(Error::Invalid("empty speech request".into()));
        }
        let controls = request_controls(req)?;
        let language = req.language.language();
        if language == "ur" {
            return Err(Error::Invalid(
                "Urdu needs validated Devanagari text and Hindi synthesis".into(),
            ));
        }
        match model {
            "indicf5" => self.synthesize_indicf5(req, &controls, cancel).await,
            "chatterbox" | "chatterbox-hi" => {
                self.synthesize_chatterbox(model, req, &controls, cancel)
                    .await
            }
            _ => self.synthesize_kokoro(model, req, &controls, cancel).await,
        }
    }

    async fn synthesize_indicf5(
        &self,
        req: &SpeechRequest,
        controls: &Controls,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>, Error> {
        if !INDICF5_LANGUAGES.contains(&req.language.language()) {
            return Err(Error::Invalid(
                "indicf5 needs Indic speech text (Urdu uses validated Hindi transliteration)"
                    .into(),
            ));
        }
        let (Some(_), Some((sample, sample_text))) = (&controls.sample, &controls.reference) else {
            return Err(Error::Invalid(
                "indicf5 needs a voice sample and its transcript".into(),
            ));
        };
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("speech.wav");
        let request = serde_json::json!({
            "text": req.text,
            "sample": sample,
            "sample_text": sample_text,
            "speed": controls.speed,
            "seed": controls.seed,
            "output": path,
        });
        self.indic_worker()
            .await?
            .request("synthesize", request, cancel)
            .await?;
        Ok(std::fs::read(path)?)
    }

    async fn synthesize_chatterbox(
        &self,
        model: &str,
        req: &SpeechRequest,
        controls: &Controls,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>, Error> {
        let language = req.language.language();
        if model == "chatterbox-hi" && language != "hi" {
            return Err(Error::Invalid(
                "chatterbox-hi needs Hindi speech text (Urdu uses validated Hindi transliteration)"
                    .into(),
            ));
        }
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("speech.wav");
        let request = serde_json::json!({
            "text": req.text,
            "language": language,
            "sample": controls.sample,
            "exaggeration": controls.exaggeration,
            "speed": controls.speed,
            "seed": controls.seed,
            "cfg_weight": controls.cfg_weight,
            "temperature": controls.temperature,
            "output": path,
        });
        self.worker_for(model)
            .await?
            .request("synthesize", request, cancel)
            .await?;
        Ok(std::fs::read(path)?)
    }

    async fn synthesize_kokoro(
        &self,
        model: &str,
        req: &SpeechRequest,
        controls: &Controls,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>, Error> {
        let language = req.language.language();
        if model != "kokoro" || !["en", "hi"].contains(&language) || controls.sample.is_some() {
            return Err(Error::Invalid(
                "choose chatterbox or indicf5 for cloning, or kokoro presets for en/hi".into(),
            ));
        }
        let dir = self.store.verify(&self.store.manifest("voice-lite")?)?;
        if !KOKORO_PRESETS.contains(&req.voice.as_str()) {
            return Err(Error::Invalid(
                "unknown Kokoro preset; choose af_heart/am_adam/hf_alpha/hm_omega".into(),
            ));
        }
        let tokenizer: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("tokenizer.json"))?)?;
        let style = std::fs::read(dir.join("voices").join(format!("{}.bin", req.voice)))?;
        let mut samples = vec![0.0; KOKORO_LEAD_IN_SAMPLES];
        let (espeak_voice, rewrite_ipa): (&str, fn(&str) -> String) = if language == "en" {
            ("en-us", kokoro_english_ipa)
        } else {
            ("hi", kokoro_hindi_ipa)
        };
        for sentence in kokoro_segments(language, &req.text) {
            let espeak = self.phonemes(espeak_voice, sentence, &dir, cancel).await?;
            let phonemes = rewrite_ipa(&espeak);
            let tokens = kokoro_tokens(&phonemes, &tokenizer)?;
            let audio = self
                .kokoro_turn(&dir, tokens, &style, controls.speed, cancel)
                .await?;
            samples.extend(audio);
            samples.extend(std::iter::repeat_n(0.0, KOKORO_PAUSE_SAMPLES));
        }
        Wave {
            rate: KOKORO_RATE,
            samples,
        }
        .encode()
    }

    /// Raw IPA for one sentence, from espeak-ng.
    async fn phonemes(
        &self,
        voice: &str,
        sentence: &str,
        dir: &std::path::Path,
        cancel: &CancellationToken,
    ) -> Result<String, Error> {
        let args = ["-q", "--ipa=3", "-v", voice, "--stdin"]
            .map(String::from)
            .to_vec();
        let limits = retold_providers::process::Limits {
            timeout: self.timeout,
            ..Default::default()
        };
        let result =
            retold_providers::process::run(&self.espeak, &args, sentence, dir, &limits, cancel)
                .await?;
        if result.status != Some(0) {
            return Err(Error::Invalid(format!(
                "phonemizer failed: {}; install espeak-ng",
                result.stderr
            )));
        }
        Ok(result.stdout)
    }

    /// One Kokoro inference: the waveform for `tokens` in the preset's style.
    async fn kokoro_turn(
        &self,
        dir: &std::path::Path,
        tokens: Vec<i64>,
        style: &[u8],
        speed: f64,
        cancel: &CancellationToken,
    ) -> Result<Vec<f32>, Error> {
        let offset = tokens.len() * KOKORO_STYLE_WIDTH * size_of::<f32>();
        let row = style
            .get(offset..offset + KOKORO_STYLE_WIDTH * size_of::<f32>())
            .ok_or_else(|| Error::Invalid("Kokoro style row missing".into()))?;
        let style_values = row
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect();
        let token_count = tokens.len();
        let mut ids = Vec::with_capacity(token_count + 2);
        ids.push(0);
        ids.extend(tokens);
        ids.push(0);
        let id_count = i64::try_from(ids.len()).map_err(|e| Error::Invalid(e.to_string()))?;
        let inputs = vec![
            Input {
                name: "input_ids".into(),
                shape: vec![1, id_count],
                data: TensorData::I64(ids),
            },
            Input {
                name: "style".into(),
                shape: vec![1, i64::try_from(KOKORO_STYLE_WIDTH).unwrap_or(i64::MAX)],
                data: TensorData::F32(style_values),
            },
            Input {
                name: "speed".into(),
                shape: vec![1],
                data: TensorData::F32(vec![num::narrow_f32(speed)]),
            },
        ];
        let outputs = self
            .onnx
            .run(&dir.join("onnx/model.onnx"), Backend::Cpu, inputs, cancel)
            .await?;
        let audio = outputs
            .into_iter()
            .find(|output| output.name == "waveform" || output.name == "audio")
            .ok_or_else(|| Error::Invalid("Kokoro waveform output missing".into()))?;
        Ok(audio.values)
    }
}

const KOKORO_PRESETS: [&str; 4] = ["af_heart", "am_adam", "hf_alpha", "hm_omega"];
/// Kokoro's output sample rate.
const KOKORO_RATE: u32 = 24_000;
/// Silence before the first sentence (0.15 s) and after each sentence (0.12 s).
const KOKORO_LEAD_IN_SAMPLES: usize = 3_600;
const KOKORO_PAUSE_SAMPLES: usize = 2_880;
/// Floats per row of a Kokoro voice style file; the row index is the token count.
const KOKORO_STYLE_WIDTH: usize = 256;
const KOKORO_MAX_PHONEMES: usize = 500;
/// espeak's multi-character IPA, rewritten to the single symbols in Kokoro's vocabulary.
const KOKORO_SYMBOLS: [(&str, &str); 10] = [
    ("aɪ", "I"),
    ("aʊ", "W"),
    ("dʒ", "ʤ"),
    ("eɪ", "A"),
    ("oʊ", "O"),
    ("əʊ", "Q"),
    ("tʃ", "ʧ"),
    ("ɔɪ", "Y"),
    ("ɚ", "əɹ"),
    ("ɐ", "ə"),
];

/// Speech controls carried as JSON in the request's instructions, or the defaults.
fn request_controls(req: &SpeechRequest) -> Result<Controls, Error> {
    let controls: Controls = req
        .instructions
        .as_deref()
        .filter(|s| s.starts_with('{'))
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default();
    controls.validate()?;
    Ok(controls)
}

/// Text split into one inference per sentence, keeping each sentence's pause mark. The
/// phonemizer drops punctuation, and one long flattened turn can skip words.
fn kokoro_segments<'a>(language: &str, text: &'a str) -> Vec<&'a str> {
    if language == "hi" {
        // Hindi presets expect an uninterrupted IPA phrase; Latin stop tokens can produce
        // extra spoken syllables at every sentence end.
        return vec![text];
    }
    text.split_inclusive(['.', '!', '?', '।', '؟'])
        .filter(|sentence| !sentence.trim().is_empty())
        .collect()
}

/// espeak IPA rewritten into Kokoro's shared phoneme alphabet.
fn kokoro_base_ipa(espeak: &str) -> String {
    let mut ipa = espeak.trim().replace('\u{200d}', "");
    for (from, to) in KOKORO_SYMBOLS {
        ipa = ipa.replace(from, to);
    }
    ipa.replace(['\u{200d}', '_', '\n'], " ")
}

fn kokoro_hindi_ipa(espeak: &str) -> String {
    // espeak encodes the Hindi retroflex flap as r.; the ONNX vocabulary uses IPA ɽ and
    // treats the period as a pause.
    kokoro_base_ipa(espeak).replace("r.", "ɽ")
}

fn kokoro_english_ipa(espeak: &str) -> String {
    // Keep the stressed standalone pronoun; espeak's unstressed diphthong can be swallowed
    // before "will".
    let mut ipa = kokoro_base_ipa(espeak)
        .split_whitespace()
        .map(|word| if word == "I" { "ˈI" } else { word })
        .collect::<Vec<_>>()
        .join(" ");
    ipa.push('.');
    ipa
}

/// Kokoro vocabulary ids for each phoneme the tokenizer knows.
fn kokoro_tokens(ipa: &str, tokenizer: &serde_json::Value) -> Result<Vec<i64>, Error> {
    let vocabulary = &tokenizer["model"]["vocab"];
    let tokens: Vec<i64> = ipa
        .chars()
        .filter_map(|symbol| vocabulary[symbol.to_string()].as_i64())
        .collect();
    if tokens.is_empty() || tokens.len() > KOKORO_MAX_PHONEMES {
        return Err(Error::Invalid(
            "Kokoro turn needs 1–500 phonemes; split long dialogue turns".into(),
        ));
    }
    Ok(tokens)
}
#[expect(
    clippy::needless_pass_by_value,
    reason = "used as `map_err(error)`, which passes by value"
)]
fn error(e: Error) -> ProviderError {
    if matches!(e, Error::Infer(retold_infer::Error::Cancelled)) {
        ProviderError::Cancelled
    } else {
        ProviderError::Unavailable(e.to_string())
    }
}
#[async_trait]
impl MediaProvider for LocalVoice {
    async fn speaker_embedding(
        &self,
        path: &std::path::Path,
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, ProviderError> {
        self.embedding(path, cancel).await.map_err(error)
    }
    fn id(&self) -> &str {
        &self.id
    }
    fn billing(&self) -> Billing {
        Billing::Subscription
    }
    async fn speech_quote(
        &self,
        _: &str,
        _: &SpeechRequest,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        Ok(Quote {
            ceiling: Usd::ZERO,
            provider_tag: None,
        })
    }
    async fn speech(
        &self,
        model: &str,
        req: &SpeechRequest,
        cancel: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError> {
        Ok(Media {
            value: self.synthesize(model, req, cancel).await.map_err(error)?,
            usage: MediaUsage {
                generation: None,
                cost: Some(Usd::ZERO),
            },
        })
    }
    async fn image_quote(
        &self,
        _: &str,
        _: &retold_providers::media::ImageRequest,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        Err(ProviderError::Unsupported(
            "LocalVoice only supports speech".into(),
        ))
    }
    async fn transcript_quote(
        &self,
        _: &str,
        _: f64,
        _: &CancellationToken,
    ) -> Result<Quote, ProviderError> {
        Err(ProviderError::Unsupported(
            "LocalVoice only supports speech".into(),
        ))
    }
    async fn image(
        &self,
        _: &str,
        _: &retold_providers::media::ImageRequest,
        _: &Quote,
        _: &CancellationToken,
    ) -> Result<Media<Vec<u8>>, ProviderError> {
        Err(ProviderError::Unsupported(
            "LocalVoice only supports speech".into(),
        ))
    }
    async fn transcribe(
        &self,
        _: &str,
        _: &[u8],
        _: &str,
        _: &LanguageTag,
        _: &CancellationToken,
    ) -> Result<Media<retold_providers::media::Transcript>, ProviderError> {
        Err(ProviderError::Unsupported(
            "LocalVoice only supports speech".into(),
        ))
    }
}
