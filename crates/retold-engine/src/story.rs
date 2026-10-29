//! Typed writing artifacts and deterministic script/storyboard gates.

use std::collections::HashSet;

use retold_core::num;
use retold_guide::manifest::{Gate, GuideManifest};
use retold_lang::text;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Beat {
    pub section: String,
    pub facts: Vec<String>,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub storyline: String,
    pub characters: Vec<String>,
    pub tone: String,
    pub open_loop: String,
    pub emotional_peak: String,
    pub omitted: Vec<String>,
    pub beats: Vec<Beat>,
    /// With a research dossier: a decision for every dossier beat, in story order.
    #[serde(default)]
    pub outline: Vec<OutlineBeat>,
    /// With a research dossier: the iconic element ids this script must set up and pay off.
    #[serde(default)]
    pub iconic: Vec<String>,
    /// Profiles of the planned characters, copied from the character bible (never written
    /// by the planner).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub cast: Vec<crate::dossier::Profile>,
    /// Recurring places from the research dossier, each drawn once as a location reference.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub settings: Vec<crate::dossier::Place>,
    /// Signature objects of the planned cast, each drawn once as a prop reference.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub props: Vec<String>,
    /// Text of the chosen iconic elements that are not quotes (symbols, objects, images),
    /// in plan order; the cover is built around the first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub iconic_images: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlineBeat {
    /// Dossier beat id.
    pub id: String,
    /// Section that tells it; any section when dropped.
    pub section: String,
    pub include: bool,
    /// Why it is dropped; empty when included.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ranking {
    /// Zero-based candidate index, selected blind.
    pub winner: usize,
    pub scores: Vec<f64>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SectionText {
    pub section: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub sections: Vec<SectionText>,
}

impl Script {
    #[must_use]
    pub fn narration(&self) -> String {
        self.sections
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FactIssue {
    pub claim: String,
    pub problem: String,
    /// Exact source quote supporting the correction; empty if the claim is unsupported.
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FactCheck {
    pub issues: Vec<FactIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub id: usize,
    pub section: String,
    pub narration: String,
    pub estimated_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shot {
    pub unit: usize,
    pub shot: String,
    pub location: String,
    pub action: String,
    pub mood: String,
    pub characters: Vec<String>,
    /// Fractions of frame width and height.
    pub focus: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<ShotMotion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallax_move: Option<ParallaxMove>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hero: bool,
    /// The expression each on-screen character shows, chosen from their profile's emotions so
    /// the frame draws the matching pose from their reference sheet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub poses: Vec<CastPose>,
    /// The plan setting this shot takes place in, exactly as named, or empty when none fits.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub setting: String,
    /// Visible state carried from earlier shots: time of day, light, weather, injuries,
    /// clothing changes, who holds what. Required whenever the shot names a setting.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub continuity: String,
    /// An earlier unit whose frame this shot recuts (cropped around `focus`) instead of
    /// generating a new image: same moment, setting, characters and expressions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reuse: Option<usize>,
    /// Diegetic sound for this unit: background ambience and a one-shot effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<SceneSound>,
    /// Music under this unit; absent keeps the previous unit's music.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<Score>,
    /// How the narrator reads this unit; absent means normal and neutral with no pause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<Delivery>,
    /// Plan props visible in this shot, exactly as named.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    Slow,
    #[default]
    Normal,
    Fast,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Neutral,
    Warm,
    Tense,
    Dark,
    Whisper,
    Excited,
}

/// Sound in the scene: `ambience` continues under the place (empty for none), `effect` is
/// one short sound at the start of the unit (empty for none).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneSound {
    pub ambience: String,
    pub effect: String,
}

/// The score moods a director is asked to choose from; music tagged with them can be matched.
pub const SCORE_MOODS: [&str; 8] = [
    "calm",
    "tense",
    "dark",
    "sad",
    "warm",
    "mysterious",
    "epic",
    "hopeful",
];

/// Music for one unit: the mood it continues or starts, a sting on a reveal, or a moment
/// of silence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Score {
    /// One word, e.g. one of [`SCORE_MOODS`].
    pub mood: String,
    pub sting: bool,
    pub silence: bool,
}

/// Narration performance for one unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub pace: Pace,
    pub tone: Tone,
    /// Silence before the unit, in seconds (0–2): for reveals and after cliffhangers.
    pub pause_before: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CastPose {
    pub name: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Direction {
    pub shots: Vec<Shot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Violation {
    pub gate: String,
    pub message: String,
}

/// Checks every manifest gate. Missing sections, length, repeated starts and rhythm are
/// hard failures, including on the last repair pass.
#[must_use]
pub fn gates(script: &Script, m: &GuideManifest) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut add = |gate: &str, message: String| {
        out.push(Violation {
            gate: gate.into(),
            message,
        });
    };
    let expected: Vec<_> = m.structure.sections.iter().map(|s| s.id.as_str()).collect();
    let actual: Vec<_> = script.sections.iter().map(|s| s.section.as_str()).collect();
    if actual != expected || script.sections.iter().any(|s| s.text.trim().is_empty()) {
        add(
            "structure",
            format!(
                "nonempty sections must appear exactly once in this order: {}",
                expected.join(", ")
            ),
        );
    }
    let spoken = spoken_sections(script, m);
    let narration = spoken
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let lang = &m.format.source_language;
    if let Some(length) = m.length.get(lang) {
        if let Some([lo, hi]) = length.words {
            let count = text::word_count(&narration);
            if count < lo as usize || count > hi as usize {
                add("length", format!("{count} words; expected {lo}–{hi}"));
            }
        }
        if let Some([lo, hi]) = length.seconds {
            let seconds = crate::speech::seconds(&narration, lang, m);
            if !(lo..=hi).contains(&seconds) {
                add(
                    "length",
                    format!("{seconds:.1} estimated seconds; expected {lo}–{hi}"),
                );
            }
        }
    }
    let sentences = text::sentences(&narration, lang.script());
    out.extend(writing_gates(&spoken, &narration, &sentences, m));
    out
}

fn spoken_sections(script: &Script, m: &GuideManifest) -> Vec<SectionText> {
    script
        .sections
        .iter()
        .map(|section| {
            let text = if m.narration.mode == retold_guide::manifest::NarrationMode::Dialogue {
                retold_voice::DialogueScript::parse(&section.text).map_or_else(
                    |_| section.text.clone(),
                    |s| {
                        s.turns
                            .into_iter()
                            .map(|t| t.text)
                            .collect::<Vec<_>>()
                            .join(" ")
                    },
                )
            } else {
                section.text.clone()
            };
            SectionText {
                section: section.section.clone(),
                text,
            }
        })
        .collect()
}
/// Dialogue cues are checked during draft selection and repair, before script activation.
#[must_use]
pub fn dialogue_gates(script: &Script, speakers: &[String]) -> Vec<Violation> {
    script
        .sections
        .iter()
        .filter_map(|section| {
            retold_voice::DialogueScript::parse(&section.text)
                .and_then(|s| s.validate_speakers(speakers))
                .err()
                .map(|error| Violation {
                    gate: "dialogue".into(),
                    message: format!("{}: {error}", section.section),
                })
        })
        .collect()
}

fn writing_gates(
    sections: &[SectionText],
    narration: &str,
    sentences: &[String],
    m: &GuideManifest,
) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut add = |gate: &str, message: String| {
        out.push(Violation {
            gate: gate.into(),
            message,
        });
    };
    for gate in &m.writing.gates {
        match gate {
            Gate::BannedPhrases { id, list } => {
                let lower = narration.to_lowercase().replace('’', "'");
                for phrase in list {
                    if lower.contains(&phrase.to_lowercase()) {
                        add(id.as_str(), format!("remove banned phrase `{phrase}`"));
                    }
                }
            }
            Gate::Regex {
                id,
                pattern,
                section,
                message,
            } => {
                let target = section.as_ref().map_or_else(
                    || narration.to_owned(),
                    |id| {
                        sections
                            .iter()
                            .find(|s| s.section == id.as_str())
                            .map_or_else(String::new, |s| s.text.clone())
                    },
                );
                if !regex::Regex::new(pattern).is_ok_and(|re| re.is_match(&target)) {
                    add(id.as_str(), message.clone());
                }
            }
            Gate::NoRepeatedSentenceStart { id } => {
                for message in repeated_starts(sentences) {
                    add(id.as_str(), message);
                }
            }
            Gate::SentenceLengthVariety {
                id,
                max_same_length_run,
            } => {
                for message in same_length_runs(sentences, *max_same_length_run) {
                    add(id.as_str(), message);
                }
            }
        }
    }
    out
}

fn repeated_starts(sentences: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut last = String::new();
    let mut previous = "";
    for s in sentences {
        let start = s
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        if !start.is_empty() && start == last {
            out.push(format!(
                "adjacent sentences both start with `{start}`: {previous} / {s}; change the opening of one sentence without changing its facts"
            ));
        }
        last = start;
        previous = s;
    }
    out
}

fn same_length_runs<N>(sentences: &[String], max_same_length_run: N) -> Vec<String>
where
    N: Copy + std::fmt::Display + TryInto<usize>,
{
    let limit = max_same_length_run.try_into().unwrap_or(usize::MAX);
    let counts: Vec<_> = sentences.iter().map(|s| text::word_count(s)).collect();
    let classes: Vec<_> = counts
        .iter()
        .map(|n| {
            if *n <= 6 {
                0
            } else if *n <= 14 {
                1
            } else {
                2
            }
        })
        .collect();
    let mut out = Vec::new();
    let mut first = 0;
    while first < classes.len() {
        let mut end = first + 1;
        while end < classes.len() && classes[end] == classes[first] {
            end += 1;
        }
        if end - first > limit {
            out.push(format!(
                "Sentences {}–{} have the same length class (short ≤6, medium 7–14, long ≥15), exceeding the maximum run of {}. Word counts: {:?}. Vary sentence lengths in this exact span while preserving facts and unit ids; splitting a sentence within its unit is allowed. Text: {}",
                first + 1,
                end,
                max_same_length_run,
                &counts[first..end],
                sentences[first..end].join(" / ")
            ));
        }
        first = end;
    }
    out
}

#[must_use]
pub fn segment(script: &Script, m: &GuideManifest, quality: crate::Quality) -> Vec<Unit> {
    let lang = &m.format.source_language;
    let range = match quality {
        crate::Quality::Lean => m.visuals.lean_seconds_per_unit,
        crate::Quality::Standard | crate::Quality::Premium => m.visuals.seconds_per_unit,
    };
    let mut units = Vec::new();
    for s in &script.sections {
        if m.narration.mode == retold_guide::manifest::NarrationMode::Dialogue {
            for line in s.text.lines().map(str::trim).filter(|s| !s.is_empty()) {
                let spoken = line.split_once(':').map_or(line, |(_, t)| t.trim());
                units.push(Unit {
                    id: units.len(),
                    section: s.section.clone(),
                    estimated_seconds: crate::speech::seconds(spoken, lang, m),
                    narration: line.into(),
                });
            }
            continue;
        }
        let sentences = text::sentences(&s.text, lang.script());
        for narration in retold_lang::units::pack(&sentences, range, lang) {
            units.push(Unit {
                id: units.len(),
                section: s.section.clone(),
                estimated_seconds: crate::speech::seconds(&narration, lang, m),
                narration,
            });
        }
    }
    units
}

/// A reused frame must come from an earlier, freshly drawn shot of the same moment, and
/// reuse stays within the guide's share.
fn reuse_gates(d: &Direction, share: f64) -> Vec<String> {
    let mut errors = Vec::new();
    let mut reused = 0_usize;
    for s in &d.shots {
        let Some(source) = s.reuse else { continue };
        reused += 1;
        let ok = d.shots.iter().find(|o| o.unit == source).is_some_and(|o| {
            o.unit < s.unit
                && o.reuse.is_none()
                && o.setting == s.setting
                && o.characters == s.characters
                && o.poses
                    .iter()
                    .map(|p| (&p.name, p.expression.to_lowercase()))
                    .eq(s
                        .poses
                        .iter()
                        .map(|p| (&p.name, p.expression.to_lowercase())))
        });
        if !ok {
            errors.push(format!(
                "unit {}: reuse only an earlier newly drawn unit with the same setting, characters and expressions",
                s.unit
            ));
        }
    }
    if num::count_f64(reused) > share * num::count_f64(d.shots.len()) + f64::EPSILON {
        errors.push(format!(
            "at most {:.0}% of shots may reuse an earlier frame",
            share * 100.0
        ));
    }
    errors
}

/// With profiled characters, every on-screen character needs one expression from the first
/// five of their profile's emotions (the ones their reference sheet draws).
fn pose_gates(d: &Direction, plan: &Plan) -> Vec<String> {
    let mut errors = Vec::new();
    for s in &d.shots {
        for name in &s.characters {
            let Some(profile) = plan.cast.iter().find(|p| &p.name == name) else {
                continue;
            };
            let allowed: Vec<_> = profile
                .emotions
                .iter()
                .take(5)
                .map(|e| e.trim().to_lowercase())
                .collect();
            let chosen: Vec<_> = s.poses.iter().filter(|p| &p.name == name).collect();
            if chosen.len() != 1 || !allowed.contains(&chosen[0].expression.trim().to_lowercase()) {
                errors.push(format!(
                    "unit {}: give {name} exactly one pose with an expression from: {}",
                    s.unit,
                    allowed.join(", ")
                ));
            }
        }
        if !s.setting.is_empty() && !plan.settings.iter().any(|p| p.name == s.setting) {
            errors.push(format!(
                "unit {}: setting `{}` must be empty or one of the plan settings",
                s.unit, s.setting
            ));
        }
        if !s.setting.is_empty() && s.continuity.split_whitespace().count() < 3 {
            errors.push(format!(
                "unit {}: describe the continuity state carried into this shot",
                s.unit
            ));
        }
        if s.delivery
            .is_some_and(|d| !(0.0..=2.0).contains(&d.pause_before))
        {
            errors.push(format!(
                "unit {}: pause_before must be 0 to 2 seconds",
                s.unit
            ));
        }
        if let Some(prop) = s.props.iter().find(|p| !plan.props.contains(p)) {
            errors.push(format!("unit {}: prop `{prop}` is not a plan prop", s.unit));
        }
        if let Some(p) = s.poses.iter().find(|p| !s.characters.contains(&p.name)) {
            errors.push(format!(
                "unit {}: pose for {} who is not in the shot",
                s.unit, p.name
            ));
        }
    }
    errors
}

/// Checkers often splice several exact quotes with an ellipsis; accept evidence when every
/// spliced fragment appears in the source.
#[must_use]
pub fn evidence_in_source(evidence: &str, source: &str) -> bool {
    evidence
        .split(['[', ']'])
        .flat_map(|part| part.split("..."))
        .flat_map(|part| part.split('\u{2026}'))
        .map(str::trim)
        .filter(|fragment| !fragment.is_empty() && *fragment != "...")
        .all(|fragment| source.contains(fragment))
}

/// Structured-output providers often fill optional fields; when the guide disables parallax,
/// any motion the director chose cannot be used, so drop it instead of failing the gate.
pub fn drop_disabled_parallax(d: &mut Direction, m: &GuideManifest) {
    if m.visuals.parallax.enabled {
        return;
    }
    for shot in &mut d.shots {
        shot.motion = None;
        shot.parallax_move = None;
    }
}

#[must_use]
pub fn direction_gates(
    d: &Direction,
    units: &[Unit],
    plan: &Plan,
    m: &GuideManifest,
) -> Vec<String> {
    let mut errors = Vec::new();
    if d.shots.len() != units.len() || d.shots.iter().enumerate().any(|(i, s)| s.unit != i) {
        errors.push(
            "shots must contain exactly one entry per unit, with ascending zero-based unit ids"
                .into(),
        );
    }
    let parallax_count = d
        .shots
        .iter()
        .filter(|s| s.motion == Some(ShotMotion::Parallax))
        .count();
    let allowed_share = f64::from(u32::try_from(parallax_count).unwrap_or(u32::MAX))
        / f64::from(u32::try_from(d.shots.len().max(1)).unwrap_or(u32::MAX));
    if parallax_count > 0
        && (!m.visuals.parallax.enabled
            || allowed_share > m.visuals.parallax.max_share + f64::EPSILON)
    {
        errors
            .push("parallax shot share exceeds the guide limit or the feature is disabled".into());
    }
    errors.extend(pose_gates(d, plan));
    if d.shots.first().is_some_and(|s| !s.hero) {
        errors.push(
            "unit 0 opens the video: make it the most striking image and mark it hero=true".into(),
        );
    }
    let count = |f: fn(&Score) -> bool| {
        d.shots
            .iter()
            .filter(|s| s.score.as_ref().is_some_and(f))
            .count()
    };
    if count(|s| s.sting) > 4 || count(|s| s.silence) > 3 {
        errors
            .push("use at most 4 music stings and 3 silences, for the biggest moments only".into());
    }
    errors.extend(reuse_gates(d, m.visuals.reuse_share));
    let mut last = "";
    let mut run = 0;
    for s in &d.shots {
        if s.parallax_move.is_some() && s.motion != Some(ShotMotion::Parallax) {
            errors.push(format!(
                "unit {} specifies a parallax move without parallax motion",
                s.unit
            ));
        }
        if !m.visuals.shots.contains(&s.shot) {
            errors.push(format!("unit {} uses unknown shot `{}`", s.unit, s.shot));
        }
        if s.action.trim().is_empty() || s.location.trim().is_empty() || s.mood.trim().is_empty() {
            errors.push(format!("unit {} needs location, action and mood", s.unit));
        }
        if s.focus
            .iter()
            .any(|n| !n.is_finite() || !(0.0..=1.0).contains(n))
        {
            errors.push(format!(
                "unit {} focus must be fractions between zero and one",
                s.unit
            ));
        }
        if s.characters.iter().any(|c| !plan.characters.contains(c)) {
            errors.push(format!("unit {} names an unplanned character", s.unit));
        }
        run = if s.shot == last { run + 1 } else { 1 };
        if run > m.visuals.max_same_shot_run {
            errors.push(format!("unit {} repeats a shot too many times", s.unit));
        }
        last = &s.shot;
    }
    errors
}

#[must_use]
pub fn plan_gates(plan: &Plan, m: &GuideManifest) -> Vec<String> {
    let mut errors = Vec::new();
    let ids: Vec<_> = plan.beats.iter().map(|b| b.section.as_str()).collect();
    let expected: Vec<_> = m.structure.sections.iter().map(|s| s.id.as_str()).collect();
    if ids != expected
        || plan
            .beats
            .iter()
            .any(|b| b.facts.is_empty() || b.facts.iter().any(|f| f.trim().is_empty()))
    {
        errors.push("plan must give source facts for every section in guide order".into());
    }
    let unique: HashSet<_> = plan.characters.iter().map(|c| c.to_lowercase()).collect();
    if plan.characters.len() > m.structure.max_named_characters as usize
        || unique.len() != plan.characters.len()
        || plan.characters.iter().any(|c| c.trim().is_empty())
    {
        errors.push(format!(
            "plan allows at most {} distinct named characters",
            m.structure.max_named_characters
        ));
    }
    if plan.storyline.trim().is_empty()
        || plan.tone.trim().is_empty()
        || (m.structure.open_loops > 0 && plan.open_loop.trim().is_empty())
        || (m.structure.emotional_peak && plan.emotional_peak.trim().is_empty())
    {
        errors.push("plan needs storyline, tone, open loop and emotional peak".into());
    }
    errors
}

/// Evaluates dramatic beats against the mathematical delta gate (ΔE >= 1.0).
#[must_use]
pub fn dramatic_beat_gates(beats: &[retold_dramaturgy::beat::DramaticBeat]) -> Vec<Violation> {
    match retold_dramaturgy::beat::validate_beat_gates(beats) {
        Ok(_) => Vec::new(),
        Err(err) => vec![Violation {
            gate: "dramaturgy_delta_e".into(),
            message: err.to_string(),
        }],
    }
}

/// Evaluates sentences for oral breath cadence and banned AI clichés.
#[must_use]
pub fn cadence_gates(sentences: &[String]) -> Vec<Violation> {
    let engine = retold_dramaturgy::cadence::CadenceEngine::new();
    let report = engine.audit_sentences(sentences);
    report
        .violations
        .into_iter()
        .map(|v| Violation {
            gate: "oral_cadence".into(),
            message: format!("{}: {}", v.sentence, v.guidance),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShotMotion {
    Parallax,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParallaxMove {
    PushIn,
    Orbit,
    Pedestal,
    DollyZoom,
}
impl From<ParallaxMove> for retold_parallax::camera::CameraMove {
    fn from(value: ParallaxMove) -> Self {
        match value {
            ParallaxMove::PushIn => Self::PushIn,
            ParallaxMove::Orbit => Self::Orbit,
            ParallaxMove::Pedestal => Self::Pedestal,
            ParallaxMove::DollyZoom => Self::DollyZoom,
        }
    }
}

impl Plan {
    pub fn populate_from_dossier(
        &mut self,
        dossier: &crate::dossier::Dossier,
        bible: &crate::dossier::Bible,
    ) {
        for beat in &mut self.beats {
            let facts: Vec<String> = self
                .outline
                .iter()
                .filter(|o| o.include && o.section == beat.section)
                .filter_map(|o| dossier.beats.iter().find(|b| b.id == o.id))
                .map(|b| format!("{}: {}", b.id, b.text))
                .collect();
            if !facts.is_empty() {
                beat.facts = facts;
            } else if beat.facts.is_empty() {
                beat.facts = vec![format!("tease: {}", dossier.logline)];
            }
        }
        self.cast = self
            .characters
            .iter()
            .filter_map(|name| {
                bible
                    .characters
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(name))
                    .cloned()
            })
            .collect();
        self.settings.clone_from(&dossier.settings);
        self.props = self
            .cast
            .iter()
            .map(|c| c.signature_object.trim().to_owned())
            .filter(|o| !o.is_empty())
            .collect();
        self.props.dedup();
        self.iconic_images = self
            .iconic
            .iter()
            .filter_map(|id| dossier.iconic.iter().find(|i| &i.id == id))
            .filter(|i| i.quote.trim().is_empty())
            .map(|i| i.text.clone())
            .collect();
    }

    #[must_use]
    pub fn find_problems(
        &self,
        manifest: &GuideManifest,
        dossier: Option<&crate::dossier::Dossier>,
        sections: &[String],
    ) -> Vec<String> {
        let Some(dossier) = dossier else {
            return plan_gates(self, manifest);
        };
        let mut problems = crate::dossier::outline_gates(
            self,
            dossier,
            sections,
            manifest
                .research
                .dossier
                .as_ref()
                .map_or(0, |r| r.min_iconic as usize),
        );
        problems.extend(plan_gates(self, manifest));
        if self.cast.len() != self.characters.len() {
            problems.push(
                "every planned character must be named exactly as in the character bible".into(),
            );
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spliced_evidence_must_quote_the_source_fragment_by_fragment() {
        let source = "Tomar reconstructs the scheme. Later, Vijay meets Savio.";
        assert!(evidence_in_source("", source));
        assert!(evidence_in_source(
            "Tomar reconstructs the scheme. [...] Vijay meets Savio.",
            source
        ));
        assert!(evidence_in_source("reconstructs ... Savio", source));
        assert!(!evidence_in_source(
            "Tomar reconstructs the scheme. [...] Vijay confesses.",
            source
        ));
    }

    #[test]
    fn reuse_needs_an_earlier_fresh_shot_of_the_same_moment_within_the_share() {
        let shot = |unit: usize, setting: &str, reuse: Option<usize>| serde_json::json!({"unit":unit,"shot":"medium","location":"l","action":"a","mood":"m","characters":["Vera"],"focus":[0.5,0.5],"setting":setting,"reuse":reuse});
        let d = |shots: Vec<serde_json::Value>| -> Direction {
            serde_json::from_value(serde_json::json!({"shots":shots})).unwrap()
        };
        let ok = d(vec![
            shot(0, "Hall", None),
            shot(1, "Hall", Some(0)),
            shot(2, "Hall", None),
        ]);
        assert_eq!(reuse_gates(&ok, 0.35), Vec::<String>::new());
        assert_eq!(reuse_gates(&ok, 0.0).len(), 1);
        let moved = d(vec![shot(0, "Hall", None), shot(1, "Beach", Some(0))]);
        assert_eq!(reuse_gates(&moved, 1.0).len(), 1);
        let chained = d(vec![
            shot(0, "Hall", None),
            shot(1, "Hall", Some(0)),
            shot(2, "Hall", Some(1)),
        ]);
        assert_eq!(reuse_gates(&chained, 1.0).len(), 1);
        let forward = d(vec![shot(0, "Hall", Some(1)), shot(1, "Hall", None)]);
        assert_eq!(reuse_gates(&forward, 1.0).len(), 1);
    }

    #[test]
    fn shots_pick_a_sheet_expression_for_every_profiled_character() {
        let mut plan: Plan = serde_json::from_value(serde_json::json!({"storyline":"","characters":["Vera"],"tone":"","open_loop":"","emotional_peak":"","omitted":[],"beats":[]})).unwrap();
        plan.cast = vec![serde_json::from_value(serde_json::json!({"name":"Vera","role":"","introduction":"","want":"","flaw":"","arc":"","turning_points":[],"signature_line":"","signature_object":"","relationships":[],"look":"","emotions":["calm","scared"]})).unwrap()];
        let shot = |poses: serde_json::Value| -> Direction {
            serde_json::from_value(serde_json::json!({"shots":[{"unit":0,"shot":"wide","location":"l","action":"a","mood":"m","characters":["Vera"],"focus":[0.5,0.5],"poses":poses}]})).unwrap()
        };
        assert_eq!(
            pose_gates(
                &shot(serde_json::json!([{"name":"Vera","expression":"Scared"}])),
                &plan
            ),
            Vec::<String>::new()
        );
        assert_eq!(pose_gates(&shot(serde_json::json!([])), &plan).len(), 1);
        assert_eq!(
            pose_gates(
                &shot(serde_json::json!([{"name":"Vera","expression":"joyful"}])),
                &plan
            )
            .len(),
            1
        );
        assert_eq!(pose_gates(&shot(serde_json::json!([{"name":"Vera","expression":"calm"},{"name":"Rex","expression":"calm"}])), &plan).len(), 1);
        let mut placed = shot(serde_json::json!([{"name":"Vera","expression":"calm"}]));
        placed.shots[0].setting = "Hall".into();
        placed.shots[0].continuity = "Night, candles lit, storm outside".into();
        assert_eq!(pose_gates(&placed, &plan).len(), 1);
        plan.settings = vec![crate::dossier::Place {
            name: "Hall".into(),
            description: "Oak-panelled hall".into(),
        }];
        assert_eq!(pose_gates(&placed, &plan), Vec::<String>::new());
        placed.shots[0].props = vec!["Fountain pen".into()];
        assert_eq!(pose_gates(&placed, &plan).len(), 1);
        plan.props = vec!["Fountain pen".into()];
        assert_eq!(pose_gates(&placed, &plan), Vec::<String>::new());
    }
    #[test]
    fn rhythm_reports_each_offending_span_for_targeted_repairs() {
        let guide = retold_guide::catalog::bundled()
            .unwrap()
            .into_iter()
            .find(|g| g.id() == "narrative-fixture")
            .unwrap();
        let mut manifest = guide.manifest().clone();
        manifest
            .writing
            .gates
            .retain(|g| matches!(g, Gate::SentenceLengthVariety { .. }));
        let sentences: Vec<String> = [
            "He stops.",
            "Night falls.",
            "Doors close.",
            "The quiet traveler turns toward the distant house.",
            "Another slow carriage passes along the empty road.",
            "Nobody inside the silent house answers his knock.",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let violations = writing_gates(&[], &sentences.join(" "), &sentences, &manifest);
        assert_eq!(violations.len(), 2);
        assert!(violations[0].message.contains("1–3"));
        assert!(violations[1].message.contains("4–6"));
        assert!(
            violations
                .iter()
                .all(|v| v.message.contains("Word counts:") && v.message.contains("Text:"))
        );
    }

    #[test]
    fn test_dramatic_beat_gates_integration() {
        use retold_dramaturgy::beat::{DramaticBeat, KnowledgeShift};
        let static_beat = vec![DramaticBeat::new(
            "hook",
            "Static filler text.",
            KnowledgeShift::None,
            0,
            0,
        )];
        let violations = dramatic_beat_gates(&static_beat);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].gate, "dramaturgy_delta_e");
    }

    #[test]
    fn test_cadence_gates_integration() {
        let sentences =
            vec!["We must delve into the rich tapestry of history to find truth.".to_string()];
        let violations = cadence_gates(&sentences);
        assert!(!violations.is_empty());
        assert!(violations.iter().any(|v| v.gate == "oral_cadence"));
    }
}
