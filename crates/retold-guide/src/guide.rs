//! A loaded, validated guide: its manifest plus its prose and template files.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use retold_core::{Capability, LanguageTag};

use crate::error::LoadError;
use crate::manifest::{
    Gate, GuideManifest, InputSpec, InputType, InputValue, SCHEMA_VERSION, SourceKind, SourceSpec,
};
use crate::template;

pub const MANIFEST_FILE: &str = "guide.toml";

/// Flags every `retold make` command already has; guide inputs may not reuse them.
pub const RESERVED_INPUTS: &[&str] = &[
    "channel",
    "lang",
    "budget",
    "yes",
    "quality",
    "lean",
    "until",
    "source_file",
    "allow_paid",
    "allow_video",
    "help",
    "version",
    "json",
    "log_json",
    "quiet",
    "verbose",
    "library",
    "offline",
    "no_review",
    "plain",
];

/// Where a guide came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Built into the binary.
    Bundled,
    /// Explicitly supplied text files (not an installed guide).
    Inline,
    /// A folder on disk (user guides and exported copies).
    Folder(PathBuf),
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bundled => f.write_str("built in"),
            Self::Inline => f.write_str("supplied files"),
            Self::Folder(p) => write!(f, "{}", p.display()),
        }
    }
}

/// A guide that passed every check: it can be trusted by the rest of the program.
#[derive(Debug, Clone)]
pub struct Guide {
    manifest: GuideManifest,
    origin: Origin,
    files: BTreeMap<String, String>,
}

impl Guide {
    /// Builds a guide from its text files (paths relative to the guide folder, `/`-separated).
    pub fn from_files(origin: Origin, files: BTreeMap<String, String>) -> Result<Self, LoadError> {
        crate::file_set::validate(&files)?;
        let label = match &origin {
            Origin::Bundled => "built-in guide".to_owned(),
            Origin::Inline => "supplied guide".to_owned(),
            Origin::Folder(p) => p.display().to_string(),
        };
        let file = |name: &str| match &origin {
            Origin::Bundled | Origin::Inline => name.to_owned(),
            Origin::Folder(p) => p.join(name).display().to_string(),
        };
        let src = files.get(MANIFEST_FILE).ok_or(LoadError::Missing {
            origin: label,
            file: MANIFEST_FILE,
        })?;
        let manifest_file = file(MANIFEST_FILE);

        // Check the format version before strict parsing, so a newer guide gets a clear
        // "update Retold" instead of a list of unknown keys.
        if let Ok(raw) = src.parse::<toml::Table>()
            && let Some(found) = raw.get("schema").and_then(toml::Value::as_integer)
        {
            let found = u32::try_from(found).unwrap_or(u32::MAX);
            if found > SCHEMA_VERSION {
                return Err(LoadError::NewerSchema {
                    file: manifest_file,
                    found,
                    supported: SCHEMA_VERSION,
                });
            }
        }
        let manifest: GuideManifest =
            toml::from_str(src).map_err(|e| LoadError::syntax(&manifest_file, src, &e))?;
        let problems = problems(&manifest, &files);
        if !problems.is_empty() {
            return Err(LoadError::invalid(&manifest_file, problems));
        }
        Ok(Self {
            manifest,
            origin,
            files,
        })
    }

    /// Reads a guide folder from disk.
    pub fn from_dir(dir: &Path) -> Result<Self, LoadError> {
        let files = crate::reader::read_guide(dir)?;
        Self::from_files(Origin::Folder(dir.to_owned()), files)
    }

    #[must_use]
    pub const fn manifest(&self) -> &GuideManifest {
        &self.manifest
    }

    #[must_use]
    pub const fn origin(&self) -> &Origin {
        &self.origin
    }

    #[must_use]
    pub fn id(&self) -> &str {
        self.manifest.id.as_str()
    }

