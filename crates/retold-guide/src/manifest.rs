//! The typed form of `guide.toml`. Unknown keys are rejected, so a typo fails at load time
//! instead of being silently ignored.

use indexmap::IndexMap;
use retold_core::{Fps, LanguageTag, Slug, Tier};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The newest manifest format this build understands.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GuideManifest {
    /// Format version of this file.
    pub schema: u32,
    /// Folder-safe id, used on the command line (`retold make narrative-fixture`).
    pub id: Slug,
    /// The guide's own version, recorded in every run (`1.0.0`).
    pub version: String,
    pub name: String,
    /// One line shown in menus.
    pub summary: String,
    /// Questions asked in the wizard; each also becomes a `--flag`. Order is kept.
    #[serde(default)]
    pub inputs: IndexMap<String, InputSpec>,
    pub run: RunFolder,
    pub format: Format,
    #[serde(default, skip_serializing_if = "Delivery::is_empty")]
    pub delivery: Delivery,
    /// Hard length gate per narration language.
    pub length: IndexMap<LanguageTag, LengthRule>,
    pub research: Research,
    pub structure: Structure,
    pub writing: Writing,
    pub quality: Quality,
    pub visuals: Visuals,
    pub narration: Narration,
    pub edit: Edit,
    pub packaging: Packaging,
    pub roles: Roles,
    /// Local bundles required by this guide. Checked offline before production.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_models: Vec<String>,
}

/// Additional local aspect packages. Empty keeps the guide's original single edition.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    #[serde(default)]
    pub aspects: Vec<String>,
}
impl Delivery {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.aspects.is_empty()
    }
}

// ---- Inputs ------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    Text,
    Choice,
    Int,
    Bool,
}

/// A value given for an input, or an input's default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum InputValue {
    Bool(bool),
    Int(i64),
    Text(String),
}

impl std::fmt::Display for InputValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Text(s) => f.write_str(s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputSpec {
    #[serde(rename = "type")]
    pub kind: InputType,
    /// The wizard question.
    pub prompt: String,
    /// Help text for the wizard and `--help`.
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<InputValue>,
    /// Allowed values, for `type = "choice"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// A realistic value, used in `--help` examples.
    #[serde(default)]
    pub example: Option<String>,
    /// Bounds, for `type = "int"`.
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
}

// ---- Run folder ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunFolder {
    /// Folder path of a video under its channel, one template per level. Each rendered part
    /// is turned into a slug; empty parts are dropped.
    pub folder: Vec<String>,
    /// How many leading folder levels own the shared character sheets.
    pub cast_depth: usize,
}

// ---- Format and length --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Format {
    /// `"9:16"`, `"16:9"`, `"1:1"`…
    pub aspect: String,
    /// Width and height in pixels.
    pub resolution: [u32; 2],
    pub fps: Fps,
    pub target_seconds: [f64; 2],
    /// The language the master script is written in.
    pub source_language: LanguageTag,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LengthRule {
    /// Word count range (good for scripts with spaces between words).
    #[serde(default)]
    pub words: Option<[u32; 2]>,
    /// Estimated speech time range, in seconds.
    #[serde(default)]
    pub seconds: Option<[f64; 2]>,
}

// ---- Research -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResearchPolicy {
    /// The script may use only facts from the sources.
    SourceOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Research {
    pub policy: ResearchPolicy,
    pub max_chars: usize,
    pub sources: Vec<SourceSpec>,
    /// Build a cited research dossier (beats, characters, iconic and hidden details) from
    /// every source before planning. Without it, the first source found is used as is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dossier: Option<DossierRules>,
    /// Research a whole franchise or series once (a canon) and slice it per part video.
    /// Needs `dossier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canon: Option<CanonRules>,
}

