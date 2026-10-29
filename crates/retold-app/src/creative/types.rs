//! Types and structures for creative optimization and prompt engineering.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DEFAULT_IMAGE_COST_MICROS: i64 = 40_000;
pub const PARALLAX_COST_MICROS: i64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelTarget {
    #[default]
    Flux,
    Midjourney,
    Sdxl,
    Recraft,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CinematicLighting {
    #[default]
    Rembrandt,
    VolumetricGodRays,
    GoldenHour,
    Chiaroscuro,
    NeonNoir,
    SoftDiffused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CameraOptics {
    #[default]
    Anamorphic35mm,
    Prime50mm,
    WideAngle24mm,
    Telephoto85mm,
    Macro100mm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorGrading {
    #[default]
    KodakPortra400,
    TechnicolorVibrant,
    MoodyDesaturated,
    TealAndOrange,
    BleachBypass,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PromptEnhanceOptions {
    pub target: ModelTarget,
    pub lighting: CinematicLighting,
    pub optics: CameraOptics,
    pub color_grade: ColorGrading,
}

impl Default for PromptEnhanceOptions {
    fn default() -> Self {
        Self {
            target: ModelTarget::Flux,
            lighting: CinematicLighting::Rembrandt,
            optics: CameraOptics::Anamorphic35mm,
            color_grade: ColorGrading::KodakPortra400,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EnhancedPrompt {
    pub target_model: ModelTarget,
    pub positive_prompt: String,
    pub negative_prompt: String,
    pub estimated_tokens: usize,
    pub cinematic_enhancements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationStrategy {
    FreshGeneration,
    FrameReuse { source_shot: usize },
    ParallaxMotion { motion_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShotOptimization {
    pub shot_index: usize,
    pub strategy: OptimizationStrategy,
    pub original_cost_micros: i64,
    pub optimized_cost_micros: i64,
    pub savings_micros: i64,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CostOptimizationSummary {
    pub total_shots: usize,
    pub fresh_generations: usize,
    pub reused_frames: usize,
    pub parallax_upgrades: usize,
    pub original_cost_usd: String,
    pub optimized_cost_usd: String,
    pub savings_usd: String,
    pub savings_percentage: f64,
    pub optimizations: Vec<ShotOptimization>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DramaturgyAuditReport {
    pub script_score: u32,
    pub total_words: usize,
    pub sentence_count: usize,
    pub words_per_sentence_avg: f64,
    pub sensory_verb_count: usize,
    pub sensory_verb_density_pct: f64,
    pub banned_cliches_found: Vec<String>,
    pub rhythm_warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BeatArcReport {
    pub total_beats: usize,
    pub average_delta_e: f64,
    pub peak_delta_e: f64,
    pub peak_index: usize,
    pub has_inversion: bool,
    pub passes_gates: bool,
    pub static_beats: Vec<StaticBeatWarning>,
    pub beat_evaluations: Vec<BeatEvaluation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StaticBeatWarning {
    pub index: usize,
    pub section: String,
    pub delta_e: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BeatEvaluation {
    pub index: usize,
    pub section: String,
    pub text: String,
    pub knowledge_shift: String,
    pub power_delta: i8,
    pub danger_delta: i8,
    pub delta_e: f64,
    pub is_climax: bool,
    pub sparkline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DossierAuditReport {
    pub work_title: String,
    pub work_medium: String,
    pub work_year: String,
    pub version_note: String,
    pub logline: String,
    pub total_beats: usize,
    pub must_beats: usize,
    pub should_beats: usize,
    pub could_beats: usize,
    pub iconic_count: usize,
    pub settings_count: usize,
    pub themes: Vec<String>,
    pub conflicts: Vec<String>,
    pub passes_gates: bool,
    pub gate_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BibleCharacterItem {
    pub name: String,
    pub role: String,
    pub introduction: String,
    pub want: String,
    pub flaw: String,
    pub arc: String,
    pub look: String,
    pub emotions: Vec<String>,
    pub signature_prop: String,
    pub signature_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BibleAuditReport {
    pub total_characters: usize,
    pub characters: Vec<BibleCharacterItem>,
    pub passes_gates: bool,
    pub gate_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScriptCoverageAuditReport {
    pub total_outline_beats: usize,
    pub covered_beats: usize,
    pub missing_beats: Vec<String>,
    pub covered_iconic: usize,
    pub missing_iconic: Vec<String>,
    pub unintroduced_characters: Vec<String>,
    pub foreign_elements: Vec<String>,
    pub passes_gates: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShotDirectionItem {
    pub unit: usize,
    pub shot: String,
    pub location: String,
    pub action: String,
    pub mood: String,
    pub characters: Vec<String>,
    pub camera_package: String,
    pub motion_type: String,
    pub focus_x: f64,
    pub focus_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StoryboardAuditReport {
    pub total_shots: usize,
    pub dynamic_motion_shots: usize,
    pub static_shots: usize,
    pub parallax_coverage_pct: f64,
    pub locations: Vec<String>,
    pub recommendations: Vec<String>,
    pub passes_gates: bool,
    pub shots: Vec<ShotDirectionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SoundUnitItem {
    pub unit: usize,
    pub ambience: String,
    pub effect: String,
    pub mood: String,
    pub sting: bool,
    pub silence: bool,
    pub pace: String,
    pub tone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaceCount {
    pub pace: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SoundscapeAuditReport {
    pub total_units: usize,
    pub stings_count: usize,
    pub silence_moments: usize,
    pub dominant_mood: String,
    pub pace_distribution: Vec<PaceCount>,
    pub passes_gates: bool,
    pub units: Vec<SoundUnitItem>,
}