    /// A text file of the guide, by relative path.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    /// All files, for snapshots and export.
    #[must_use]
    pub const fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }

    /// The house voice plus the notes for `lang`, if the guide has any.
    #[must_use]
    pub fn voice_for(&self, lang: &LanguageTag) -> String {
        let base_path = &self.manifest.writing.voice;
        let mut voice = self.file(base_path).unwrap_or_default().to_owned();
        let stem = base_path.strip_suffix(".md").unwrap_or(base_path);
        if let Some(notes) = self.file(&format!("{stem}.{lang}.md")) {
            voice.push_str("\n\n");
            voice.push_str(notes);
        }
        voice
    }

    /// Hash of every file; runs record it so a changed guide is noticed on resume.
    #[must_use]
    pub fn content_hash(&self) -> String {
        let mut h = blake3::Hasher::new();
        for (path, text) in &self.files {
            h.update(path.as_bytes());
            h.update(&[0]);
            h.update(text.as_bytes());
            h.update(&[0]);
        }
        h.finalize().to_hex()[..16].to_owned()
    }

    /// Writes the guide's files into `dir` (which must not contain a guide already).
    pub fn write_to(&self, dir: &Path) -> Result<(), LoadError> {
        for (path, text) in &self.files {
            let target = dir.join(path);
            let io = |source| LoadError::Io {
                path: target.display().to_string(),
                source,
            };
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            std::fs::write(&target, text).map_err(io)?;
        }
        Ok(())
    }
}

pub(crate) fn is_text(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("toml" | "md" | "j2" | "txt" | "json")
    )
}

/// Every problem with a parsed manifest. Collects them all, so one `guides check` run shows
/// the full list.
fn problems(manifest: &GuideManifest, files: &BTreeMap<String, String>) -> Vec<String> {
    let mut checker = Checker::new(manifest, files);
    checker.inputs();
    checker.run_folder();
    checker.delivery_and_format();
    checker.length();
    checker.research();
    checker.structure();
    let sections = checker.section_ids();
    checker.writing(&sections);
    checker.visuals();
    checker.edit();
    checker.packaging();
    checker.roles();
    checker.found
}

/// Allowed rounding error when section shares add up to one.
const SHARE_TOLERANCE: f64 = 0.005;
/// Narration speeds a guide may set, in words per second.
const WORDS_PER_SECOND: std::ops::RangeInclusive<f64> = 0.5..=10.0;
/// Valid `visuals.panels` values: frames per image request.
const PANEL_COUNTS: [u32; 3] = [1, 2, 4];
const DELIVERY_ASPECTS: [&str; 3] = ["9:16", "16:9", "1:1"];
const FRAME_RESOLUTIONS: [&str; 2] = ["1K", "2K"];

/// Collects manifest problems, one method per manifest section.
struct Checker<'a> {
    manifest: &'a GuideManifest,
    files: &'a BTreeMap<String, String>,
    input_names: Vec<&'a str>,
    found: Vec<String>,
}

impl<'a> Checker<'a> {
    fn new(manifest: &'a GuideManifest, files: &'a BTreeMap<String, String>) -> Self {
        let input_names = manifest.inputs.keys().map(String::as_str).collect();
        Self {
            manifest,
            files,
            input_names,
            found: Vec::new(),
        }
    }

    fn report(&mut self, problem: impl Into<String>) {
        self.found.push(problem.into());
    }

    fn template(&mut self, at: &str, text: &str) {
        let problems = template::check(at, text, &self.input_names);
        self.found.extend(problems);
    }

    fn range(&mut self, at: &str, [low, high]: [f64; 2]) {
        if !(low.is_finite() && high.is_finite() && low > 0.0 && low <= high) {
            self.report(format!(
                "{at}: [{low}, {high}] must be two positive numbers, low first"
            ));
        }
    }

    fn inputs(&mut self) {
        for (name, spec) in &self.manifest.inputs {
            let at = format!("inputs.{name}");
            self.input_name(&at, name);
            self.input_default(&at, spec);
            self.input_choices(&at, spec);
            self.input_bounds(&at, spec);
        }
    }

    fn input_name(&mut self, at: &str, name: &str) {
        let valid_name = name.starts_with(|c: char| c.is_ascii_lowercase())
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        if !valid_name {
            self.report(format!(
                "{at}: input names use lowercase letters, digits and `_` (`source_url`)"
            ));
        }
        if RESERVED_INPUTS.contains(&name) {
            self.report(format!(
                "{at}: `--{}` is already a Retold flag; pick another name",
                name.replace('_', "-")
            ));
        }
    }

    fn input_default(&mut self, at: &str, spec: &InputSpec) {
        match (spec.kind, &spec.default) {
            (_, None)
            | (InputType::Text | InputType::Choice, Some(InputValue::Text(_)))
            | (InputType::Int, Some(InputValue::Int(_)))
            | (InputType::Bool, Some(InputValue::Bool(_))) => {}
            (kind, Some(default)) => self.report(format!(
                "{at}: default `{default}` does not fit type {kind:?}"
            )),
        }
    }

