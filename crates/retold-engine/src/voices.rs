//! Durable per-turn dialogue synthesis, pronunciation QA and caption-preserving alignment.
use crate::Stage;
use crate::{
    EngineError,
    context::Context,
    fsx,
    languages::LocalUnit,
    media_runtime::{Asset, MediaError},
    store::hash,
};
use retold_core::{LanguageTag, Usd};
use retold_guide::channel::VoiceSpec;
use retold_lang::align::{Alignment, SpokenWord, TimedUnit};
use retold_providers::{
    config::ProviderConfig,
    media::{SpeechRequest, Transcript},
};
use retold_voice::{
    DialogueScript, DialogueTurn, EmotionalTag,
    audio::{Wave, accuracy, cosine},
    local::Controls,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub unit: usize,
    pub speaker: String,
    pub emotion: EmotionalTag,
    pub text: String,
    pub speech_text: String,
    pub start: f64,
    pub end: f64,
    pub track: String,
    pub audio: Asset,
    pub timing: TimedUnit,
    pub accuracy: f64,
    pub speaker_similarity: Option<f64>,
    pub fallback: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub turns: Vec<Turn>,
    pub room_tone: Option<Asset>,
    pub audio: Asset,
    pub alignment: Alignment,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Casting {
    voice: VoiceSpec,
    sample: Option<PathBuf>,
    sample_hash: Option<String>,
    sample_language: LanguageTag,
}
/// Local models that clone a reference sample.
const CLONING: [&str; 3] = ["chatterbox", "chatterbox-hi", "indicf5"];
fn setup(e: impl std::fmt::Display) -> MediaError {
    EngineError::Setup(e.to_string()).into()
}
#[must_use]
pub fn file(context: &Context, lang: &LanguageTag) -> PathBuf {
    context
        .layout
        .stage(Stage::Voice)
        .join(lang.to_string())
        .join("turns.json")
}
#[must_use]
pub fn local_narrator(context: &Context, lang: &LanguageTag) -> bool {
    context
        .channel
        .voice(lang)
        .is_ok_and(|v| local(context, &v.provider))
}
pub(crate) fn local(context: &Context, id: &str) -> bool {
    context
        .media
        .registry
        .config
        .providers
        .get(id)
        .is_some_and(|p| matches!(p, ProviderConfig::LocalVoice { .. }))
}
pub fn parse(
    units: &[LocalUnit],
    speakers: &[String],
) -> Result<Vec<(usize, DialogueTurn)>, MediaError> {
    let mut turns = Vec::new();
    for unit in units {
        let script = DialogueScript::parse(&unit.text).map_err(setup)?;
        script.validate_speakers(speakers).map_err(setup)?;
        turns.extend(script.turns.into_iter().map(|t| (unit.id, t)));
    }
    Ok(turns)
}
#[expect(
    clippy::too_many_lines,
    reason = "split pending (Phase 0 step 3c); counted by the clean-code baseline"
)]
async fn cast(
    context: &Context,
    lang: &LanguageTag,
    speaker: &str,
    index: usize,
) -> Result<Casting, MediaError> {
    let narrator = context
        .channel
        .voice(lang)
        .map_err(MediaError::Unavailable)?;
    let mut voice = context
        .channel
        .file()
        .character_voices
        .get(speaker)
        .unwrap_or(narrator)
        .clone();
    let mut sample_language = voice
        .sample_language
        .clone()
        .unwrap_or_else(|| context.guide.manifest().format.source_language.clone());
    let mut sample = voice
        .sample
        .as_ref()
        .map(|s| context.channel.library_path(s));
    if speaker == "NARRATOR"
        && local(context, &voice.provider)
        && CLONING.contains(&voice.model.as_str())
        && sample.is_none()
    {
        let p = context.channel.library_path("voice-sample.wav");
        if !p.exists() {
            return Err(MediaError::Unavailable("dialogue narrator needs a 10–30 second channel voice-sample.wav or voices.<lang>.sample".into()));
        }
        sample = Some(p);
    }
    if speaker != "NARRATOR"
        && !context
            .channel
            .file()
            .character_voices
            .contains_key(speaker)
        && local(context, &voice.provider)
    {
        let presets = ["am_adam", "af_heart", "hm_omega", "hf_alpha"];
        let preset = presets.get(index).ok_or_else(|| {
            MediaError::Quality(
                "more than four automatic characters need explicit character_voices overrides"
                    .into(),
            )
        })?;
        let directory = context.layout.cast_dir().join("voices");
        let label = blake3::hash(speaker.as_bytes()).to_hex().to_string();
        let path = directory.join(format!("{label}-{preset}.wav"));
        // The actual preset is recorded by immutable name, independent of encounter order across runs.
        let record = directory.join(format!("{label}.json"));
        std::fs::create_dir_all(&directory).map_err(setup)?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("roster.lock"))
            .map_err(setup)?;
        fs2::FileExt::lock_exclusive(&lock).map_err(setup)?;
        let chosen = if record.exists() {
            read::<String>(&record)?
        } else {
            let used: Vec<String> = std::fs::read_dir(&directory)
                .map_err(setup)?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .filter_map(|p| read::<String>(&p).ok())
                .collect();
            let cards =
                crate::assets::read_cards(&context.layout.cast_dir().join("characters.json"))?;
            let described = cards
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(speaker))
                .map_or("", |(_, card)| card.voice.as_str());
            let available = preset_order(&presets, described, lang)
                .into_iter()
                .find(|p| !used.iter().any(|u| u == *p))
                .ok_or_else(|| {
                    MediaError::Quality(
                        "automatic cast presets exhausted; add character_voices overrides".into(),
                    )
                })?;
            fsx::write_json_atomic(&record, &available).map_err(setup)?;
            available.into()
        };
        fs2::FileExt::unlock(&lock).map_err(setup)?;
        drop(lock);
        if !presets.contains(&chosen.as_str()) {
            return Err(MediaError::Quality("invalid shared cast preset".into()));
        }
        let path = if chosen == *preset {
            path
        } else {
            directory.join(format!("{label}-{chosen}.wav"))
        };
        if voice.model == "kokoro" {
            voice.voice = chosen;
            return Ok(Casting {
                voice,
                sample: None,
                sample_hash: None,
                sample_language,
            });
        }
        let chosen_language = if chosen.starts_with('h') { "hi" } else { "en" };
        sample_language = chosen_language.parse().map_err(setup)?;
        let request = SpeechRequest {
            text: if chosen.starts_with('h') {
                "यह एक साफ़ आवाज़ का नमूना है। हम आज साथ बैठकर अपनी कहानी सुनाते हैं और हर बात धीरे धीरे समझते हैं। हर व्यक्ति की आवाज़ अलग है और हम ध्यान से हर शब्द को सुनते हैं।".into()
            } else {
                "This is a clear reference recording for a fictional character. We speak calmly together, listen carefully, and describe what happens in the story today. Every person has a different voice, and each voice is easy to recognize.".into()
            },
            voice: chosen,
            instructions: None,
            language: if chosen_language == "hi" { "hi" } else { "en" }
                .parse()
                .map_err(setup)?,
        };
        let asset = context
            .media
            .speech(
                &format!("cast-voice-{label}"),
                &voice.provider,
                "kokoro",
                &request,
                &path,
            )
            .await?;
        sample = Some(asset.path);
    }
    if local(context, &voice.provider)
        && CLONING.contains(&voice.model.as_str())
        && sample.is_none()
    {
        return Err(MediaError::Unavailable(format!(
            "speaker {speaker} needs a sample for voice cloning; set character_voices.<speaker>.sample or choose a Kokoro preset"
        )));
    }
    let sample_hash = if let Some(p) = &sample {
        let canonical = p.canonicalize().map_err(setup)?;
        let root = context.channel.dir().canonicalize().map_err(setup)?;
        let cast_root = context.layout.cast_dir().canonicalize().ok();
        if !canonical.starts_with(root) && !cast_root.is_some_and(|p| canonical.starts_with(p)) {
            return Err(MediaError::Quality(
                "voice sample escapes the channel".into(),
            ));
        }
        Wave::decode(&std::fs::read(p).map_err(setup)?)
            .and_then(|w| w.reference())
            .map_err(setup)?;
        Some(retold_infer::models::file_sha256(p).map_err(setup)?)
    } else {
        None
    };
    Ok(Casting {
        voice,
        sample,
        sample_hash,
        sample_language,
    })
}
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Pronunciation {
    text: String,
}
/// Presets in casting preference: the character's gender first (from their card's voice
/// description), the run language's accent before the other, then the rest.
fn preset_order<'a>(presets: &[&'a str], described: &str, lang: &LanguageTag) -> Vec<&'a str> {
    let words: Vec<_> = described
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_owned)
        .collect();
    let has = |list: &[&str]| words.iter().any(|w| list.contains(&w.as_str()));
    let gender = if has(&["female", "woman", "girl", "feminine", "lady"]) {
        Some('f')
    } else if has(&["male", "man", "boy", "masculine", "gentleman"]) {
        Some('m')
    } else {
        None
    };
    let accent = if lang.to_string().starts_with("hi") || lang.to_string().starts_with("ur") {
        'h'
    } else {
        'a'
    };
    let mut order = presets.to_vec();
    order.sort_by_key(|p| {
        let mut c = p.chars();
        let (a, g) = (c.next(), c.next());
        (gender.is_some_and(|x| g != Some(x)), a != Some(accent))
    });
    order
}