/// How a guide names a work and its parts, and how part videos see the canon.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CanonRules {
    /// Input holding the franchise, series or book series name.
    pub work_input: String,
    /// Input holding the part: `canon` builds the canon; anything else names a part
    /// (number, title, `Season 2`, or a canon id such as `P03`).
    pub part_input: String,
    /// Input holding the kind of work (`film`, `series`, `book`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind_input: Option<String>,
    /// Input that, when true, adds spin-offs and specials to the canon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spinoffs_input: Option<String>,
    /// Which order decides what a part video may know.
    #[serde(default)]
    pub spoilers: CanonOrder,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CanonOrder {
    /// What a viewer had seen when the part came out.
    #[default]
    Release,
    /// The story's own chronology.
    Story,
}

/// Minimums the dossier must meet before writing starts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DossierRules {
    /// Distinct sources that must contribute facts (a user source file does not count).
    pub min_sources: u32,
    /// Chronological story beats.
    pub min_beats: u32,
    /// Iconic elements and hidden details, each with a source.
    pub min_iconic: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Wikipedia,
    File,
    /// Famous lines, verbatim, from Wikiquote.
    Wikiquote,
    /// The full text of a public-domain book from Project Gutenberg.
    Gutenberg,
    /// A Fandom wiki's article about the work.
    Fandom,
    /// Live web research by a subscription model with search tools; findings carry URLs.
    Web,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
    pub kind: SourceKind,
    /// Search query template.
    #[serde(default)]
    pub query: Option<String>,
    /// Section names to keep, in priority order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    /// Name of the input that selects a season ("Season 2").
    #[serde(default)]
    pub season_from: Option<String>,
    /// Skip silently when unavailable.
    #[serde(default)]
    pub optional: bool,
    /// Character budget for this source; defaults to `research.max_chars`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<usize>,
    /// Wikipedia: keep every section, with `sections` first, instead of only `sections`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub full: bool,
    /// Wikipedia: also read up to this many related articles (adaptations, sequels).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub related: u32,
    /// Wikipedia language edition template (`en`, `ml`, `{{ inputs.language }}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Web: what to look for, as a template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<String>,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if passes a reference"
)]
const fn is_zero(n: &u32) -> bool {
    *n == 0
}

// ---- Structure ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    #[serde(default)]
    pub open_loops: u32,
    #[serde(default)]
    pub emotional_peak: bool,
    pub max_named_characters: u32,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub id: Slug,
    /// Fraction of the script's length; all shares add up to 1.
    pub share: f64,
    /// What this section must do, given to the writer.
    pub purpose: String,
    #[serde(default)]
    pub edit: SectionEdit,
    #[serde(default)]
    pub voice: Option<SectionVoice>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SectionEdit {
    /// Transition into this section.
    #[serde(default)]
    pub transition: Option<Transition>,
    /// Camera motion forced on this section's frames.
    #[serde(default)]
    pub motion: Option<Motion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SectionVoice {
    /// Delivery tag inserted before the section, e.g. `[pause]`.
    pub tag: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    FilmDissolve,
    Whip,
    Flash,
    CrossZoom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    ZoomIn,
    ZoomOut,
    PanLeft,
    PanRight,
    PushFocus,
    Punch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    CameraShake,
    LightLeaks,
}

// ---- Writing ------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tense {
    Present,
    Past,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Person {
    First,
    Second,
    Third,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Writing {
    /// House voice file, relative to the guide folder. `voice.<lang>.md` files next to it
    /// add per-language notes.
    pub voice: String,
    #[serde(default)]
    pub examples: Vec<String>,
    pub tense: Tense,
    pub person: Person,
    /// Hooks generated for the tournament (1 disables it).
    pub hook_candidates: u32,
    pub drafts: Drafts,
    /// Draft one section at a time, each seeing the sections before it, then stitch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub per_section: bool,
    /// What the draft judge scores, one criterion per entry; empty uses the built-in rubric.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub judge_criteria: Vec<String>,
    #[serde(default)]
    pub gates: Vec<Gate>,
    /// Additional checks for a narration language, including native phrase bans.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub language_gates: IndexMap<LanguageTag, Vec<Gate>>,
}

/// Best-of-N full drafts, depending on whether text is free (subscription) or paid.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Drafts {
    pub subscription: u32,
    pub paid: u32,
}

/// A rule checked in code on every script; a failing gate gets targeted fix passes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Gate {
    BannedPhrases {
        id: Slug,
        list: Vec<String>,
    },
    Regex {
        id: Slug,
        pattern: String,
        /// Only check this section.
        #[serde(default)]
        section: Option<Slug>,
        message: String,
    },
    NoRepeatedSentenceStart {
        id: Slug,
    },
    SentenceLengthVariety {
        id: Slug,
        max_same_length_run: u32,
    },
}