    fn input_choices(&mut self, at: &str, spec: &InputSpec) {
        let InputSpec {
            kind,
            choices,
            default,
            ..
        } = spec;
        if *kind != InputType::Choice {
            if !choices.is_empty() {
                self.report(format!("{at}: `choices` only applies to type \"choice\""));
            }
            return;
        }
        if choices.is_empty() {
            self.report(format!("{at}: a choice input needs `choices`"));
        }
        if let Some(InputValue::Text(def)) = default
            && !choices.contains(def)
        {
            self.report(format!("{at}: default `{def}` is not one of the choices"));
        }
    }

    fn input_bounds(&mut self, at: &str, spec: &InputSpec) {
        let InputSpec {
            kind,
            min,
            max,
            default,
            ..
        } = spec;
        if *kind != InputType::Int && (min.is_some() || max.is_some()) {
            self.report(format!("{at}: `min`/`max` only apply to type \"int\""));
        }
        if let (Some(low), Some(high)) = (*min, *max)
            && low > high
        {
            self.report(format!("{at}: min {low} is above max {high}"));
        }
        let Some(InputValue::Int(def)) = default else {
            return;
        };
        if let Some(low) = *min
            && *def < low
        {
            self.report(format!("{at}: default `{def}` is below min {low}"));
        }
        if let Some(high) = *max
            && *def > high
        {
            self.report(format!("{at}: default `{def}` is above max {high}"));
        }
    }

    fn run_folder(&mut self) {
        let run = &self.manifest.run;
        if run.folder.is_empty() {
            self.report("run.folder: needs at least one level");
        }
        if run.cast_depth == 0 || run.cast_depth > run.folder.len() {
            self.report(format!(
                "run.cast_depth: must be between 1 and {} (the number of folder levels)",
                run.folder.len()
            ));
        }
        for (index, level) in run.folder.iter().enumerate() {
            self.template(&format!("run.folder[{index}]"), level);
        }
    }

    fn delivery_and_format(&mut self) {
        let manifest = self.manifest;
        let aspects = &manifest.delivery.aspects;
        let unknown = aspects
            .iter()
            .any(|aspect| !DELIVERY_ASPECTS.contains(&aspect.as_str()));
        let distinct = aspects.iter().collect::<HashSet<_>>().len();
        if unknown || distinct != aspects.len() {
            self.report("delivery.aspects: choose distinct 9:16, 16:9 or 1:1 values");
        }
        let format = &manifest.format;
        let [width, height] = format.resolution;
        match parse_aspect(&format.aspect) {
            Some((across, down))
                if u64::from(width) * u64::from(down) == u64::from(height) * u64::from(across) => {}
            Some(_) => self.report(format!(
                "format.resolution: {width}×{height} is not {}",
                format.aspect
            )),
            None => self.report(format!(
                "format.aspect: `{}` should look like \"9:16\"",
                format.aspect
            )),
        }
        self.range("format.target_seconds", format.target_seconds);
        for (language, rate) in &manifest.narration.words_per_second {
            if !rate.is_finite() || !WORDS_PER_SECOND.contains(rate) {
                self.report(format!(
                    "narration.words_per_second.{language}: expected a finite rate from 0.5 to 10"
                ));
            }
        }
        if !manifest.length.contains_key(&format.source_language) {
            self.report(format!(
                "length: no rule for the source language `{}`",
                format.source_language
            ));
        }
    }

    fn length(&mut self) {
        for (language, rule) in &self.manifest.length {
            let at = format!("length.{language}");
            match (rule.words, rule.seconds) {
                (Some([low, high]), None) if low > high => {
                    self.report(format!("{at}.words: {low} is above {high}"));
                }
                (None, Some(seconds)) => self.range(&format!("{at}.seconds"), seconds),
                (Some(_), None) => {}
                _ => self.report(format!("{at}: set exactly one of `words` or `seconds`")),
            }
        }
    }

    fn research(&mut self) {
        let manifest = self.manifest;
        if manifest.research.sources.is_empty() {
            self.report("research.sources: needs at least one source");
        }
        for (index, source) in manifest.research.sources.iter().enumerate() {
            self.source(index, source);
        }
        if manifest.quality.coverage && manifest.research.dossier.is_none() {
            self.report("quality.coverage: needs research.dossier");
        }
    }