async fn hindi(context: &Context, key: &str, text: &str) -> Result<String, MediaError> {
    let result:Pronunciation=context.ask(key,context.guide.manifest().roles.transcreate,format!("Transliterate Urdu into Devanagari for Hindi speech, preserving the spoken Urdu words, vowels and order; do not translate or paraphrase. Preserve exactly one whitespace-separated output token per input token, including joined compounds, for original-script caption alignment. Keep Persian/Arabic loanword nukta sounds क़ ख़ ग़ ज़ फ़. Return only text. Input: {text}"),vec![]).await?;
    validate_hindi(text, &result.text)?;
    Ok(result.text)
}
fn validate_hindi(source: &str, target: &str) -> Result<(), MediaError> {
    if !target
        .chars()
        .any(|c| ('\u{0900}'..='\u{097f}').contains(&c))
        || target
            .chars()
            .any(|c| ('\u{0600}'..='\u{06ff}').contains(&c))
    {
        return Err(MediaError::Quality(
            "Urdu speech transliteration must use Devanagari".into(),
        ));
    }
    for (a, b) in [('ق', "क़"), ('خ', "ख़"), ('غ', "ग़"), ('ز', "ज़"), ('ف', "फ़")]
    {
        if source.contains(a) && !target.contains(b) {
            return Err(MediaError::Quality(format!(
                "Urdu loanword transliteration lost {b}; correct the pronunciation lexicon"
            )));
        }
    }
    Ok(())
}
async fn transcript(
    context: &Context,
    key: &str,
    audio: &Asset,
    lang: &LanguageTag,
    path: &Path,
) -> Result<Transcript, MediaError> {
    let routes = context
        .media
        .registry
        .config
        .routing
        .get(&context.guide.manifest().roles.transcribe)
        .ok_or_else(|| {
            MediaError::Unavailable("configure a local Whisper transcription route".into())
        })?;
    let mut last = None;
    for r in routes {
        match context
            .media
            .transcribe(key, &r.provider, &r.selector, audio, lang, path)
            .await
        {
            Ok(t) => return Ok(t),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| MediaError::Unavailable("no transcription route".into())))
}
/// The `IndicF5` reference cut of a sample and its transcript, cached by content.
async fn conditioning(
    context: &Context,
    path: &Path,
    language: &str,
    parent: &Path,
) -> Result<(PathBuf, String), MediaError> {
    let wave = Wave::decode(&std::fs::read(path).map_err(setup)?)
        .map_err(setup)?
        .conditioning()
        .encode()
        .map_err(setup)?;
    let digest = blake3::hash(&wave).to_hex().to_string();
    std::fs::create_dir_all(parent).map_err(setup)?;
    let clip = parent.join(format!("reference-{}.wav", &digest[..16]));
    if !clip.is_file() {
        crate::fsx::write_atomic(&clip, &wave)?;
    }
    let audio = Asset {
        path: clip.clone(),
        content_hash: digest.clone(),
        provider: "voice-sample".into(),
        model: "wav".into(),
        request_id: String::new(),
        quote: retold_providers::media::Quote {
            ceiling: Usd::ZERO,
            provider_tag: None,
        },
        usage: retold_providers::media::MediaUsage {
            generation: None,
            cost: Some(Usd::ZERO),
        },
    };
    let text = transcript(
        context,
        &format!("reference-transcript-{}", &digest[..16]),
        &audio,
        &language.parse().map_err(setup)?,
        &parent.join(format!("reference-{}.transcript.json", &digest[..16])),
    )
    .await?
    .text;
    if text.trim().is_empty() {
        return Err(MediaError::Quality(format!(
            "voice sample {} has no intelligible speech to condition IndicF5",
            path.display()
        )));
    }
    Ok((clip, text))
}
async fn normalize(context: &Context, audio: &Asset) -> Result<Wave, MediaError> {
    let dir = tempfile::tempdir().map_err(setup)?;
    let path = dir.path().join("mono.wav");
    let ffmpeg = std::env::var_os("RETOLD_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let args = vec![
        "-y".into(),
        "-v".into(),
        "error".into(),
        "-i".into(),
        audio.path.display().to_string(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        "24000".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        path.display().to_string(),
    ];
    let r = retold_providers::process::run(
        Path::new(&ffmpeg),
        &args,
        "",
        dir.path(),
        &retold_providers::process::Limits::default(),
        &context.media.cancel,
    )
    .await?;
    if r.status != Some(0) {
        return Err(MediaError::Quality(format!(
            "voice decode failed: {}",
            r.stderr
        )));
    }
    Wave::decode(&std::fs::read(path).map_err(setup)?).map_err(setup)
}
/// Content-addressed speaker measurement, shared by clone QA and acceptance.
pub async fn embedding(
    context: &Context,
    provider: &str,
    path: &Path,
) -> Result<Vec<f64>, MediaError> {
    let slot = context
        .media
        .registry
        .slots
        .get(provider)
        .ok_or_else(|| MediaError::Unavailable("voice provider missing".into()))?;
    let key = hash(
        &serde_json::json!({"version":1,"provider":provider,"path":path,"hash":retold_infer::models::file_sha256(path).map_err(setup)?}),
    )?;
    let step = format!("speaker-embedding-{key}");
    let cached = context
        .media
        .store
        .lock()
        .await
        .cached::<Vec<f64>>(&step, &key)
        .map_err(setup)?;
    if let Some(v) = cached {
        return Ok(v);
    }
    // Share the provider's concurrency limit with synthesis so checkpoint
    // switching cannot race a speaker measurement and spawn two full models.
    let _permit = tokio::select! {
        permit = std::sync::Arc::clone(&slot.permits).acquire_owned() => permit.map_err(setup)?,
        () = context.media.cancel.cancelled() => return Err(MediaError::Provider(retold_providers::ProviderError::Cancelled)),
    };
    let v = slot
        .provider
        .speaker_embedding(path, &context.media.cancel)
        .await?;
    context
        .media
        .store
        .lock()
        .await
        .commit(&step, &key, &v, None)
        .map_err(setup)?;
    Ok(v)
}
/// Fully cached per-turn graph. Strict quality failure never activates a partial scene.
#[expect(
    clippy::too_many_lines,
    reason = "split pending (Phase 0 step 3c); counted by the clean-code baseline"
)]
pub async fn narrate(
    context: &Context,
    lang: &LanguageTag,
    units: &[LocalUnit],
    speakers: &[String],
) -> Result<Scene, MediaError> {
    let parsed = parse(units, speakers)?;
    let mut roster = BTreeMap::new();
    let mut characters: Vec<_> = parsed
        .iter()
        .map(|(_, t)| t.speaker.clone())
        .filter(|s| s != "NARRATOR")
        .collect();
    characters.sort();
    characters.dedup();
    for speaker in parsed.iter().map(|(_, t)| &t.speaker) {
        if !roster.contains_key(speaker) {
            let index = characters.iter().position(|s| s == speaker).unwrap_or(0);
            roster.insert(speaker.clone(), cast(context, lang, speaker, index).await?);
        }
    }
    let root = context.layout.stage(Stage::Voice).join(lang.to_string());
    let revision = context.media.store.lock().await.revision("voices");
    let parent = root.join(format!("turns-r{revision}"));
    let mut turns = Vec::new();
    let room_tone = if let Some(relative) = &context.channel.file().room_tone {
        let path = context.channel.library_path(relative);
        let digest = blake3::hash(&std::fs::read(&path).map_err(setup)?)
            .to_hex()
            .to_string();
        Some(Asset {
            path,
            content_hash: digest,
            provider: "imported-room-tone".into(),
            model: "wav".into(),
            request_id: String::new(),
            quote: retold_providers::media::Quote {
                ceiling: Usd::ZERO,
                provider_tag: None,
            },
            usage: retold_providers::media::MediaUsage {
                generation: None,
                cost: Some(Usd::ZERO),
            },
        })
    } else {
        None
    };
    let ambience = if let Some(a) = &room_tone {
        let w = normalize(context, a).await?;
        if w.samples.len() < 4320 {
            return Err(MediaError::Quality(
                "room_tone must be at least 0.18 seconds".into(),
            ));
        }
        w.samples[..4320].to_vec()
    } else {
        vec![0.0; 4320]
    };
    let mut pcm = Vec::new();
    let mut aligned: BTreeMap<usize, TimedUnit> = BTreeMap::new();
    let mut offset = 0.0;
    for (index, (unit, turn)) in parsed.iter().enumerate() {
        let casting = &roster[&turn.speaker];
        let is_local = local(context, &casting.voice.provider);
        let mut directed = turn.text.clone();
        if let Some(map) = context.channel.file().pronunciations.get(lang) {
            for (from, to) in map {
                let re =
                    regex::Regex::new(&format!(r"\b{}\b", regex::escape(from))).map_err(setup)?;
                directed = re.replace_all(&directed, to.as_str()).into_owned();
            }
        }
        let already_devanagari = directed
            .chars()
            .any(|c| ('\u{0900}'..='\u{097f}').contains(&c))
            && !directed
                .chars()
                .any(|c| ('\u{0600}'..='\u{06ff}').contains(&c));
        let speech_text = match (lang.language() == "ur" && is_local, already_devanagari) {
            (false, _) => directed,
            (true, true) => {
                validate_hindi(&turn.text, &directed)?;
                directed
            }
            (true, false) => {
                hindi(
                    context,
                    &format!("speech-translit-{lang}-{index}"),
                    &directed,
                )
                .await?
            }
        };
        let step = format!("voices-{index}-{lang}");
        let revision = context.media.store.lock().await.revision(&step);
        let asr_routes = context
            .media
            .registry
            .config
            .routing
            .get(&context.guide.manifest().roles.transcribe);
        let asr_models: Vec<_> = asr_routes
            .into_iter()
            .flatten()
            .filter_map(|route| context.media.local_transcription_identity(&route.provider))
            .collect();
        let key = hash(
            &serde_json::json!({"version":8,"revision":revision,"turn":turn,"speech":speech_text,"cast":casting,"unit":unit,"offset":offset,"asr_routes":asr_routes,"asr_models":asr_models}),
        )?;
        let cached = context
            .media
            .store
            .lock()
            .await
            .cached::<Turn>(&step, &key)
            .map_err(setup)?
            .filter(|t| t.audio.verify().is_ok());
        if let Some(saved) = cached {
            saved.audio.verify()?;
            let wav = normalize(context, &saved.audio).await?;
            let timing = saved.timing.clone();
            pcm.extend(wav.samples);
            pcm.extend_from_slice(&ambience);
            offset = saved.end + 0.18;
            merge(&mut aligned, *unit, timing);
            turns.push(saved);
            continue;
        }
        context
            .media
            .store
            .lock()
            .await
            .begin(&step)
            .map_err(setup)?;
        let mut selected = None;
        let mut last_quality = String::from("no available synthesis route");
        let mut skipped: Vec<String> = Vec::new();
        let mut choices = vec![(
            casting.voice.provider.clone(),
            casting.voice.model.clone(),
            casting.voice.voice.clone(),
            casting.sample.clone(),
            casting.sample_hash.clone(),
        )];
        if is_local && casting.voice.model == "indicf5" && casting.sample.is_some() {
            // Same cloned voice on the dedicated Hindi Chatterbox pack; the
            // route is skipped when that pack is not installed.
            choices.push((
                casting.voice.provider.clone(),
                "chatterbox-hi".into(),
                casting.voice.voice.clone(),
                casting.sample.clone(),
                casting.sample_hash.clone(),
            ));
        }
        if is_local && CLONING.contains(&casting.voice.model.as_str()) && casting.sample.is_some() {
            choices.push((
                casting.voice.provider.clone(),
                "nearest-preset".into(),
                String::new(),
                None,
                None,
            ));
        }
        choices.extend(casting.voice.fallbacks.iter().map(|f| {
            (
                f.provider.clone(),
                f.model.clone(),
                f.voice.clone(),
                None,
                None,
            )
        }));
        let speech_language: LanguageTag = if lang.language() == "ur" && is_local {
            "hi".parse().map_err(setup)?
        } else {
            lang.clone()
        };
        for (choice, (provider, mut model, mut voice, mut sample, mut sample_hash)) in
            choices.into_iter().enumerate()
        {
            if model == "nearest-preset" {
                voice = match closest(context, casting, &speech_language).await {
                    Ok(nearest) => nearest.0,
                    Err(
                        MediaError::Unavailable(_)
                        | MediaError::Quality(_)
                        | MediaError::Provider(
                            retold_providers::ProviderError::Unavailable(_)
                            | retold_providers::ProviderError::Unsupported(_),
                        ),
                    ) => continue,
                    Err(error) => return Err(error),
                };
                model = "kokoro".into();
                sample = None;
                sample_hash = None;
            }
            let provider = &provider;
            let model = &model;
            let mut spoken = if local(context, provider) {
                speech_text.clone()
            } else {
                turn.text.clone()
            };
            let synthesis_lang = if local(context, provider) {
                speech_language.clone()
            } else {
                lang.clone()
            };
            for attempt in 0..2 {
                let mut controls = Controls::emotion(&turn.emotion);
                controls.temperature = if attempt == 0 { 0.8 } else { 0.6 };
                controls.sample = sample.clone();
                controls.sample_hash = sample_hash.clone();
                let reference_language = if casting.sample_language.language() == "ur" {
                    "hi"
                } else {
                    casting.sample_language.language()
                };
                if model == "indicf5"
                    && let Some(path) = &sample
                {
                    controls.reference =
                        Some(conditioning(context, path, reference_language, &parent).await?);
                }
                if sample.is_some() && reference_language != synthesis_lang.language() {
                    controls.cfg_weight = 0.0;
                }
                controls.seed = u32::try_from(index)
                    .unwrap_or(u32::MAX)
                    .wrapping_add(42 + attempt);
                let req = SpeechRequest {
                    text: spoken.clone(),
                    voice: voice.clone(),
                    instructions: if local(context, provider) {
                        Some(serde_json::to_string(&controls).map_err(setup)?)
                    } else {
                        casting.voice.instructions.clone()
                    },
                    language: synthesis_lang.clone(),
                };
                let audio = match context
                    .media
                    .speech(
                        &format!("{step}-r{revision}-c{choice}-a{attempt}"),
                        provider,
                        model,
                        &req,
                        &parent.join(format!("{index}-c{choice}-a{attempt}.wav")),
                    )
                    .await
                {
                    Ok(a) => a,
                    Err(MediaError::Provider(retold_providers::ProviderError::Cancelled)) => {
                        return Err(MediaError::Provider(
                            retold_providers::ProviderError::Cancelled,
                        ));
                    }
                    Err(MediaError::Provider(
                        retold_providers::ProviderError::Unavailable(reason)
                        | retold_providers::ProviderError::Unsupported(reason),
                    )) if local(context, provider) => {
                        // Keep why a route was skipped; otherwise a broken install
                        // looks like a quality failure of the next fallback.
                        skipped.push(format!("{model}: {reason}"));
                        break;
                    }
                    Err(error) => return Err(error),
                };
                // Urdu is synthesised from Devanagari but heard as Urdu: Whisper
                // spells Urdu words (شخص, متفق) reliably only in Urdu script.
                let urdu = lang.language() == "ur";
                let transcribed = transcript(
                    context,
                    &format!("transcript-{step}-r{revision}-c{choice}-a{attempt}"),
                    &audio,
                    if urdu { lang } else { &synthesis_lang },
                    &parent.join(format!("{index}-c{choice}-a{attempt}.transcript.json")),
                )
                .await?;
                let expected = if local(context, provider) && !urdu {
                    speech_text.as_str()
                } else {
                    turn.text.as_str()
                };
                let score = accuracy(expected, &transcribed.text);
                let similarity = if let Some(sample) = &sample {
                    let a = embedding(context, provider, sample).await?;
                    let b = embedding(context, provider, &audio.path).await?;
                    Some(cosine(&a, &b).map_err(setup)?)
                } else {
                    None
                };
                last_quality = format!(
                    "accuracy={score:.3}, clone_similarity={similarity:?}, provider={provider}, model={model}"
                );
                if score >= 0.95 && similarity.is_none_or(|s| s >= 0.70) {
                    selected = Some((audio, transcribed, score, similarity, choice > 0, spoken));
                    break;
                }
                if attempt == 0 {
                    let repair:Pronunciation=context.ask(&format!("repair-{step}-r{revision}-c{choice}"),context.guide.manifest().roles.transcreate,format!("Respell the following for clearer {} speech. Preserve all words, meaning and order. Change only mispronounced loanwords or names; keep Devanagari nukta. Expected: {speech_text}. Heard: {}. Return text only.",synthesis_lang,transcribed.text),vec![]).await?;
                    spoken = repair.text;
                    if lang.language() == "ur" && local(context, provider) {
                        validate_hindi(&turn.text, &spoken)?;
                    }
                }
            }
            if selected.is_some() {
                break;
            }
        }
        let spend = context.media.ledger.total()?;
        let(audio,heard,score,similarity,fallback,spoken)=selected.ok_or_else(||MediaError::Quality(format!("{lang} turn {} failed 95% pronunciation / 0.70 clone similarity after one retry and configured fallbacks ({last_quality}{}); spent {spend}; evidence: {}; fix channel pronunciations or voice sample, then retold redo <run> voices {}",index+1,if skipped.is_empty() { String::new() } else { format!("; skipped routes: {}", skipped.join("; ")) },parent.display(),index+1)))?;
        let wav = normalize(context, &audio).await?;
        let end = offset + wav.duration();
        let words: Vec<_> = heard
            .words
            .iter()
            .map(|w| SpokenWord {
                word: w.word.clone(),
                start: w.start,
                end: w.end,
            })
            .collect();
        // Align in the script the transcript was heard in (Urdu is heard as Urdu).
        let heard_script = if lang.language() == "ur" {
            turn.text.clone()
        } else {
            spoken.clone()
        };
        let a =
            retold_lang::align::align(&[(unit.to_string(), heard_script)], &words, wav.duration())
                .map_err(setup)?;
        let mut timing = a
            .units
            .into_iter()
            .next()
            .ok_or_else(|| MediaError::Quality("empty turn alignment".into()))?;
        {
            let source: Vec<_> = turn.text.split_whitespace().collect();
            if source.len() != timing.words.len() {
                return Err(MediaError::Quality("pronunciation spelling or transliteration changed token count; cannot preserve accurate original-script captions".into()));
            }
            for (w, text) in timing.words.iter_mut().zip(source) {
                w.text = text.into();
            }
        }
        for w in &mut timing.words {
            w.start += offset;
            w.end += offset;
        }
        timing.start += offset;
        timing.end += offset;
        fsx::write_json_atomic(&parent.join(format!("{index}.alignment.json")), &timing)
            .map_err(setup)?;
        merge(&mut aligned, *unit, timing.clone());
        let result = Turn {
            unit: *unit,
            speaker: turn.speaker.clone(),
            emotion: turn.emotion.clone(),
            text: turn.text.clone(),
            speech_text: spoken,
            start: offset,
            end,
            track: if index % 2 == 0 { "A1" } else { "A2" }.into(),
            audio,
            timing,
            accuracy: score,
            speaker_similarity: similarity,
            fallback,
        };
        pcm.extend(wav.samples);
        pcm.extend_from_slice(&ambience);
        offset = end + 0.18;
        context
            .media
            .store
            .lock()
            .await
            .commit(&step, &key, &result, None)
            .map_err(setup)?;
        turns.push(result);
    }
    let wav = Wave {
        rate: 24000,
        samples: pcm,
    };
    let duration = wav.duration();
    let [lo, hi] = context.guide.manifest().format.target_seconds;
    let total = duration + context.guide.manifest().edit.ending.seconds;
    if !(lo..=hi).contains(&total) {
        return Err(MediaError::Quality(format!(
            "dialogue duration {total:.2}s is outside guide {lo}–{hi}; shorten/extend script before edit"
        )));
    }
    let bytes = wav.encode().map_err(setup)?;
    let digest = blake3::hash(&bytes).to_hex().to_string();
    let path = root.join(format!("dialogue-{digest}.wav"));
    fsx::write_atomic(&path, &bytes).map_err(setup)?;
    let audio = Asset {
        path,
        content_hash: digest,
        provider: "local-dialogue-mix".into(),
        model: "pcm24k".into(),
        request_id: String::new(),
        quote: retold_providers::media::Quote {
            ceiling: Usd::ZERO,
            provider_tag: None,
        },
        usage: retold_providers::media::MediaUsage {
            generation: None,
            cost: Some(Usd::ZERO),
        },
    };
    let matched = turns.iter().map(|t| t.accuracy).fold(1.0, f64::min);
    let result = Scene {
        turns,
        room_tone,
        audio,
        alignment: Alignment {
            duration,
            units: aligned.into_values().collect(),
            matched,
        },
    };
    fsx::write_json_atomic(&file(context, lang), &result).map_err(setup)?;
    let key = hash(&result)?;
    context
        .media
        .store
        .lock()
        .await
        .commit("voices", &key, &result, None)
        .map_err(setup)?;
    Ok(result)
}
fn merge(units: &mut BTreeMap<usize, TimedUnit>, id: usize, timing: TimedUnit) {
    if let Some(previous) = units.get_mut(&id) {
        previous.end = timing.end;
        previous.words.extend(timing.words);
    } else {
        units.insert(id, timing);
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, MediaError> {
    serde_json::from_slice(&std::fs::read(path).map_err(setup)?).map_err(setup)
}

/// Choose the preset nearest to the channel sample using actual model speaker embeddings.
async fn closest(
    context: &Context,
    casting: &Casting,
    language: &LanguageTag,
) -> Result<(String, PathBuf, String), MediaError> {
    let sample = casting
        .sample
        .as_ref()
        .ok_or_else(|| MediaError::Quality("clone sample missing".into()))?;
    let target = embedding(context, &casting.voice.provider, sample).await?;
    let folder = context.layout.cast_dir().join("voice-fallback");
    let mut best = None;
    let presets = if language.language() == "hi" {
        ["hm_omega", "hf_alpha"]
    } else {
        ["am_adam", "af_heart"]
    };
    for preset in presets {
        let path = folder.join(format!("{preset}.wav"));
        let asset = if path.exists() {
            let bytes = std::fs::read(&path).map_err(setup)?;
            let digest = blake3::hash(&bytes).to_hex().to_string();
            Asset {
                path,
                content_hash: digest,
                provider: casting.voice.provider.clone(),
                model: "kokoro".into(),
                request_id: String::new(),
                usage: retold_providers::media::MediaUsage {
                    generation: None,
                    cost: Some(Usd::ZERO),
                },
                quote: retold_providers::media::Quote {
                    ceiling: Usd::ZERO,
                    provider_tag: None,
                },
            }
        } else {
            let request = SpeechRequest {
                text: if preset.starts_with('h') {
                    "यह एक साफ़ आवाज़ का नमूना है। हम आज साथ बैठकर अपनी कहानी सुनाते हैं और हर बात धीरे धीरे समझते हैं। हर व्यक्ति की आवाज़ अलग है और हम ध्यान से हर शब्द को सुनते हैं।".into()
                } else {
                    "This is a clear reference recording for a fictional character. We speak calmly together, listen carefully, and describe what happens in the story today. Every person has a different voice, and each voice is easy to recognize.".into()
                },
                voice: preset.into(),
                instructions: None,
                language: if preset.starts_with('h') { "hi" } else { "en" }
                    .parse()
                    .map_err(setup)?,
            };
            context
                .media
                .speech(
                    &format!("cast-fallback-{}", preset.replace('_', "-")),
                    &casting.voice.provider,
                    "kokoro",
                    &request,
                    &path,
                )
                .await?
        };
        asset.verify()?;
        Wave::decode(&std::fs::read(&asset.path).map_err(setup)?)
            .and_then(|w| w.reference())
            .map_err(setup)?;
        let similarity = cosine(
            &target,
            &embedding(context, &casting.voice.provider, &asset.path).await?,
        )
        .map_err(setup)?;
        if best
            .as_ref()
            .is_none_or(|b: &(f64, String, PathBuf, String)| similarity > b.0)
        {
            best = Some((
                similarity,
                preset.into(),
                asset.path.clone(),
                retold_infer::models::file_sha256(&asset.path).map_err(setup)?,
            ));
        }
    }
    let (_, name, path, digest) =
        best.ok_or_else(|| MediaError::Quality("no valid fallback preset".into()))?;
    Ok((name, path, digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casting_prefers_the_characters_gender_then_the_runs_accent() {
        let presets = ["am_adam", "af_heart", "hm_omega", "hf_alpha"];
        let en: LanguageTag = "en".parse().unwrap();
        let hi: LanguageTag = "hi".parse().unwrap();
        assert_eq!(
            preset_order(&presets, "female, late twenties", &en)[..2],
            ["af_heart", "hf_alpha"]
        );
        assert_eq!(
            preset_order(&presets, "Male, elderly, dry", &hi)[..2],
            ["hm_omega", "am_adam"]
        );
        assert_eq!(
            preset_order(&presets, "", &en)[..2],
            ["am_adam", "af_heart"]
        );
    }
}