impl Gate {
    #[must_use]
    pub const fn id(&self) -> &Slug {
        match self {
            Self::BannedPhrases { id, .. }
            | Self::Regex { id, .. }
            | Self::NoRepeatedSentenceStart { id }
            | Self::SentenceLengthVariety { id, .. } => id,
        }
    }
}

// ---- Quality ------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactCheck {
    Off,
    Fast,
    Smart,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Quality {
    pub max_fix_passes: u32,
    pub fact_check: FactCheck,
    /// Check that every planned beat and iconic element made it into the script, and that
    /// every named character is introduced. Needs `research.dossier`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub coverage: bool,
    pub image_qa: bool,
}

// ---- Visuals ------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Visuals {
    /// Speech time per image.
    pub seconds_per_unit: [f64; 2],
    /// Speech time per image with `--lean`.
    pub lean_seconds_per_unit: [f64; 2],
    /// Style block template added to every image prompt.
    pub style: String,
    pub safe_zones: SafeZones,
    pub shots: Vec<String>,
    pub max_same_shot_run: u32,
    pub characters: CharacterRules,
    #[serde(default, skip_serializing_if = "Parallax::is_disabled")]
    pub parallax: Parallax,
    #[serde(default, skip_serializing_if = "LipSync::is_disabled")]
    pub lipsync: LipSync,
    /// Most shots that may recut an earlier frame (same moment, tighter framing) instead of
    /// generating a new image. 0 disables reuse.
    #[serde(
        default = "default_reuse_share",
        skip_serializing_if = "is_default_reuse_share"
    )]
    pub reuse_share: f64,
    /// Resolution of scene frames ("1K" or "2K"). References (sheets, places, props, style)
    /// are always 2K because every frame copies them; frames may be cheaper at 1K.
    #[serde(
        default = "default_frame_resolution",
        skip_serializing_if = "is_default_frame_resolution"
    )]
    pub frame_resolution: String,
    /// Supporting shots of one scene drawn per generated image (1, 2 or 4) and split along
    /// white gutters; hero shots are always drawn alone. 1 (the default) disables it.
    #[serde(default = "one_panel", skip_serializing_if = "is_one_panel")]
    pub panels: u32,
}

const fn one_panel() -> u32 {
    1
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if passes a reference"
)]
const fn is_one_panel(value: &u32) -> bool {
    *value == 1
}

fn default_frame_resolution() -> String {
    "2K".into()
}

fn is_default_frame_resolution(value: &String) -> bool {
    value == "2K"
}

const fn default_reuse_share() -> f64 {
    0.35
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    clippy::float_cmp,
    reason = "serde skip_serializing_if passes a reference; the default is an exact literal"
)]
fn is_default_reuse_share(value: &f64) -> bool {
    *value == default_reuse_share()
}