    fn source(&mut self, index: usize, source: &'a SourceSpec) {
        let at = format!("research.sources[{index}]");
        let templates = [
            ("query", &source.query),
            ("language", &source.language),
            ("brief", &source.brief),
        ];
        for (field, value) in templates {
            if let Some(text) = value {
                self.template(&format!("{at}.{field}"), text);
            }
        }
        if source.kind == SourceKind::Web && self.manifest.research.dossier.is_none() {
            self.report(format!("{at}: web research needs research.dossier"));
        }
        if let Some(input) = &source.season_from
            && !self.manifest.inputs.contains_key(input)
        {
            self.report(format!(
                "{at}.season_from: `{input}` is not a declared input"
            ));
        }
    }

    /// Checks the sections and reports structural problems.
    fn structure(&mut self) {
        let sections = &self.manifest.structure.sections;
        if sections.is_empty() {
            self.report("structure.sections: needs at least one section");
        }
        let mut ids = HashSet::new();
        for section in sections {
            if !ids.insert(section.id.as_str()) {
                self.report(format!(
                    "structure.sections: `{}` appears twice",
                    section.id
                ));
            }
            if !(section.share > 0.0 && section.share <= 1.0) {
                self.report(format!(
                    "structure.sections.{}: share must be above 0 and at most 1",
                    section.id
                ));
            }
        }
        let total: f64 = sections.iter().map(|section| section.share).sum();
        if !sections.is_empty() && (total - 1.0).abs() > SHARE_TOLERANCE {
            self.report(format!(
                "structure.sections: shares add up to {total:.3}; they must add up to 1"
            ));
        }
    }

