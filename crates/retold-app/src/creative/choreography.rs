//! Cinematic storyboard choreography, camera motion direction, and audio soundscape scoring.

use std::collections::{BTreeMap, HashSet};

use retold_engine::story::{
    Delivery, Direction, Pace, ParallaxMove, Plan, SceneSound, Score, Shot, ShotMotion, Tone,
};

use super::types::{
    PaceCount, ShotDirectionItem, SoundUnitItem, SoundscapeAuditReport, StoryboardAuditReport,
};

pub const DEFAULT_CAMERA_PACKAGE: &str = "Panavision C-Series 35mm f/2.0 anamorphic";
pub const STATIC_MOTION_LABEL: &str = "static lock-off";
pub const DEFAULT_MAX_ITEMS: usize = 100_000;

fn describe_parallax_move(move_type: Option<ParallaxMove>) -> String {
    match move_type {
        Some(ParallaxMove::PushIn) => "parallax push-in".to_string(),
        Some(ParallaxMove::Orbit) => "parallax orbit".to_string(),
        Some(ParallaxMove::Pedestal) => "parallax pedestal".to_string(),
        Some(ParallaxMove::DollyZoom) => "parallax vertigo dolly-zoom".to_string(),
        None => "parallax depth-shift".to_string(),
    }
}

fn describe_shot_motion(shot: &Shot) -> String {
    match shot.motion {
        Some(ShotMotion::Parallax) => describe_parallax_move(shot.parallax_move),
        None => STATIC_MOTION_LABEL.to_string(),
    }
}

fn map_single_shot(shot: &Shot) -> ShotDirectionItem {
    let motion_type = describe_shot_motion(shot);
    ShotDirectionItem {
        unit: shot.unit,
        shot: shot.shot.clone(),
        location: shot.location.clone(),
        action: shot.action.clone(),
        mood: shot.mood.clone(),
        characters: shot.characters.clone(),
        camera_package: DEFAULT_CAMERA_PACKAGE.to_string(),
        motion_type,
        focus_x: shot.focus[0],
        focus_y: shot.focus[1],
    }
}

fn map_shot_items(shots: &[Shot]) -> Vec<ShotDirectionItem> {
    shots.iter().map(map_single_shot).collect()
}

fn count_dynamic_motion_shots(shots: &[Shot]) -> usize {
    shots
        .iter()
        .filter(|shot| shot.motion.is_some() || shot.parallax_move.is_some())
        .count()
}

const fn compute_static_difference(total: usize, dynamic: usize) -> usize {
    total.saturating_sub(dynamic)
}

fn calculate_coverage_percentage(dynamic: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let bounded = total.min(DEFAULT_MAX_ITEMS);
    #[expect(
        clippy::cast_precision_loss,
        reason = "bounded shot count within integer precision"
    )]
    let total_float = bounded as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "bounded dynamic count within integer precision"
    )]
    let dynamic_float = dynamic as f64;
    (dynamic_float / total_float) * 100.0
}

fn collect_unique_locations(shots: &[Shot]) -> Vec<String> {
    let mut location_set = HashSet::new();
    let mut unique_list = Vec::new();
    for shot in shots {
        let is_new_location = location_set.insert(shot.location.clone());
        if is_new_location {
            unique_list.push(shot.location.clone());
        }
    }
    unique_list
}

fn build_storyboard_recommendations(static_shots: usize, total_shots: usize) -> Vec<String> {
    let mut recommendations = Vec::new();
    let static_threshold = total_shots / 2;
    let is_mostly_static = static_shots > static_threshold;
    if is_mostly_static {
        recommendations.push(
            "Over 50% static shots detected. Recommend adding 2.5D parallax push-in on dramatic reveals."
                .to_string(),
        );
    }
    let has_few_shots = total_shots < 2;
    if has_few_shots {
        recommendations.push(
            "Storyboard requires at least two distinct camera setups for cinematic rhythm."
                .to_string(),
        );
    }
    recommendations
}

const fn evaluate_storyboard_gates(total_shots: usize, recommendations: &[String]) -> bool {
    total_shots > 0 && recommendations.is_empty()
}

#[must_use]
pub fn audit_storyboard(direction: &Direction, _plan: &Plan) -> StoryboardAuditReport {
    let shots = map_shot_items(&direction.shots);
    let total_shots = direction.shots.len();
    let dynamic_motion_shots = count_dynamic_motion_shots(&direction.shots);
    let static_shots = compute_static_difference(total_shots, dynamic_motion_shots);
    let parallax_coverage_pct = calculate_coverage_percentage(dynamic_motion_shots, total_shots);
    let locations = collect_unique_locations(&direction.shots);
    let recommendations = build_storyboard_recommendations(static_shots, total_shots);
    let passes_gates = evaluate_storyboard_gates(total_shots, &recommendations);

    StoryboardAuditReport {
        total_shots,
        dynamic_motion_shots,
        static_shots,
        parallax_coverage_pct,
        locations,
        recommendations,
        passes_gates,
        shots,
    }
}

