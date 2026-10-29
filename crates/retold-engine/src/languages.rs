//! Per-unit transcreation, continuous voice, alignment, the three caption modes, sidecars
//! and posting text. Editions with the same narration share one paid audio request.

use retold_core::num;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use retold_core::{CaptionKind, CaptionMode, LanguageTag, Variant};
use retold_lang::{Alignment, Cue, SpokenWord, TimedWord, captions, subtitles};
use retold_providers::media::SpeechRequest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::task::JoinSet;

use crate::assets::Storyboard;
use crate::context::Context;
use crate::media_runtime::{Asset, MediaError};
use crate::pipeline::Until;
use crate::store::hash;
use crate::story::{FactCheck, Script, SectionText};
use crate::{EngineError, Stage, fsx};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VoiceBackend {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalUnit {
    pub id: usize,
    pub section: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalScript {
    pub units: Vec<LocalUnit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordMapping {
    pub words: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PhraseTranslation {
    pub phrases: Vec<Phrase>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Phrase {
    pub id: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionBlock {
    pub text: String,
    pub start: f64,
    pub end: f64,
    /// Unicode scalar ranges, never byte offsets.
    pub highlight: Option<[usize; 2]>,
    pub pop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Post {
    pub platform: String,
    pub title: String,
    pub description: String,
    pub hashtags: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Packaging {
    pub posts: Vec<Post>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edition {
    pub variant: Variant,
    pub units: Vec<LocalUnit>,
    pub audio: Asset,
    pub alignment: Alignment,
    pub captions: Vec<CaptionBlock>,
    pub sidecars: Vec<PathBuf>,
    pub packaging: Packaging,
}

#[must_use]
pub fn edition_id(variant: &Variant) -> String {
    variant
        .burned()
        .filter(|t| t.lang != variant.narration)
        .map_or_else(
            || variant.narration.to_string(),
            |t| format!("{}-{}", variant.narration, t.lang),
        )
}

pub async fn languages(
    context: Arc<Context>,
    board: Arc<Storyboard>,
    until: Until,
) -> Result<Vec<Edition>, MediaError> {
    let mut grouped: BTreeMap<LanguageTag, Vec<Variant>> = BTreeMap::new();
    for variant in &context.meta.variants {
        grouped
            .entry(variant.narration.clone())
            .or_default()
            .push(variant.clone());
    }
    let mut tasks = JoinSet::new();
    for (lang, variants) in grouped {
        let context = Arc::clone(&context);
        let board = Arc::clone(&board);
        tasks.spawn(async move {
            let result = crate::events::observe_branch(
                one_language(&context, &board, &lang, &variants, until),
                async || {
                    context
                        .media
                        .store
                        .lock()
                        .await
                        .branch_failed("languages", Some(&lang.to_string()))
                },
            )
            .await;
            fsx::write_json_atomic(
                &context
                    .layout
                    .stage(Stage::Script)
                    .join(lang.to_string())
                    .join("result.json"),
                &json!({"language":lang,"error":result.as_ref().err().map(ToString::to_string)}),
            )?;
            result
        });
    }
    let mut editions = Vec::new();
    let mut failure = None;
    while let Some(result) = tasks.join_next().await {
        match result
            .map_err(|e| MediaError::from(EngineError::Setup(format!("language task failed: {e}"))))
        {
            Ok(Ok(items)) => editions.extend(items),
            Ok(Err(error)) | Err(error) => {
                if failure.is_none() {
                    failure = Some(error);
                }
            }
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    editions.sort_by_key(|e| edition_id(&e.variant));
    fsx::write_json_atomic(
        &context.layout.stage(Stage::Captions).join("editions.json"),
        &editions,
    )?;
    context
        .media
        .store
        .lock()
        .await
        .commit("captions", &hash(&editions)?, &editions, None)?;
    Ok(editions)
}

async fn one_language(
    context: &Context,
    board: &Storyboard,
    lang: &LanguageTag,
    variants: &[Variant],
    until: Until,
) -> Result<Vec<Edition>, MediaError> {
    let with_captions = until >= Until::Captions;
    let units = transcreate(context, board, lang).await?;
    let units = crate::editing::dialogue::effective(&context.layout, &lang.to_string(), units)
        .map_err(|error| MediaError::Quality(error.to_string()))?;
    let (audio, alignment) = if context.guide.manifest().narration.mode
        == retold_guide::manifest::NarrationMode::Dialogue
        || crate::voices::local_narrator(context, lang)
    {
        let spoken_units = if context.guide.manifest().narration.mode
            == retold_guide::manifest::NarrationMode::Dialogue
        {
            units.clone()
        } else {
            units
                .iter()
                .map(|u| LocalUnit {
                    id: u.id,
                    section: u.section.clone(),
                    text: format!("NARRATOR: {}", u.text),
                })
                .collect()
        };
        let scene =
            crate::voices::narrate(context, lang, &spoken_units, &board.plan.characters).await?;
        (scene.audio, scene.alignment)
    } else {
        narrate(context, lang, &units).await?
    };
    let packaging = if with_captions {
        package(context, lang, &units).await?
    } else {
        Packaging { posts: vec![] }
    };
    let mut editions = Vec::new();
    for variant in variants {
        let mut burned = Vec::new();
        let mut sidecars = Vec::new();
        for track in variant.captions.iter().filter(|_| with_captions) {
            let (blocks, cues) = caption_track(context, variant, track, &alignment).await?;
            match track.mode {
                CaptionMode::Burned => {
                    burned = blocks;
                    for (extension, text) in [
                        ("srt", subtitles::to_srt(&cues)),
                        ("vtt", subtitles::to_vtt(&cues)),
                    ] {
                        let path = context.layout.subtitle_file(lang, &track.lang, extension);
                        fsx::write_atomic(&path, text.as_bytes())?;
                        sidecars.push(path);
                    }
                }
                CaptionMode::Sidecar(format) => {
                    let path = context
                        .layout
                        .subtitle_file(lang, &track.lang, format.extension());
                    let text = if format.extension() == "srt" {
                        subtitles::to_srt(&cues)
                    } else {
                        subtitles::to_vtt(&cues)
                    };
                    fsx::write_atomic(&path, text.as_bytes())?;
                    sidecars.push(path);
                }
            }
        }
        editions.push(Edition {
            variant: variant.clone(),
            units: units.clone(),
            audio: audio.clone(),
            alignment: alignment.clone(),
            captions: burned,
            sidecars,
            packaging: packaging.clone(),
        });
    }
    Ok(editions)
}

async fn narrate(
    context: &Context,
    lang: &LanguageTag,
    units: &[LocalUnit],
) -> Result<(Asset, Alignment), MediaError> {
    let voice = context
        .channel
        .voice(lang)
        .map_err(MediaError::Unavailable)?;
    let audio = synthesize_narration(context, lang, units, voice).await?;
    let mut transcript = transcribe(context, lang, &audio, units).await?;
    let fitted = crate::audio_fit::fit(context, audio).await?;
    let audio = fitted.asset;
    if (fitted.time_scale - 1.0).abs() > f64::EPSILON {
        for word in &mut transcript.words {
            word.start *= fitted.time_scale;
            word.end *= fitted.time_scale;
        }
        transcript.duration = fitted.duration;
        transcript.validate()?;
        fsx::write_json_atomic(&audio.path.with_extension("transcript.json"), &transcript)?;
    }
    let words: Vec<SpokenWord> = transcript
        .words
        .iter()
        .map(|w| SpokenWord {
            word: w.word.clone(),
            start: w.start,
            end: w.end,
        })
        .collect();
    let narration: Vec<_> = units
        .iter()
        .map(|u| (u.id.to_string(), u.text.clone()))
        .collect();
    let alignment = retold_lang::align::align(&narration, &words, transcript.duration)
        .map_err(|e| MediaError::Quality(e.to_string()))?;
    if alignment.matched < 0.70 {
        return Err(MediaError::Quality(format!(
            "{lang} transcript matched only {:.0}% of words; check the voice before editing",
            alignment.matched * 100.0
        )));
    }
    let [lo, hi] = context.guide.manifest().format.target_seconds;
    let hold = context.guide.manifest().edit.ending.seconds;
    if !duration_fits(alignment.duration, hold, [lo, hi]) {
        return Err(MediaError::Quality(format!(
            "{lang} narration plus ending hold is {:.1}s, outside guide range {lo}–{hi}s",
            alignment.duration + hold
        )));
    }
    fsx::write_json_atomic(
        &context
            .layout
            .stage(Stage::Voice)
            .join(lang.to_string())
            .join("alignment.json"),
        &alignment,
    )?;
    Ok((audio, alignment))
}

fn duration_fits(narration: f64, hold: f64, range: [f64; 2]) -> bool {
    narration.is_finite()
        && narration > 0.0
        && hold.is_finite()
        && hold >= 0.0
        && (range[0]..=range[1]).contains(&(narration + hold))
}

async fn transcribe(
    context: &Context,
    lang: &LanguageTag,
    audio: &Asset,
    units: &[LocalUnit],
) -> Result<retold_providers::media::Transcript, MediaError> {
    let routes = context
        .media
        .registry
        .config
        .routing
        .get(&context.guide.manifest().roles.transcribe)
        .ok_or_else(|| MediaError::Unavailable("no transcription route configured".into()))?;
    let mut best: Option<(f64, retold_providers::media::Transcript, String, String)> = None;
    let mut candidates = Vec::new();
    for route in routes {
        match context
            .media
            .cached_transcript(&format!("transcript-{lang}"), &route.selector, audio, lang)
            .await
        {
            Ok(Some(transcript)) => {
                let score = transcript_match(units, &transcript)?;
                candidates.push(
                    json!({"provider":route.provider,"model":route.selector,"matched":score}),
                );
                if best.as_ref().is_none_or(|(known, _, _, _)| score > *known) {
                    best = Some((
                        score,
                        transcript,
                        route.provider.clone(),
                        route.selector.clone(),
                    ));
                }
            }
            Ok(None) | Err(MediaError::Provider(_)) => {}
            Err(error) => return Err(error),
        }
    }
    if let Some((score, transcript, provider, model)) = best {
        fsx::write_json_atomic(
            &context
                .layout
                .stage(Stage::Voice)
                .join(lang.to_string())
                .join("transcript-selection.json"),
            &json!({"audio_hash":audio.content_hash,"candidates":candidates,"selected":{"provider":provider,"model":model,"matched":score},"method":"highest direct word match among valid committed transcripts; no generation"}),
        )?;
        return Ok(transcript);
    }
    let mut failures = Vec::new();
    for route in routes {
        let result = context
            .media
            .transcribe(
                &format!("transcript-{lang}"),
                &route.provider,
                &route.selector,
                audio,
                lang,
                &context
                    .layout
                    .stage(Stage::Voice)
                    .join(lang.to_string())
                    .join("transcript.json"),
            )
            .await;
        match result {
            Ok(transcript) => return Ok(transcript),
            Err(error @ (MediaError::Ledger(_) | MediaError::Engine(_))) => return Err(error),
            Err(error) => {
                context.check_cancel()?;
                failures.push(error.to_string());
            }
        }
    }
    Err(MediaError::Unavailable(failures.join("; ")))
}

fn transcript_match(
    units: &[LocalUnit],
    transcript: &retold_providers::media::Transcript,
) -> Result<f64, MediaError> {
    let narration = units
        .iter()
        .map(|u| (u.id.to_string(), u.text.clone()))
        .collect::<Vec<_>>();
    let spoken = transcript
        .words
        .iter()
        .map(|w| SpokenWord {
            word: w.word.clone(),
            start: w.start,
            end: w.end,
        })
        .collect::<Vec<_>>();
    Ok(
        retold_lang::align::align(&narration, &spoken, transcript.duration)
            .map_err(|e| MediaError::Quality(e.to_string()))?
            .matched,
    )
}

async fn transcreate(
    context: &Context,
    board: &Storyboard,
    lang: &LanguageTag,
) -> Result<Vec<LocalUnit>, MediaError> {
    let master = LocalScript {
        units: board
            .units
            .iter()
            .map(|u| LocalUnit {
                id: u.id,
                section: u.section.clone(),
                text: u.narration.clone(),
            })
            .collect(),
    };
    if lang == &context.guide.manifest().format.source_language {
        return Ok(master.units);
    }
    let prompt = transcreation_prompt(context, &master, lang)?;
    let mut local: LocalScript = context
        .ask(
            &format!("transcreate-{lang}"),
            context.guide.manifest().roles.transcreate,
            prompt.clone(),
            vec![],
        )
        .await?;
    for pass in 0..=context.guide.manifest().quality.max_fix_passes {
        let mut problems = local_gates(&local, &master, lang, context);
        let facts: FactCheck = context.ask(&format!("transcreate-check-{lang}-{pass}"),context.guide.manifest().roles.check,
            format!("Check factual fidelity against the MASTER, not outside knowledge. Report invented/contradictory events, motives, relationships or changed meaning. Also check idioms and cultural references: flag literal translations that lose the intended meaning, unnatural target-language idioms, and unexplained references that make the intended meaning inaccessible. Preserve native names and factual references; do not replace them with invented local facts. Apply the target-language guide notes: {}. Target language: {}. MASTER: {}\nLOCAL: {}. Evidence must quote the master or be empty when unsupported.",context.guide.voice_for(lang),lang.display_name(),serde_json::to_string(&master).map_err(|e|EngineError::Setup(e.to_string()))?,serde_json::to_string(&local).map_err(|e|EngineError::Setup(e.to_string()))?),vec![]).await?;
        problems.extend(
            facts
                .issues
                .iter()
                .map(|i| format!("{}: {}", i.claim, i.problem)),
        );
        fsx::write_json_atomic(
            &context
                .layout
                .stage(Stage::Script)
                .join(lang.to_string())
                .join(format!("gates-{pass}.json")),
            &json!({"pass":pass,"problems":problems,"fact_check":facts}),
        )?;
        if problems.is_empty() {
            fsx::write_json_atomic(
                &context
                    .layout
                    .stage(Stage::Script)
                    .join(lang.to_string())
                    .join("units.json"),
                &local,
            )?;
            return Ok(local.units);
        }
        if pass == context.guide.manifest().quality.max_fix_passes {
            // Length is the one gate a model misses by feel. When it is the only problem left,
            // one last pass gets exact numbers instead of "too long".
            if problems.iter().all(|p| p.starts_with("length:")) {
                return match length_trim(context, &master, lang, &prompt, &local).await? {
                    Ok(units) => Ok(units),
                    Err(remaining) => Err(MediaError::Quality(format!(
                        "{} after a targeted length pass: {}",
                        lang.display_name(),
                        remaining.join("; ")
                    ))),
                };
            }
            return Err(MediaError::Quality(problems.join("; ")));
        }
        local = context
            .ask(
                &format!("transcreate-fix-{lang}-{pass}"),
                context.guide.manifest().roles.transcreate,
                format!(
                    "{prompt}\nPrevious: {}\nFix these checks with minimum changes: {}",
                    serde_json::to_string(&local).map_err(|e| EngineError::Setup(e.to_string()))?,
                    problems.join("; ")
                ),
                vec![],
            )
            .await?;
    }
    Err(MediaError::Quality("transcreation did not finish".into()))
}

/// A final transcreation pass with an exact word target, used only when length is the sole
/// remaining problem, plus one ordinary fix pass if the trim breaks another gate. The result
/// must pass every gate and a fresh fact check; otherwise the remaining problems are returned.
async fn length_trim(
    context: &Context,
    master: &LocalScript,
    lang: &LanguageTag,
    prompt: &str,
    local: &LocalScript,
) -> Result<Result<Vec<LocalUnit>, Vec<String>>, MediaError> {
    let Some([lo, hi]) = context
        .guide
        .manifest()
        .length
        .get(lang)
        .and_then(|r| r.seconds)
    else {
        return Ok(Err(vec!["no seconds rule to trim against".into()]));
    };
    let all = local
        .units
        .iter()
        .map(|u| u.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let seconds = crate::speech::seconds(&all, lang, context.guide.manifest());
    let words = retold_lang::text::word_count(&all);
    let (target, change) = length_target(words, seconds, [lo, hi]);
    let mut current: LocalScript = context
        .ask(
            &format!("transcreate-trim-{lang}"),
            context.guide.manifest().roles.transcreate,
            format!(
                "{prompt}\nPrevious: {}\nThis version has {words} words, about {seconds:.0} seconds of \
                 speech; it must be {lo}–{hi} seconds. Rewrite it to about {target} words in total \
                 ({change}) by tightening wording across many units, not by dropping units. Keep every \
                 unit id and section, every fact, the names and the guide's ending requirements.",
                serde_json::to_string(local).map_err(|e| EngineError::Setup(e.to_string()))?
            ),
            vec![],
        )
        .await?;
    let dir = context.layout.stage(Stage::Script).join(lang.to_string());
    for (attempt, suffix) in ["trim", "trim-fix"].into_iter().enumerate() {
        let mut problems = local_gates(&current, master, lang, context);
        let facts: FactCheck = context.ask(&format!("transcreate-check-{lang}-{suffix}"),context.guide.manifest().roles.check,
            format!("Check factual fidelity against the MASTER, not outside knowledge. Report invented/contradictory events, motives, relationships or changed meaning. Also check idioms and cultural references: flag literal translations that lose the intended meaning, unnatural target-language idioms, and unexplained references that make the intended meaning inaccessible. Preserve native names and factual references; do not replace them with invented local facts. Apply the target-language guide notes: {}. Target language: {}. MASTER: {}\nLOCAL: {}. Evidence must quote the master or be empty when unsupported.",context.guide.voice_for(lang),lang.display_name(),serde_json::to_string(master).map_err(|e|EngineError::Setup(e.to_string()))?,serde_json::to_string(&current).map_err(|e|EngineError::Setup(e.to_string()))?),vec![]).await?;
        problems.extend(
            facts
                .issues
                .iter()
                .map(|i| format!("{}: {}", i.claim, i.problem)),
        );
        fsx::write_json_atomic(
            &dir.join(format!("gates-{suffix}.json")),
            &json!({"pass":suffix,"problems":problems,"fact_check":facts}),
        )?;
        if problems.is_empty() {
            fsx::write_json_atomic(&dir.join("units.json"), &current)?;
            return Ok(Ok(current.units));
        }
        if attempt == 1 {
            return Ok(Err(problems));
        }
        current = context
            .ask(
                &format!("transcreate-fix-{lang}-trim"),
                context.guide.manifest().roles.transcreate,
                format!(
                    "{prompt}\nPrevious: {}\nFix these checks with minimum changes, keeping the total \
                     length about the same ({target} words): {}",
                    serde_json::to_string(&current).map_err(|e| EngineError::Setup(e.to_string()))?,
                    problems.join("; ")
                ),
                vec![],
            )
            .await?;
    }
    Ok(Err(vec!["transcreation did not finish".into()]))
}

/// Words to aim for so the estimated speech lands mid-range, and how to describe the change.
fn length_target(words: usize, seconds: f64, [lo, hi]: [f64; 2]) -> (usize, String) {
    let target = num::round_usize(num::count_f64(words) * lo.midpoint(hi) / seconds.max(1.0));
    let change = if target < words {
        format!("cut about {} words", words - target)
    } else {
        format!(
            "add about {} words of detail already in the master",
            target - words
        )
    };
    (target, change)
}

fn transcreation_prompt(
    context: &Context,
    master: &LocalScript,
    lang: &LanguageTag,
) -> Result<String, MediaError> {
    let rule = context
        .guide
        .manifest()
        .length
        .get(lang)
        .ok_or_else(|| MediaError::Quality(format!("no length rule for {lang}")))?;
    let prompt = format!(
        "Transcreate this narration into {} using its {} script, unit by unit, preserving every id and section exactly. Keep facts, order, relationships and the guide's section/ending purposes. Use the guide's voice and idiomatic language, preserving native names and number conventions; introduce no facts. Each unit retains its image. Language notes: {}. Length rule: {}. Master: {}",
        lang.display_name(),
        lang.script(),
        context.guide.voice_for(lang),
        serde_json::to_string(rule).map_err(|e| EngineError::Setup(e.to_string()))?,
        serde_json::to_string(&master).map_err(|e| EngineError::Setup(e.to_string()))?
    );
    let mut prompt = format!(
        "{prompt}\nPreferred native name spellings: {}",
        serde_json::to_string(&context.channel.file().name_spellings.get(lang))
            .map_err(|e| EngineError::Setup(e.to_string()))?
    );
    if context.guide.manifest().narration.mode == retold_guide::manifest::NarrationMode::Dialogue {
        prompt.push_str("\nPreserve every speaker cue and emotion tag exactly, one turn per line. Translate only spoken text after the colon. Never translate speaker identifiers or emotion tags.");
    }
    if let Some([lo, hi]) = rule.seconds {
        let unit_count = u32::try_from(master.units.len())
            .map_err(|_| EngineError::Setup("too many narration units".into()))?;
        let pauses = f64::from(unit_count) * 0.25;
        let rate = crate::speech::rate(context.guide.manifest(), lang);
        let target = (lo.midpoint(hi) - pauses) * rate;
        write!(prompt,"\nAim for approximately {target:.0} whitespace-separated words across all units. Speech is estimated at {rate} words per second plus punctuation pauses; seconds are not a word count. Write concise native idiom rather than a verbose literal translation. Preserve all facts, unit ids and sections. Split sentences within a unit when useful, without splitting the unit itself.").map_err(|e|EngineError::Setup(e.to_string()))?;
        let max_run = context
            .guide
            .manifest()
            .writing
            .gates
            .iter()
            .chain(
                context
                    .guide
                    .manifest()
                    .writing
                    .language_gates
                    .get(lang)
                    .into_iter()
                    .flatten(),
            )
            .filter_map(|g| match g {
                retold_guide::manifest::Gate::SentenceLengthVariety {
                    max_same_length_run,
                    ..
                } => Some(*max_same_length_run),
                _ => None,
            })
            .min();
        if let Some(max_run) = max_run {
            write!(prompt,"\nVary adjacent sentence lengths (short ≤6 words, medium 7–14, long ≥15); the configured maximum run of one length class is {max_run}. Count whitespace-separated words to check this before returning.").map_err(|e|EngineError::Setup(e.to_string()))?;
        }
    }
    Ok(prompt)
}

fn local_gates(
    local: &LocalScript,
    master: &LocalScript,
    lang: &LanguageTag,
    context: &Context,
) -> Vec<String> {
    let mut problems = Vec::new();
    if local.units.len() != master.units.len()
        || local
            .units
            .iter()
            .zip(&master.units)
            .any(|(a, b)| a.id != b.id || a.section != b.section || a.text.trim().is_empty())
    {
        problems.push(
            "one nonempty unit per master id, in the same order and section, is required".into(),
        );
    }
    let all = local
        .units
        .iter()
        .map(|u| u.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let letters = all.chars().filter(|c| c.is_alphabetic()).count();
    let native = all
        .chars()
        .filter(|c| match lang.script() {
            retold_core::WritingScript::Deva => ('\u{0900}'..='\u{097f}').contains(c),
            retold_core::WritingScript::Arab => {
                ('\u{0600}'..='\u{06ff}').contains(c) || ('\u{0750}'..='\u{077f}').contains(c)
            }
            _ => true,
        })
        .count();
    if letters == 0 || native * 10 < letters * 8 {
        problems.push(format!(
            "at least 80% of alphabetic text must use {} script",
            lang.script()
        ));
    }
    let mut sections: BTreeMap<String, String> = BTreeMap::new();
    for unit in &local.units {
        let text = sections.entry(unit.section.clone()).or_default();
        text.push_str(&unit.text);
        text.push(' ');
    }
    let script = Script {
        sections: context
            .guide
            .manifest()
            .structure
            .sections
            .iter()
            .map(|s| SectionText {
                section: s.id.to_string(),
                text: sections.remove(s.id.as_str()).unwrap_or_default(),
            })
            .collect(),
    };
    let mut manifest = context.guide.manifest().clone();
    manifest.format.source_language = lang.clone();
    // Translations are gated on what the plan asks of every language: length, the ending
    // question and banned phrases, plus a fact check. Rhythm and sentence-start rules are
    // English style heuristics; applied to a unit-by-unit transcreation they force rewrites
    // that drift from the master's facts. A guide can still add per-language gates below.
    manifest.writing.gates.retain(|g| {
        matches!(
            g,
            retold_guide::manifest::Gate::Regex { .. }
                | retold_guide::manifest::Gate::BannedPhrases { .. }
        )
    });
    // English banned phrases do not translate into a useful Hindi/Urdu blacklist. The
    // language's prose notes and checker supply idiomatic restrictions.
    if !lang.script().has_case() {
        manifest
            .writing
            .gates
            .retain(|g| !matches!(g, retold_guide::manifest::Gate::BannedPhrases { .. }));
        for gate in &mut manifest.writing.gates {
            if let retold_guide::manifest::Gate::Regex { id, pattern, .. } = gate
                && id.as_str() == "ending-question"
            {
                // The question may be followed by a short call to follow.
                *pattern = r"[?؟][^?؟]{0,200}$".into();
            }
        }
    }
    if let Some(gates) = manifest.writing.language_gates.get(lang) {
        manifest.writing.gates.extend(gates.clone());
    }
    problems.extend(
        crate::story::gates(&script, &manifest)
            .into_iter()
            .map(|v| format!("{}: {}", v.gate, v.message)),
    );
    problems
}

/// Each unit's delivery from the storyboard (absent: normal, neutral, no pause).
fn deliveries(context: &Context) -> BTreeMap<usize, crate::story::Delivery> {
    let path = context
        .layout
        .stage(Stage::Storyboard)
        .join("storyboard.json");
    std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Storyboard>(&b).ok())
        .map(|board| {
            board
                .direction
                .shots
                .iter()
                .filter_map(|s| s.delivery.map(|d| (s.unit, d)))
                .collect()
        })
        .unwrap_or_default()
}

/// Consecutive units read the same way, with the pause before the group.
fn delivery_groups<'a>(
    units: &'a [LocalUnit],
    plan: &BTreeMap<usize, crate::story::Delivery>,
) -> Vec<(crate::story::Delivery, Vec<&'a LocalUnit>)> {
    let mut groups: Vec<(crate::story::Delivery, Vec<&LocalUnit>)> = Vec::new();
    for unit in units {
        let d = plan.get(&unit.id).copied().unwrap_or_default();
        match groups.last_mut() {
            Some((g, members)) if g.pace == d.pace && g.tone == d.tone && d.pause_before < 0.05 => {
                members.push(unit);
            }
            _ => groups.push((d, vec![unit])),
        }
    }
    groups
}

/// The narration audio. With varied delivery, each group of units is voiced with matching
/// controls (local voices: speed and expressiveness; ElevenLabs: inline tags in one take;
/// other voices: spoken direction) and the groups are joined with their pauses.
async fn synthesize_narration(
    context: &Context,
    lang: &LanguageTag,
    units: &[LocalUnit],
    voice: &retold_guide::channel::VoiceSpec,
) -> Result<Asset, MediaError> {
    let folder = context.layout.stage(Stage::Voice).join(lang.to_string());
    let by_unit = deliveries(context);
    let groups = delivery_groups(units, &by_unit);
    let plain = groups.len() <= 1 && groups.iter().all(|(d, _)| d.pause_before < 0.05);
    let tags = voice.model.starts_with("elevenlabs/");
    if plain || tags {
        let text = if tags && !plain {
            groups
                .iter()
                .map(|(d, members)| {
                    let owned: Vec<_> = members.iter().map(|u| (*u).clone()).collect();
                    format!(
                        "{}{}",
                        eleven_tags(*d),
                        directed_text(context, &owned, lang, &voice.model)
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            directed_text(context, units, lang, &voice.model)
        };
        let request = SpeechRequest {
            text,
            voice: voice.voice.clone(),
            instructions: voice.instructions.clone(),
            language: lang.clone(),
        };
        return context
            .media
            .speech(
                &format!("voice-{lang}"),
                &voice.provider,
                &voice.model,
                &request,
                &folder.join("narration.mp3"),
            )
            .await;
    }
    let backend = if crate::voices::local(context, &voice.provider) {
        VoiceBackend::Local
    } else {
        VoiceBackend::Remote
    };
    let mut parts = Vec::new();
    for (i, (d, members)) in groups.iter().enumerate() {
        let owned: Vec<_> = members.iter().map(|u| (*u).clone()).collect();
        let request = SpeechRequest {
            text: directed_text(context, &owned, lang, &voice.model),
            voice: voice.voice.clone(),
            instructions: performance_instructions(voice.instructions.as_deref(), *d, backend)?,
            language: lang.clone(),
        };
        let part = context
            .media
            .speech(
                &format!("voice-{lang}-g{i}"),
                &voice.provider,
                &voice.model,
                &request,
                &folder.join(format!("narration-g{i}.mp3")),
            )
            .await?;
        parts.push((d.pause_before, part));
    }
    join_takes(context, &parts, &folder.join("narration-performed.wav")).await
}

/// ElevenLabs v3 audio tags for a delivery.
fn eleven_tags(d: crate::story::Delivery) -> String {
    use crate::story::{Pace, Tone};
    let mut tags = String::new();
    if d.pause_before >= 0.05 {
        tags.push_str("[pause] ");
    }
    let tone = [
        (Tone::Neutral, ""),
        (Tone::Warm, "[warmly] "),
        (Tone::Tense, "[tense] "),
        (Tone::Dark, "[ominous] "),
        (Tone::Whisper, "[whispers] "),
        (Tone::Excited, "[excited] "),
    ]
    .into_iter()
    .find(|(candidate_tone, ..)| *candidate_tone == d.tone)
    .map_or("", |(_, tag_str)| tag_str);
    tags.push_str(tone);
    tags.push_str(match d.pace {
        Pace::Slow => "[slowly] ",
        Pace::Normal => "",
        Pace::Fast => "[quickly] ",
    });
    tags
}

/// Voice instructions for one delivery group: local voices take their control JSON with
/// speed and expressiveness set; remote voices get a spoken direction added.
fn performance_instructions(
    base: Option<&str>,
    d: crate::story::Delivery,
    backend: VoiceBackend,
) -> Result<Option<String>, MediaError> {
    use crate::story::{Pace, Tone};
    let speed = match d.pace {
        Pace::Slow => 0.9,
        Pace::Normal => 1.0,
        Pace::Fast => 1.08,
    };
    let (exaggeration, words) = [
        (Tone::Neutral, 0.5, ""),
        (Tone::Warm, 0.55, "warm and gentle"),
        (Tone::Tense, 0.7, "tense and urgent"),
        (Tone::Dark, 0.35, "low, grave and ominous"),
        (Tone::Whisper, 0.25, "hushed, almost a whisper"),
        (Tone::Excited, 0.75, "bright and excited"),
    ]
    .into_iter()
    .find(|(candidate_tone, ..)| *candidate_tone == d.tone)
    .map_or((0.5, ""), |(_, ex, wd)| (ex, wd));
    if backend == VoiceBackend::Local {
        let mut controls: retold_voice::local::Controls = base
            .and_then(|b| serde_json::from_str(b).ok())
            .unwrap_or_default();
        controls.speed = speed;
        controls.exaggeration = exaggeration;
        return serde_json::to_string(&controls)
            .map(Some)
            .map_err(|e| EngineError::Setup(e.to_string()).into());
    }
    let pace = match d.pace {
        Pace::Slow => "slowly, letting each line land",
        Pace::Normal => "",
        Pace::Fast => "a little faster, with urgency",
    };
    let direction: Vec<_> = [words, pace]
        .into_iter()
        .filter(|w| !w.is_empty())
        .collect();
    Ok(match (base, direction.is_empty()) {
        (base, true) => base.map(str::to_owned),
        (Some(base), false) => Some(format!("{base} Read this part {}.", direction.join(", "))),
        (None, false) => Some(format!("Read this part {}.", direction.join(", "))),
    })
}

/// Joins voiced groups into one mono 24 kHz take, with silence before each as planned.
async fn join_takes(
    context: &Context,
    parts: &[(f64, Asset)],
    out: &std::path::Path,
) -> Result<Asset, MediaError> {
    let binary = std::env::var_os("RETOLD_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let mut args: Vec<String> = vec!["-nostdin".into(), "-y".into(), "-v".into(), "error".into()];
    let mut filter = String::new();
    for (i, (pause, part)) in parts.iter().enumerate() {
        args.extend(["-i".into(), part.path.display().to_string()]);
        let delay = (pause.clamp(0.0, 2.0) * 1000.0).round();
        let _ = write!(
            filter,
            "[{i}:a]aresample=24000,aformat=channel_layouts=mono,adelay={delay}:all=1[a{i}];"
        );
    }
    for i in 0..parts.len() {
        let _ = write!(filter, "[a{i}]");
    }
    let _ = write!(filter, "concat=n={}:v=0:a=1[out]", parts.len());
    args.extend([
        "-filter_complex".into(),
        filter,
        "-map".into(),
        "[out]".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        out.display().to_string(),
    ]);
    let parent = out
        .parent()
        .ok_or_else(|| EngineError::Setup("narration has no folder".into()))?;
    let result = retold_providers::process::run(
        std::path::Path::new(&binary),
        &args,
        "",
        parent,
        &retold_providers::process::Limits::default(),
        &context.media.cancel,
    )
    .await?;
    if result.status != Some(0) {
        return Err(MediaError::Quality(format!(
            "joining narration takes failed: {}",
            result.stderr.trim()
        )));
    }
    let bytes = std::fs::read(out).map_err(|e| EngineError::io(out, e))?;
    Ok(Asset {
        path: out.to_owned(),
        content_hash: blake3::hash(&bytes).to_hex().to_string(),
        provider: "local-join".into(),
        model: format!("{} takes", parts.len()),
        usage: retold_providers::media::MediaUsage {
            generation: None,
            cost: Some(retold_core::Usd::ZERO),
        },
        quote: retold_providers::media::Quote {
            ceiling: retold_core::Usd::ZERO,
            provider_tag: None,
        },
        request_id: String::new(),
    })
}

/// Respellings for the narrator: the research's pronunciations in the source language,
/// overridden and extended by the channel's own list, compiled once per edition.
fn pronunciations(context: &Context, lang: &LanguageTag) -> Vec<(regex::Regex, String)> {
    let mut map = BTreeMap::new();
    if *lang == context.guide.manifest().format.source_language
        && let Ok(bytes) = std::fs::read(context.layout.stage(Stage::Research).join("dossier.json"))
        && let Ok(dossier) = serde_json::from_slice::<crate::dossier::Dossier>(&bytes)
    {
        for p in dossier.pronunciations {
            if !p.name.trim().is_empty() && !p.say.trim().is_empty() {
                map.insert(p.name, p.say);
            }
        }
    }
    if let Some(channel) = context.channel.file().pronunciations.get(lang) {
        map.extend(channel.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    map.into_iter()
        .filter_map(|(spelling, said)| {
            let pattern = format!(r"\b{}\b", regex::escape(&spelling));
            regex::Regex::new(&pattern).ok().map(|re| (re, said))
        })
        .collect()
}

fn directed_text(
    context: &Context,
    units: &[LocalUnit],
    lang: &LanguageTag,
    model: &str,
) -> String {
    let tags = model.starts_with("elevenlabs/");
    let mut parts = Vec::new();
    let mut previous = "";
    let respellings = pronunciations(context, lang);
    if tags && let Some(direction) = context.guide.manifest().narration.direction.get(lang) {
        parts.push(direction.clone());
    }
    for unit in units {
        if tags
            && unit.section != previous
            && let Some(section) = context
                .guide
                .manifest()
                .structure
                .sections
                .iter()
                .find(|s| s.id.as_str() == unit.section)
            && let Some(voice) = &section.voice
        {
            parts.push(voice.tag.clone());
        }
        let mut narration = unit.text.clone();
        for (spelling, said) in &respellings {
            narration = spelling.replace_all(&narration, said.as_str()).into_owned();
        }
        parts.push(narration);
        previous = &unit.section;
    }
    parts.join(" ")
}

/// Maps each timed word into the track's script, retrying with checker feedback.
async fn transliterated_blocks(
    context: &Context,
    variant: &Variant,
    track: &retold_core::CaptionTrack,
    words: &[TimedWord],
) -> Result<(Vec<CaptionBlock>, Vec<Cue>), MediaError> {
    let source: Vec<String> = words.iter().map(|w| w.text.clone()).collect();
    let target = track.lang.script();
    let mut feedback = String::new();
    for attempt in 0..TRANSLITERATION_ATTEMPTS {
        let step = if attempt == 0 {
            format!("transliterate-{}-{}", variant.narration, track.lang)
        } else {
            format!(
                "transliterate-{}-{}-{attempt}",
                variant.narration, track.lang
            )
        };
        let prompt = transliteration_prompt(&source, &variant.narration, &track.lang, &feedback)?;
        let mapping: WordMapping = context
            .ask(
                &step,
                context.guide.manifest().roles.transcreate,
                prompt,
                vec![],
            )
            .await?;
        match check_transliteration(&source, &mapping.words, target) {
            Ok(()) => {
                let mapped: Vec<_> = words
                    .iter()
                    .zip(mapping.words)
                    .map(|(w, text)| TimedWord {
                        text,
                        start: w.start,
                        end: w.end,
                    })
                    .collect();
                return Ok(word_blocks(context, &mapped, &track.lang));
            }
            Err(problem) => feedback = problem,
        }
    }
    Err(MediaError::Quality(format!(
        "{} captions: transliteration failed after {TRANSLITERATION_ATTEMPTS} attempts: {feedback}",
        track.lang.display_name()
    )))
}

async fn caption_track(
    context: &Context,
    variant: &Variant,
    track: &retold_core::CaptionTrack,
    alignment: &Alignment,
) -> Result<(Vec<CaptionBlock>, Vec<Cue>), MediaError> {
    let words: Vec<TimedWord> = alignment
        .units
        .iter()
        .flat_map(|u| u.words.clone())
        .collect();
    match variant.caption_kind(track) {
        CaptionKind::Same => Ok(word_blocks(context, &words, &track.lang)),
        CaptionKind::Transliteration => {
            transliterated_blocks(context, variant, track, &words).await
        }
        CaptionKind::Translation => {
            let cues = subtitles::cues_from_words(&words, 32, 4.0);
            let phrases: Vec<_> = cues
                .iter()
                .enumerate()
                .map(|(id, c)| Phrase {
                    id,
                    text: c.text.clone(),
                })
                .collect();
            let translated: PhraseTranslation = context.ask(&format!("translate-{}-{}",variant.narration,track.lang),context.guide.manifest().roles.transcreate,
                format!("Translate each phrase into {}, preserving ids and meaning. Keep concise natural subtitle phrasing. Do not add facts. Phrases: {}",track.lang.display_name(),serde_json::to_string(&phrases).map_err(|e|EngineError::Setup(e.to_string()))?),vec![]).await?;
            if translated.phrases.len() != cues.len()
                || translated
                    .phrases
                    .iter()
                    .enumerate()
                    .any(|(i, p)| p.id != i || p.text.trim().is_empty())
            {
                return Err(MediaError::Quality(
                    "translated captions lost or reordered phrases".into(),
                ));
            }
            let cues: Vec<_> = cues
                .iter()
                .zip(translated.phrases)
                .map(|(c, p)| Cue {
                    start: c.start,
                    end: c.end,
                    text: p.text,
                })
                .collect();
            let blocks = cues
                .iter()
                .map(|c| CaptionBlock {
                    text: c.text.clone(),
                    start: c.start,
                    end: c.end,
                    highlight: None,
                    pop: true,
                })
                .collect();
            Ok((blocks, cues))
        }
    }
}

const TRANSLITERATION_ATTEMPTS: usize = 3;

/// Asks for a word-for-word change of script, keeping the sound and never the meaning.
fn transliteration_prompt(
    source: &[String],
    from: &LanguageTag,
    to: &LanguageTag,
    feedback: &str,
) -> Result<String, MediaError> {
    let script = to.script().name();
    let example = match (from.language(), to.script()) {
        ("ur", retold_core::WritingScript::Deva) => {
            "Example: [\"ایک\", \"ویمپائر\", \"آتا\", \"ہے۔\"] → [\"एक\", \"वैम्पायर\", \"आता\", \"है।\n"
        }
        ("hi", retold_core::WritingScript::Arab) => {
            "Example: [\"एक\", \"वैम्पायर\", \"आता\", \"है।\"] → [\"ایک\", \"ویمپائر\", \"آتا\", \"ہے۔\"]\n"
        }
        (_, retold_core::WritingScript::Latn) => {
            "Example: [\"ایک\", \"ویمپائر\", \"آتا\", \"ہے۔\"] → [\"ek\", \"vampire\", \"aata\", \"hai.\"]\n"
        }
        _ => "",
    };
    let retry = if feedback.is_empty() {
        String::new()
    } else {
        format!("\nYour previous answer was rejected: {feedback}\nFix every listed word.\n")
    };
    let input = serde_json::to_string(source).map_err(|e| EngineError::Setup(e.to_string()))?;
    Ok(format!(
        "Transliterate these {from_name} words into {script} script, as {to_name} readers would \
         write their sound. Do not translate, and do not split, merge, add or drop words: return \
         exactly {n} words, one per input word, in the same order. Every letter of every output \
         word must be a {script} letter; keep numbers, and turn sentence punctuation into its \
         {script} form.\n{example}{retry}\nThere are {n} input words.\nInput: {input}",
        from_name = from.display_name(),
        to_name = to.display_name(),
        n = source.len(),
    ))
}

/// Checks a transliteration in code: same word count, one token per word, and every letter
/// in the target script. Returns a description of what is wrong, naming the bad words.
fn check_transliteration(
    source: &[String],
    output: &[String],
    target: retold_core::WritingScript,
) -> Result<(), String> {
    if output.len() != source.len() {
        return Err(format!(
            "returned {} words for {} input words",
            output.len(),
            source.len()
        ));
    }
    let bad: Vec<String> = output
        .iter()
        .enumerate()
        .filter(|(_, w)| {
            w.trim().is_empty()
                || w.split_whitespace().count() != 1
                || !retold_lang::script::is_written_in(w, target)
        })
        .map(|(i, w)| format!("#{} \"{}\" (for \"{}\")", i + 1, w, source[i]))
        .collect();
    if bad.is_empty() {
        Ok(())
    } else {
        let shown: Vec<&str> = bad.iter().take(12).map(String::as_str).collect();
        Err(format!(
            "{} of {} words are not single {} words: {}{}",
            bad.len(),
            output.len(),
            target.name(),
            shown.join(", "),
            if bad.len() > shown.len() { ", …" } else { "" }
        ))
    }
}

#[must_use]
pub fn word_blocks(
    context: &Context,
    words: &[TimedWord],
    lang: &LanguageTag,
) -> (Vec<CaptionBlock>, Vec<Cue>) {
    let settings = &context.channel.file().captions;
    let groups = captions::group(
        words,
        lang.script(),
        captions::Rules {
            max_words: settings.max_words as usize,
            max_chars: settings.max_chars as usize,
            uppercase: settings.uppercase,
            tail: 0.2,
        },
    );
    let mut blocks = Vec::new();
    for group in groups {
        for i in 0..group.words.len() {
            let start = group.words[i].start;
            let end = group.words.get(i + 1).map_or(group.end, |w| w.start);
            if end <= start {
                continue;
            }
            let (a, b) = group.char_range(i);
            blocks.push(CaptionBlock {
                text: group.line(),
                start,
                end,
                highlight: Some([a, b]),
                pop: i == 0,
            });
        }
    }
    (blocks, subtitles::cues_from_words(words, 32, 4.0))
}

/// For a part of a canon: the part number and work, which titles must carry.
fn series_note(context: &Context) -> String {
    std::fs::read(context.layout.stage(Stage::Research).join("canon-slice.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<crate::canon::Slice>(&b).ok())
        .filter(|s| s.number > 0)
        .map(|s| format!(" This video is part {} of {} in {} ({}): every title names {} and says \"Part {}\" in the target language.", s.number, s.of, s.work, s.part.title, s.work, s.number))
        .unwrap_or_default()
}

async fn package(
    context: &Context,
    lang: &LanguageTag,
    units: &[LocalUnit],
) -> Result<Packaging, MediaError> {
    let rules = retold_guide::template::render("packaging.rules",&context.guide.manifest().packaging.rules,
        json!({"inputs":context.meta.inputs.template_context(),"channel":context.channel.template_context(),"lang":lang.to_string()})).map_err(|e|EngineError::Template(e.to_string()))?;
    let platforms: Vec<String> = context
        .guide
        .manifest()
        .packaging
        .platforms
        .iter()
        .map(|p| {
            serde_json::to_value(p)
                .unwrap_or_default()
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    let packaging = &context.guide.manifest().packaging;
    let series = series_note(context);
    let mut post: Packaging = context.ask(&format!("package-{lang}"),context.guide.manifest().roles.package,
        format!("Write posting metadata in {} for each of {:?}, exactly once.{series} User guide rules: {}. Titles must be nonempty and at most {} characters; include exactly {} distinct hashtags starting #. Do not add spoiler, genre or disclosure wording unless requested by these rules. Subject: {}. Narration: {}",lang.display_name(),platforms,rules,packaging.title_max_chars,packaging.hashtag_count,context.meta.title,units.iter().map(|u|u.text.as_str()).collect::<Vec<_>>().join(" ")),vec![]).await?;
    let actual: std::collections::BTreeSet<_> =
        post.posts.iter().map(|p| p.platform.clone()).collect();
    if actual != platforms.into_iter().collect()
        || actual.len() != post.posts.len()
        || post.posts.iter().any(|p| {
            p.title.trim().is_empty()
                || p.title.chars().count() > packaging.title_max_chars
                || p.description.trim().is_empty()
                || p.hashtags.len() != packaging.hashtag_count
                || p.hashtags
                    .iter()
                    .any(|h| !h.starts_with('#') || h.split_whitespace().count() != 1)
                || p.hashtags
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != packaging.hashtag_count
        })
    {
        return Err(MediaError::Quality(
            "posting metadata failed platform/title/hashtag checks".into(),
        ));
    }
    if let Some(notice) = packaging.notices.get(lang) {
        let ctx = json!({"inputs":context.meta.inputs.template_context(),"channel":context.channel.template_context(),"lang":lang.to_string(),"guide":context.guide.manifest()});
        let prefix =
            retold_guide::template::render("packaging.notices.prefix", &notice.prefix, &ctx)
                .map_err(|e| EngineError::Template(e.to_string()))?;
        let suffix =
            retold_guide::template::render("packaging.notices.suffix", &notice.suffix, &ctx)
                .map_err(|e| EngineError::Template(e.to_string()))?;
        for p in &mut post.posts {
            p.description = [prefix.trim(), p.description.trim(), suffix.trim()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
        }
    }
    for p in &post.posts {
        fsx::write_json_atomic(
            &context
                .layout
                .post_dir(lang)
                .join(format!("{}.json", p.platform)),
            p,
        )?;
        fsx::write_atomic(
            &context
                .layout
                .post_dir(lang)
                .join(format!("{}.txt", p.platform)),
            format!(
                "{}\n\n{}\n\n{}\n",
                p.title,
                p.description,
                p.hashtags.join(" ")
            )
            .as_bytes(),
        )?;
    }
    Ok(post)
}

#[cfg(test)]
mod duration_tests {
    use super::duration_fits;
    #[test]
    fn duration_gate_checks_the_complete_timeline_including_ending_hold() {
        assert!(duration_fits(94.92, 1.2, [95.0, 115.0]));
        assert!(!duration_fits(93.0, 1.2, [95.0, 115.0]));
        assert!(!duration_fits(114.0, 1.2, [95.0, 115.0]));
        assert!(!duration_fits(f64::NAN, 1.2, [95.0, 115.0]));
    }
}

#[cfg(test)]
mod transliteration_tests {
    use super::*;

    fn v(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn rejects_untransliterated_and_mixed_words_naming_them() {
        let source = v(&["ایک", "ویمپائر", "ہے۔"]);
        let deva = retold_core::WritingScript::Deva;
        assert!(check_transliteration(&source, &v(&["एक", "वैम्पायर", "है।"]), deva).is_ok());
        let err = check_transliteration(&source, &v(&["एک", "ویمپائر", "है।"]), deva).unwrap_err();
        assert!(err.starts_with("2 of 3"), "{err}");
        assert!(
            err.contains("#1") && err.contains("#2") && !err.contains("#3"),
            "{err}"
        );
        assert!(check_transliteration(&source, &v(&["एक", "वैम्पायर"]), deva).is_err());
        assert!(check_transliteration(&source, &v(&["एक", "वैम् पायर", "है।"]), deva).is_err());
    }

    #[test]
    fn prompt_names_the_script_and_carries_feedback() {
        let ur: LanguageTag = "ur".parse().unwrap();
        let hi: LanguageTag = "hi".parse().unwrap();
        let p = transliteration_prompt(&v(&["ایک"]), &ur, &hi, "").unwrap();
        assert!(
            p.contains("Devanagari script") && p.contains("Example") && !p.contains("rejected")
        );
        let p = transliteration_prompt(&v(&["ایک"]), &ur, &hi, "1 of 1 words").unwrap();
        assert!(p.contains("rejected: 1 of 1 words"));
    }
}

#[cfg(test)]
mod length_trim_tests {
    use super::length_target;

    #[test]
    fn aims_for_the_middle_of_the_range() {
        let (target, change) = length_target(330, 120.0, [95.0, 115.0]);
        assert_eq!(target, 289);
        assert_eq!(change, "cut about 41 words");
        let (target, change) = length_target(200, 80.0, [95.0, 115.0]);
        assert_eq!(target, 263);
        assert!(change.starts_with("add about 63"));
    }
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    use crate::story::{Delivery, Pace, Tone};

    fn unit(id: usize) -> LocalUnit {
        LocalUnit {
            id,
            section: "s".into(),
            text: format!("line {id}."),
        }
    }

    #[test]
    fn units_group_by_delivery_and_split_at_pauses() {
        let units: Vec<_> = (0..5).map(unit).collect();
        let dark = Delivery {
            pace: Pace::Slow,
            tone: Tone::Dark,
            pause_before: 0.0,
        };
        let plan = BTreeMap::from([
            (
                2,
                Delivery {
                    pause_before: 1.0,
                    ..dark
                },
            ),
            (3, dark),
        ]);
        let groups = delivery_groups(&units, &plan);
        let ids: Vec<Vec<usize>> = groups
            .iter()
            .map(|(_, m)| m.iter().map(|u| u.id).collect())
            .collect();
        assert_eq!(ids, [vec![0, 1], vec![2, 3], vec![4]]);
        assert!((groups[1].0.pause_before - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn deliveries_become_local_controls_or_spoken_direction() {
        let d = Delivery {
            pace: Pace::Slow,
            tone: Tone::Whisper,
            pause_before: 0.0,
        };
        let local = performance_instructions(
            Some(r#"{"sample":null,"sample_hash":null,"exaggeration":0.5,"speed":1.0,"seed":7,"cfg_weight":0.5}"#),
            d,
            VoiceBackend::Local,
        )
        .unwrap()
        .unwrap();
        let controls: retold_voice::local::Controls = serde_json::from_str(&local).unwrap();
        assert!(
            (controls.speed - 0.9).abs() < 1e-9
                && (controls.exaggeration - 0.25).abs() < 1e-9
                && controls.seed == 7
        );
        let remote = performance_instructions(Some("Calm narrator."), d, VoiceBackend::Remote)
            .unwrap()
            .unwrap();
        assert!(
            remote.starts_with("Calm narrator.")
                && remote.contains("whisper")
                && remote.contains("slowly")
        );
        assert_eq!(
            performance_instructions(None, Delivery::default(), VoiceBackend::Remote).unwrap(),
            None
        );
        assert_eq!(eleven_tags(d), "[whispers] [slowly] ");
    }
}