    fn section_ids(&self) -> HashSet<&'a str> {
        self.manifest
            .structure
            .sections
            .iter()
            .map(|section| section.id.as_str())
            .collect()
    }

    fn writing(&mut self, sections: &HashSet<&str>) {
        let writing = &self.manifest.writing;
        for path in std::iter::once(&writing.voice).chain(&writing.examples) {
            if !self.files.contains_key(path) {
                self.report(format!("writing: file `{path}` is not in the guide folder"));
            }
        }
        if writing.hook_candidates == 0 {
            self.report("writing.hook_candidates: must be at least 1");
        }
        if writing.drafts.subscription == 0 || writing.drafts.paid == 0 {
            self.report("writing.drafts: both counts must be at least 1");
        }
        let mut gate_ids = HashSet::new();
        for gate in writing
            .gates
            .iter()
            .chain(writing.language_gates.values().flatten())
        {
            let id = gate.id();
            if !gate_ids.insert(id.as_str()) {
                self.report(format!("writing.gates: `{id}` appears twice"));
            }
            self.gate(id.as_str(), gate, sections);
        }
    }

    fn gate(&mut self, id: &str, gate: &Gate, sections: &HashSet<&str>) {
        match gate {
            Gate::Regex {
                pattern, section, ..
            } => {
                if let Err(error) = regex::Regex::new(pattern) {
                    self.report(format!("writing.gates.{id}: bad pattern: {error}"));
                }
                if let Some(section) = section
                    && !sections.contains(section.as_str())
                {
                    self.report(format!(
                        "writing.gates.{id}: section `{section}` does not exist"
                    ));
                }
            }
            Gate::BannedPhrases { list, .. } if list.is_empty() => {
                self.report(format!("writing.gates.{id}: the list is empty"));
            }
            Gate::SentenceLengthVariety {
                max_same_length_run: 0,
                ..
            } => {
                self.report(format!(
                    "writing.gates.{id}: max_same_length_run must be at least 1"
                ));
            }
            _ => {}
        }
    }

    fn visuals(&mut self) {
        let visuals = &self.manifest.visuals;
        self.range("visuals.seconds_per_unit", visuals.seconds_per_unit);
        self.range(
            "visuals.lean_seconds_per_unit",
            visuals.lean_seconds_per_unit,
        );
        self.template("visuals.style", &visuals.style);
        self.parallax();
        let zones = visuals.safe_zones;
        if zones.top < 0.0 || zones.bottom < 0.0 || zones.top + zones.bottom >= 1.0 {
            self.report(
                "visuals.safe_zones: top and bottom must be positive and leave room between them",
            );
        }
        if visuals.shots.is_empty() {
            self.report("visuals.shots: needs at least one shot type");
        }
        if !FRAME_RESOLUTIONS.contains(&visuals.frame_resolution.as_str()) {
            self.report("visuals.frame_resolution: must be \"1K\" or \"2K\"");
        }
        if !PANEL_COUNTS.contains(&visuals.panels) {
            self.report("visuals.panels: must be 1, 2 or 4");
        }
        if !(0.0..=1.0).contains(&visuals.reuse_share) {
            self.report("visuals.reuse_share: must be between 0 and 1");
        }
        if visuals.max_same_shot_run == 0 {
            self.report("visuals.max_same_shot_run: must be at least 1");
        }
    }

    fn parallax(&mut self) {
        let parallax = &self.manifest.visuals.parallax;
        let fraction = |value: f64| value.is_finite() && (0.0..=1.0).contains(&value);
        if !(fraction(parallax.max_share) && fraction(parallax.strength)) {
            self.report(
                "visuals.parallax: max_share and strength must be fractions between zero and one",
            );
        }
        if parallax.hero_fill && !parallax.enabled {
            self.report("visuals.parallax: hero_fill requires enabled = true");
        }
    }

    fn edit(&mut self) {
        let edit = &self.manifest.edit;
        if edit.motions.is_empty() {
            self.report("edit.motions: needs at least one motion");
        }
        let non_negative = |seconds: f64| seconds.is_finite() && seconds >= 0.0;
        if !non_negative(edit.opening.seconds) {
            self.report("edit.opening.seconds: must be a finite non-negative number");
        }
        if !non_negative(edit.ending.seconds) {
            self.report("edit.ending.seconds: must be a finite non-negative number");
        }
        let at_most_zero = |level: f64| level.is_finite() && level <= 0.0;
        if !at_most_zero(edit.music.bed_lufs) {
            self.report("edit.music.bed_lufs: must be a finite negative LUFS value (e.g. -24.0)");
        }
        if !at_most_zero(edit.music.duck_under_voice_db) {
            self.report(
                "edit.music.duck_under_voice_db: must be a finite negative dB value (e.g. -6.0)",
            );
        }
    }

    fn packaging(&mut self) {
        let packaging = &self.manifest.packaging;
        if packaging.platforms.is_empty() {
            self.report("packaging.platforms: needs at least one platform");
        }
        self.template("packaging.rules", &packaging.rules);
        for (language, notice) in &packaging.notices {
            self.template(
                &format!("packaging.notices.{language}.prefix"),
                &notice.prefix,
            );
            self.template(
                &format!("packaging.notices.{language}.suffix"),
                &notice.suffix,
            );
        }
    }

    /// Each role must ask for the kind of capability its job needs.
    fn roles(&mut self) {
        for (role, tier) in self.manifest.roles.all() {
            if tier.capability() != role_capability(role) {
                self.report(format!("roles.{role}: `{tier}` cannot do this job"));
            }
        }
    }
}

fn role_capability(role: &str) -> Capability {
    const ROLE_CAPABILITIES: &[(&str, Capability)] = &[
        ("image_qa", Capability::Vision),
        ("image", Capability::Image),
        ("voice", Capability::Tts),
        ("transcribe", Capability::Transcribe),
    ];
    ROLE_CAPABILITIES
        .iter()
        .find(|(role_name, ..)| *role_name == role)
        .map_or(Capability::Text, |(_, capability_val)| *capability_val)
}

fn parse_aspect(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once(':')?;
    let (a, b) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a > 0 && b > 0).then_some((a, b))
}