/// Fractions of frame height kept clear of faces (title on top, captions below).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SafeZones {
    pub top: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Originality {
    /// Never imitate actors or iconic costumes.
    Strict,
    Loose,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterRules {
    pub reference_sheet: bool,
    pub originality: Originality,
    /// Pause for approval after the sheets (skipped with `--yes`).
    pub review: bool,
}

// ---- Narration, edit, packaging -----------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NarrationMode {
    /// One take for the whole script, then word timings.
    Continuous,
    /// Validated speaker/emotion cues, synthesized and aligned one turn at a time.
    Dialogue,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Narration {
    pub mode: NarrationMode,
    /// Measured speaking rates, saved with the guide so calibration cannot change old runs.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub words_per_second: IndexMap<LanguageTag, f64>,
    /// Delivery direction per language.
    #[serde(default)]
    pub direction: IndexMap<LanguageTag, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    /// Camera motions to choose from.
    pub motions: Vec<Motion>,
    pub opening: Opening,
    pub ending: Ending,
    pub music: Music,
    pub sfx_on_transitions: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    CoverTitle,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub kind: OpeningKind,
    pub seconds: f64,
    #[serde(default)]
    pub effect: Option<Effect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EndingKind {
    HoldFade,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ending {
    pub kind: EndingKind,
    pub seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Music {
    /// Where the mood comes from (`plan.tone`).
    pub mood_from: String,
    /// Music loudness under narration, LUFS.
    pub bed_lufs: f64,
    /// Extra reduction while someone speaks, dB (negative).
    pub duck_under_voice_db: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Youtube,
    Tiktok,
    Instagram,
    Facebook,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Packaging {
    pub platforms: Vec<Platform>,
    /// Template for the packaging writer.
    pub rules: String,
    #[serde(
        default = "default_title_max",
        skip_serializing_if = "is_default_title_max"
    )]
    pub title_max_chars: usize,
    #[serde(
        default = "default_hashtags",
        skip_serializing_if = "is_default_hashtags"
    )]
    pub hashtag_count: usize,
    /// Optional per-language prefix/suffix templates; no content notices are forced by the engine.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub notices: IndexMap<LanguageTag, PostNotices>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PostNotices {
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
}
const fn default_title_max() -> usize {
    69
}
const fn default_hashtags() -> usize {
    5
}
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if passes a reference"
)]
const fn is_default_title_max(value: &usize) -> bool {
    *value == 69
}
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if passes a reference"
)]
const fn is_default_hashtags(value: &usize) -> bool {
    *value == 5
}

/// Which capability tier each stage uses.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Roles {
    pub plan: Tier,
    pub write: Tier,
    pub transcreate: Tier,
    pub judge: Tier,
    pub check: Tier,
    pub direct: Tier,
    pub design: Tier,
    pub package: Tier,
    pub image_qa: Tier,
    pub image: Tier,
    pub voice: Tier,
    pub transcribe: Tier,
}

impl Roles {
    /// Every role with its name, for validation and display.
    #[must_use]
    pub const fn all(&self) -> [(&'static str, Tier); 12] {
        [
            ("plan", self.plan),
            ("write", self.write),
            ("transcreate", self.transcreate),
            ("judge", self.judge),
            ("check", self.check),
            ("direct", self.direct),
            ("design", self.design),
            ("package", self.package),
            ("image_qa", self.image_qa),
            ("image", self.image),
            ("voice", self.voice),
            ("transcribe", self.transcribe),
        ]
    }
}

/// Mouth and blink animation for on-screen dialogue speakers; explicitly opt-in.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LipSync {
    pub enabled: bool,
}
impl LipSync {
    #[must_use]
    pub const fn is_disabled(&self) -> bool {
        !self.enabled
    }
}

/// Local parallax is explicitly opt-in; paid background plates have separate permission.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Parallax {
    pub enabled: bool,
    pub max_share: f64,
    pub strength: f64,
    pub hero_fill: bool,
}
impl Default for Parallax {
    fn default() -> Self {
        Self {
            enabled: false,
            max_share: 0.35,
            strength: 0.35,
            hero_fill: false,
        }
    }
}
impl Parallax {
    #[must_use]
    pub const fn is_disabled(&self) -> bool {
        !self.enabled
            && !self.hero_fill
            && self.max_share.to_bits() == 0.35_f64.to_bits()
            && self.strength.to_bits() == 0.35_f64.to_bits()
    }
}