const fn describe_pace(pace: Pace) -> &'static str {
    match pace {
        Pace::Normal => "normal",
        Pace::Slow => "slow",
        Pace::Fast => "fast",
    }
}

const fn describe_extended_tone(tone: Tone) -> &'static str {
    match tone {
        Tone::Dark => "dark",
        Tone::Whisper => "whisper",
        Tone::Excited => "excited",
        _ => "neutral",
    }
}

const fn describe_tone(tone: Tone) -> &'static str {
    match tone {
        Tone::Neutral => "neutral",
        Tone::Warm => "warm",
        Tone::Tense => "tense",
        _ => describe_extended_tone(tone),
    }
}

#[derive(Clone, Copy)]
struct SoundInput<'a> {
    sound: Option<&'a SceneSound>,
    score: Option<&'a Score>,
    delivery: Option<&'a Delivery>,
}

fn extract_audio_delivery(delivery: Option<&Delivery>) -> (String, String) {
    let pace = delivery.map_or_else(
        || "normal".to_string(),
        |performance| describe_pace(performance.pace).to_string(),
    );
    let tone = delivery.map_or_else(
        || "neutral".to_string(),
        |performance| describe_tone(performance.tone).to_string(),
    );
    (pace, tone)
}

fn map_single_sound_unit(unit: usize, input: SoundInput<'_>) -> SoundUnitItem {
    let ambience = input
        .sound
        .map_or_else(String::new, |scene_sound| scene_sound.ambience.clone());
    let effect = input
        .sound
        .map_or_else(String::new, |scene_sound| scene_sound.effect.clone());
    let mood = input
        .score
        .map_or_else(String::new, |music_score| music_score.mood.clone());
    let sting = input.score.is_some_and(|music_score| music_score.sting);
    let silence = input.score.is_some_and(|music_score| music_score.silence);
    let (pace, tone) = extract_audio_delivery(input.delivery);

    SoundUnitItem {
        unit,
        ambience,
        effect,
        mood,
        sting,
        silence,
        pace,
        tone,
    }
}

#[derive(Clone, Copy)]
struct SoundContext<'a> {
    sounds: &'a [SceneSound],
    scores: &'a [Score],
    deliveries: &'a [Delivery],
}

impl<'a> SoundContext<'a> {
    fn build_sound_input(self, index: usize) -> SoundInput<'a> {
        SoundInput {
            sound: self.sounds.get(index),
            score: self.scores.get(index),
            delivery: self.deliveries.get(index),
        }
    }

    const fn max_units(self) -> usize {
        let first = self.sounds.len();
        let second = self.scores.len();
        let third = self.deliveries.len();
        let intermediate = if first > second { first } else { second };
        if intermediate > third {
            intermediate
        } else {
            third
        }
    }
}

fn map_all_sound_units(context: SoundContext<'_>) -> Vec<SoundUnitItem> {
    let max_len = context.max_units();
    let mut items = Vec::with_capacity(max_len);
    for index in 0..max_len {
        let input = context.build_sound_input(index);
        items.push(map_single_sound_unit(index, input));
    }
    items
}

fn count_stings(scores: &[Score]) -> usize {
    scores.iter().filter(|score| score.sting).count()
}

fn count_silences(scores: &[Score]) -> usize {
    scores.iter().filter(|score| score.silence).count()
}

fn compute_pace_counts(deliveries: &[Delivery]) -> Vec<PaceCount> {
    let mut tally: BTreeMap<&'static str, usize> = BTreeMap::new();
    for delivery in deliveries {
        let name = describe_pace(delivery.pace);
        *tally.entry(name).or_insert(0) += 1;
    }
    tally
        .into_iter()
        .map(|(pace, count)| PaceCount {
            pace: pace.to_string(),
            count,
        })
        .collect()
}

fn find_dominant_mood(scores: &[Score]) -> String {
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for score in scores {
        let has_mood_text = !score.mood.is_empty();
        if has_mood_text {
            *tally.entry(score.mood.as_str()).or_insert(0) += 1;
        }
    }
    tally
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map_or_else(|| "dramatic".to_string(), |(mood, _)| mood.to_string())
}

#[must_use]
pub fn audit_soundscape(
    sounds: &[SceneSound],
    scores: &[Score],
    deliveries: &[Delivery],
) -> SoundscapeAuditReport {
    let context = SoundContext {
        sounds,
        scores,
        deliveries,
    };
    let units = map_all_sound_units(context);
    let total_units = units.len();
    let stings_count = count_stings(scores);
    let silence_moments = count_silences(scores);
    let dominant_mood = find_dominant_mood(scores);
    let pace_distribution = compute_pace_counts(deliveries);
    let has_units = total_units > 0;
    let has_mood = !dominant_mood.is_empty();
    let passes_gates = has_units && has_mood;

    SoundscapeAuditReport {
        total_units,
        stings_count,
        silence_moments,
        dominant_mood,
        pace_distribution,
        passes_gates,
        units,
    }
}