pub(crate) fn relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && Path::new(path)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::bundled;

    fn story_files() -> BTreeMap<String, String> {
        bundled()
            .unwrap()
            .into_iter()
            .find(|g| g.id() == "narrative-fixture")
            .unwrap()
            .files()
            .clone()
    }

    fn load_with(edit: impl Fn(&str) -> String) -> Result<Guide, LoadError> {
        let mut files = story_files();
        let toml = edit(&files[MANIFEST_FILE]);
        files.insert(MANIFEST_FILE.to_owned(), toml);
        Guide::from_files(Origin::Bundled, files)
    }

    #[test]
    fn typo_points_at_the_line() {
        let err =
            load_with(|t| t.replace("hook_candidates = 5", "hook_candidate = 5")).unwrap_err();
        let LoadError::Syntax { message, span, .. } = err else {
            panic!("{err:?}")
        };
        assert!(message.contains("hook_candidate"), "{message}");
        assert!(span.is_some());
    }

    #[test]
    fn collects_every_problem() {
        let err = load_with(|t| {
            t.replace("share = 0.09", "share = 0.5")
                .replace("section = \"ending\"", "section = \"finale\"")
                .replace("{{ inputs.title }}{% if", "{{ inputs.titel }}{% if")
                .replace("image_qa = \"vision.fast\"", "image_qa = \"tts\"")
        })
        .unwrap_err();
        let LoadError::Invalid { problems, .. } = err else {
            panic!("{err:?}")
        };
        assert_eq!(problems.len(), 5, "{problems:#?}"); // the typo appears in two templates
    }

    #[test]
    fn newer_format_asks_for_an_update() {
        let err = load_with(|t| t.replacen("schema = 1", "schema = 2", 1)).unwrap_err();
        assert!(matches!(err, LoadError::NewerSchema { found: 2, .. }));
    }

    #[test]
    fn voice_includes_language_notes() {
        let g = Guide::from_files(Origin::Bundled, story_files()).unwrap();
        assert!(
            g.voice_for(&"hi".parse().unwrap())
                .contains("Hindi voice notes")
        );
        assert!(!g.voice_for(&"en".parse().unwrap()).contains("Hindi"));
    }

    #[test]
    fn speech_calibration_is_validated_and_absent_from_legacy_serialization() {
        for rate in ["0", "-1", "nan", "inf", "11"] {
            assert!(load_with(|t| t.replace("hi = 2.75", &format!("hi = {rate}"))).is_err());
        }
        let guide = load_with(|t| t.replace("words_per_second = { hi = 2.75 }\n", "")).unwrap();
        assert!(guide.manifest().narration.words_per_second.is_empty());
        let narration = serde_json::to_value(&guide.manifest().narration).unwrap();
        assert!(narration.get("words_per_second").is_none());
    }

    #[test]
    fn int_default_out_of_bounds_is_rejected() {
        let err = load_with(|t| t.replace("min = 1880", "default = 1800\nmin = 1900")).unwrap_err();
        let LoadError::Invalid { problems, .. } = err else {
            panic!("{err:?}");
        };
        assert!(
            problems
                .iter()
                .any(|p| p.contains("default `1800` is below min 1900"))
        );
    }

    #[test]
    fn packaging_notices_unknown_variable_is_rejected() {
        let err = load_with(|t| {
            let mut s = t.to_owned();
            s.push_str("\n[packaging.notices.en]\nprefix = \"{{ inputs.unknown_var }}\"\nsuffix = \"ok\"\n");
            s
        })
        .unwrap_err();
        let LoadError::Invalid { problems, .. } = err else {
            panic!("{err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("unknown_var")));
    }

    #[test]
    fn visual_and_edit_non_finite_or_out_of_bounds_are_rejected() {
        let err =
            load_with(|t| t.replace("max_same_shot_run = 2", "max_same_shot_run = 0")).unwrap_err();
        let LoadError::Invalid { problems, .. } = err else {
            panic!("{err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("max_same_shot_run")));

        let err = load_with(|t| t.replace("bed_lufs = -30", "bed_lufs = 5.0")).unwrap_err();
        let LoadError::Invalid { problems, .. } = err else {
            panic!("{err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("bed_lufs")));
    }
}

#[cfg(test)]
mod parallax_validation_tests {
    #[test]
    fn invalid_limits_and_paid_fill_without_parallax_are_rejected() {
        for settings in [
            "enabled=true\nmax_share=1.1\nstrength=0.35",
            "enabled=true\nmax_share=0.35\nstrength=nan",
            "enabled=false\nhero_fill=true",
        ] {
            let guide = crate::catalog::bundled().unwrap().remove(0);
            let mut files = guide.files().clone();
            files
                .get_mut("guide.toml")
                .unwrap()
                .push_str("\n[visuals.parallax]\n");
            files.get_mut("guide.toml").unwrap().push_str(settings);
            assert!(super::Guide::from_files(crate::Origin::Inline, files).is_err());
        }
    }
}