struct BenchmarkShotSpec<'a> {
    unit: usize,
    shot: &'a str,
    location: &'a str,
    action: &'a str,
    mood: &'a str,
    characters: Vec<String>,
    focus: [f64; 2],
    parallax_move: Option<ParallaxMove>,
}

fn make_benchmark_shot(spec: BenchmarkShotSpec<'_>) -> Shot {
    Shot {
        unit: spec.unit,
        shot: spec.shot.into(),
        location: spec.location.into(),
        action: spec.action.into(),
        mood: spec.mood.into(),
        characters: spec.characters,
        focus: spec.focus,
        motion: Some(ShotMotion::Parallax),
        parallax_move: spec.parallax_move,
        hero: false,
        poses: Vec::new(),
        setting: String::new(),
        continuity: String::new(),
        reuse: None,
        sound: None,
        score: None,
        delivery: None,
        props: Vec::new(),
    }
}

fn benchmark_early_shots() -> Vec<Shot> {
    vec![
        make_benchmark_shot(BenchmarkShotSpec {
            unit: 0,
            shot: "Wide establishing of the sun-drenched Catalan tavern banquet.",
            location: "Marseille Port Tavern",
            action: "Royal gendarmes kick open the timber doors with fixed bayonets.",
            mood: "ominous shock",
            characters: vec!["Edmond Dantes".into(), "Fernand Mondego".into()],
            focus: [0.5, 0.5],
            parallax_move: Some(ParallaxMove::PushIn),
        }),
        make_benchmark_shot(BenchmarkShotSpec {
            unit: 1,
            shot: "Low-angle tracking shot of Dantes descending the damp stone steps.",
            location: "Chateau d'If Dungeon",
            action: "Heavy iron door slams shut, locking out the sunlight forever.",
            mood: "grim despair",
            characters: vec!["Edmond Dantes".into()],
            focus: [0.45, 0.6],
            parallax_move: Some(ParallaxMove::Orbit),
        }),
    ]
}

fn benchmark_late_shots() -> Vec<Shot> {
    vec![
        make_benchmark_shot(BenchmarkShotSpec {
            unit: 2,
            shot: "Extreme close-up macro of the flame illuminating invisible ink.",
            location: "Subterranean Cell 34",
            action: "Abbe Faria holds the Spada parchment above the trembling wick.",
            mood: "revelation",
            characters: vec!["Abbe Faria".into(), "Edmond Dantes".into()],
            focus: [0.5, 0.4],
            parallax_move: Some(ParallaxMove::PushIn),
        }),
        make_benchmark_shot(BenchmarkShotSpec {
            unit: 3,
            shot: "Dolly-zoom on the edge of the granite cliff over crashing waves.",
            location: "Chateau d'If Ramparts",
            action: "The burial shroud sack plunges two hundred feet into dark waters.",
            mood: "visceral climax",
            characters: vec!["Edmond Dantes".into()],
            focus: [0.5, 0.7],
            parallax_move: Some(ParallaxMove::DollyZoom),
        }),
    ]
}

#[must_use]
pub fn benchmark_direction() -> (Direction, Plan) {
    let mut shots = benchmark_early_shots();
    shots.extend(benchmark_late_shots());
    let direction = Direction { shots };
    let plan = super::dossier::benchmark_plan_outline();
    (direction, plan)
}

fn benchmark_sounds() -> Vec<SceneSound> {
    vec![
        SceneSound {
            ambience: "wind whistling through prison iron bars and distant crashing waves".into(),
            effect: "heavy iron lock latch clank".into(),
        },
        SceneSound {
            ambience: "flickering torch flame hiss in damp subterranean limestone".into(),
            effect: "parchment paper crisp unfolding".into(),
        },
        SceneSound {
            ambience: "howling storm gusts on midnight granite ramparts".into(),
            effect: "violent sea plunge splash".into(),
        },
    ]
}

fn benchmark_scores() -> Vec<Score> {
    vec![
        Score {
            mood: "dark".into(),
            sting: true,
            silence: false,
        },
        Score {
            mood: "mysterious".into(),
            sting: false,
            silence: false,
        },
        Score {
            mood: "epic".into(),
            sting: true,
            silence: false,
        },
    ]
}

fn benchmark_deliveries() -> Vec<Delivery> {
    vec![
        Delivery {
            pace: Pace::Fast,
            tone: Tone::Tense,
            pause_before: 0.2,
        },
        Delivery {
            pace: Pace::Slow,
            tone: Tone::Dark,
            pause_before: 0.4,
        },
        Delivery {
            pace: Pace::Normal,
            tone: Tone::Whisper,
            pause_before: 0.1,
        },
    ]
}

#[must_use]
pub fn benchmark_soundscape() -> (Vec<SceneSound>, Vec<Score>, Vec<Delivery>) {
    (
        benchmark_sounds(),
        benchmark_scores(),
        benchmark_deliveries(),
    )
}
