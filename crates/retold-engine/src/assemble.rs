//! Pure timeline assembly: no editor launch or video generation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use retold_core::{Frames, Seconds, WritingScript};
use retold_filmcraft::timeline::{
    Animation, Clip, Easing, Effect, Highlight, Key, Source, Timeline, Transition, crop_position,
};
use retold_guide::manifest::Motion;
use retold_lang::fit::{Fonts, Measure};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::assets::{Images, Storyboard};
use crate::context::Context;
use crate::languages::{Edition, edition_id};
use crate::media_runtime::MediaError;
use crate::{EngineError, Stage, fsx};
use retold_core::num;

pub fn assemble(
    context: &Context,
    board: &Storyboard,
    images: &Images,
    edition: &Edition,
) -> Result<Timeline, MediaError> {
    let fonts = NativeFonts(Fonts::system());
    assemble_with_fonts(context, board, images, edition, &fonts)
}

pub trait FontResolver: Send + Sync + std::fmt::Debug {
    fn face(&self, spec: &retold_guide::channel::FontSpec) -> Result<Box<dyn Measure>, MediaError>;
}
#[derive(Debug)]
pub struct NativeFonts(pub Fonts);
impl FontResolver for NativeFonts {
    fn face(&self, spec: &retold_guide::channel::FontSpec) -> Result<Box<dyn Measure>, MediaError> {
        Ok(Box::new(
            self.0
                .face(&spec.family, &spec.style)
                .map_err(|e| MediaError::Quality(e.to_string()))?,
        ))
    }
}

pub fn assemble_with_fonts(
    context: &Context,
    board: &Storyboard,
    images: &Images,
    edition: &Edition,
    fonts: &dyn FontResolver,
) -> Result<Timeline, MediaError> {
    assemble_named_with_fonts(
        context,
        board,
        images,
        edition,
        fonts,
        &edition_id(&edition.variant),
    )
}

pub fn assemble_named_with_fonts(
    context: &Context,
    board: &Storyboard,
    images: &Images,
    edition: &Edition,
    fonts: &dyn FontResolver,
    name: &str,
) -> Result<Timeline, MediaError> {
    let m = context.guide.manifest();
    let fps = m.format.fps;
    let frames = |s: f64| {
        Seconds::new(s)
            .map(|s| s.to_frames(fps))
            .ok_or_else(|| MediaError::Quality("non-finite timeline time".into()))
    };
    let audio_end = frames(edition.alignment.duration)?;
    let duration = Frames(audio_end.0 + frames(m.edit.ending.seconds)?.0);
    if images.frames.len() != board.units.len()
        || edition.alignment.units.len() != board.units.len()
    {
        return Err(MediaError::Quality(
            "assets do not cover every storyboard unit".into(),
        ));
    }
    let mut timeline = Timeline {
        name: name.to_owned(),
        width: m.format.resolution[0],
        height: m.format.resolution[1],
        fps,
        duration,
        clips: Vec::new(),
        video_tracks: None,
        audio_tracks: None,
    };
    scene_clips(context, board, images, edition, &mut timeline)?;
    if m.narration.mode == retold_guide::manifest::NarrationMode::Dialogue
        || crate::voices::local_narrator(context, &edition.variant.narration)
    {
        let scene: crate::voices::Scene = serde_json::from_slice(
            &std::fs::read(crate::voices::file(context, &edition.variant.narration))
                .map_err(|e| EngineError::Setup(e.to_string()))?,
        )
        .map_err(|e| EngineError::Setup(e.to_string()))?;
        if let Some(room) = &scene.room_tone {
            room.verify()?;
            for turn in &scene.turns {
                timeline.clips.push(Clip {
                    track: "A3".into(),
                    start: frames(turn.end)?,
                    duration: frames(0.18)?,
                    source: Source::Media {
                        path: room.path.clone(),
                    },
                    effects: vec![],
                    transition: None,
                    audio_gain_db: Some(0.0),
                });
            }
        }
        for turn in scene.turns {
            turn.audio.verify()?;
            timeline.clips.push(Clip {
                track: turn.track,
                start: frames(turn.start)?,
                duration: frames(turn.end - turn.start)?,
                source: Source::Media {
                    path: turn.audio.path,
                },
                effects: vec![],
                transition: None,
                audio_gain_db: Some(0.0),
            });
        }
    } else {
        timeline.clips.push(Clip {
            track: "A1".into(),
            start: Frames(0),
            duration: audio_end,
            source: Source::Media {
                path: edition.audio.path.clone(),
            },
            effects: vec![],
            transition: None,
            audio_gain_db: Some(0.0),
        });
    }
    opening(context, images, &mut timeline, fonts)?;
    let keywords = caption_keywords(board);
    captions(context, edition, &keywords, &mut timeline, fonts)?;
    look(context, &mut timeline);
    sound_layers(context, board, edition, &mut timeline)?;
    timeline.validate().map_err(MediaError::Quality)?;
    let path = context
        .layout
        .stage(Stage::Edit)
        .join(format!("{}.timeline.json", timeline.name));
    fsx::write_json_atomic(&path, &timeline)?;
    Ok(timeline)
}

// Auto Reframe adds its own lateral tracking, including on a centered plate.
// Explicit native Motion keeps the supplied focus centered without a second pan.
fn native_reframe(plate: &crate::delivery_plates::Plate, clip: &mut Clip, duration: Frames) {
    let position = |zoom: f64| -> [f64; 2] {
        std::array::from_fn(|i| f64::from(plate.canvas[i]) / 2.0 - plate.offset[i] * zoom / 100.0)
    };
    let mut motion = effect_values(
        "motion",
        &[
            ("scale", json!(plate.zoom)),
            ("position", json!(position(plate.zoom))),
        ],
    );
    if !plate.moving && duration.0 > 1 {
        for (param, start, end) in [
            ("scale", json!(plate.zoom), json!(plate.zoom * 1.05)),
            (
                "position",
                json!(position(plate.zoom)),
                json!(position(plate.zoom * 1.05)),
            ),
        ] {
            motion.animations.push(Animation {
                param: param.into(),
                keys: vec![
                    key(0, start, Easing::EaseOut),
                    key(duration.0 - 1, end, Easing::EaseIn),
                ],
            });
        }
    }
    clip.effects = vec![motion];
}

fn load_plates(
    context: &Context,
    name: &str,
) -> Result<BTreeMap<usize, crate::delivery_plates::Plate>, MediaError> {
    if context.guide.manifest().delivery.aspects.is_empty() {
        return Ok(BTreeMap::new());
    }
    let path = crate::delivery_plates::file(context, name);
    serde_json::from_slice(&std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?)
        .map_err(|e| EngineError::Setup(e.to_string()).into())
}

fn load_shot_bindings(
    context: &Context,
    edition: &Edition,
) -> Result<BTreeMap<usize, crate::local_stages::ShotBinding>, MediaError> {
    let bindings_path = crate::local_stages::bindings_file(&context.layout, edition);
    let bindings: BTreeMap<usize, crate::local_stages::ShotBinding> =
        match std::fs::read(&bindings_path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| EngineError::Setup(e.to_string()))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(EngineError::io(&bindings_path, e).into()),
        };
    Ok(bindings)
}

fn scene_clips(
    context: &Context,
    board: &Storyboard,
    images: &Images,
    edition: &Edition,
    timeline: &mut Timeline,
) -> Result<(), MediaError> {
    let m = context.guide.manifest();
    let fps = timeline.fps;
    let frames = |s: f64| {
        Seconds::new(s)
            .map(|s| s.to_frames(fps))
            .ok_or_else(|| MediaError::Quality("invalid scene timing".into()))
    };
    let duration = timeline.duration;
    let plates = load_plates(context, &timeline.name)?;
    let mut history = Vec::new();
    let bindings = load_shot_bindings(context, edition)?;
    for (i, frame) in images.frames.iter().enumerate() {
        if frame.unit != i {
            return Err(MediaError::Quality(
                "image ids are not in storyboard order".into(),
            ));
        }
        frame.image.verify()?;
        let (start, end) = scene_span(
            &edition.alignment.units,
            i,
            images.frames.len(),
            duration,
            &frames,
        )?;
        let d = Frames(end.0 - start.0);
        let (image_path, dimensions) = if let Some(plate) = plates.get(&i) {
            (plate.path.clone(), plate.canvas)
        } else if let Some(binding) = bindings.get(&i) {
            (binding.path.clone(), binding.dimensions)
        } else {
            prepared_image(context, &frame.image.path)?
        };
        let section = m
            .structure
            .sections
            .iter()
            .find(|s| s.id.as_str() == board.units[i].section)
            .ok_or_else(|| MediaError::Quality("unit names an unknown section".into()))?;
        let (motion, strength) = scene_motion(
            section.edit.motion,
            i,
            &board.direction.shots[i],
            frame.qa.focus,
            &history,
        );
        history.push(motion);
        let mut clip = Clip {
            track: "V1".into(),
            start,
            duration: d,
            source: Source::Media { path: image_path },
            effects: vec![if bindings.get(&i).is_some_and(|b| !b.camera) {
                camera(
                    dimensions,
                    m.format.resolution,
                    Frames(1),
                    Motion::ZoomIn,
                    [0.5, 0.5],
                )
            } else {
                camera_with(
                    dimensions,
                    m.format.resolution,
                    d,
                    motion,
                    frame.qa.focus,
                    strength,
                )
            }],
            transition: None,
            audio_gain_db: None,
        };
        if let Some(plate) = plates.get(&i) {
            native_reframe(plate, &mut clip, d);
        }
        treatment(context, board, i, &mut clip)?;
        append_scene(
            context,
            timeline,
            clip,
            dimensions,
            frame.qa.focus,
            ScenePosition::from_index(i, images.frames.len()),
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenePosition {
    Body,
    Ending,
}

impl ScenePosition {
    const fn from_index(index: usize, count: usize) -> Self {
        if index + 1 == count {
            Self::Ending
        } else {
            Self::Body
        }
    }
}

fn append_scene(
    context: &Context,
    timeline: &mut Timeline,
    mut clip: Clip,
    dimensions: [u32; 2],
    focus: [f64; 2],
    position: ScenePosition,
) -> Result<(), MediaError> {
    let m = context.guide.manifest();
    let d = clip.duration;
    let start = clip.start;
    let fps = timeline.fps;
    let frames = |s: f64| {
        Seconds::new(s)
            .map(|s| s.to_frames(fps))
            .ok_or_else(|| MediaError::Quality("invalid fade time".into()))
    };
    let rendered =
        matches!(&clip.source,Source::Media{path} if path.extension().is_some_and(|e|e=="mov"));
    if context.meta.quality == crate::Quality::Lean && d.0 >= 4 && !rendered {
        let mut close = clip.clone();
        clip.duration = Frames(d.0 / 2);
        close.start = Frames(start.0 + clip.duration.0);
        close.duration = Frames(d.0 - clip.duration.0);
        close.transition = None;
        clip.effects = vec![camera(
            dimensions,
            m.format.resolution,
            clip.duration,
            Motion::ZoomIn,
            focus,
        )];
        let mut zoom = camera(
            dimensions,
            m.format.resolution,
            close.duration,
            Motion::ZoomOut,
            focus,
        );
        magnify(&mut zoom, dimensions, m.format.resolution, focus, 1.45);
        close.effects = vec![zoom];
        // The hold-and-fade belongs to the final close-up, not the wide half.
        if position == ScenePosition::Ending {
            let fade = frames(m.edit.ending.seconds)?
                .0
                .min(close.duration.0.saturating_sub(1));
            if fade > 0 {
                close.effects.push(animated(
                    "opacity",
                    "opacity",
                    vec![
                        key(close.duration.0 - fade - 1, json!(100.0), Easing::Linear),
                        key(close.duration.0 - 1, json!(0.0), Easing::Linear),
                    ],
                ));
            }
        }
        timeline.clips.extend([clip, close]);
    } else {
        timeline.clips.push(clip);
    }
    Ok(())
}

fn treatment(
    context: &Context,
    board: &Storyboard,
    i: usize,
    clip: &mut Clip,
) -> Result<(), MediaError> {
    let m = context.guide.manifest();
    let d = clip.duration;
    let frames = |s: f64| {
        Seconds::new(s)
            .map(|s| s.to_frames(m.format.fps))
            .ok_or_else(|| MediaError::Quality("invalid effect time".into()))
    };
    let section = m
        .structure
        .sections
        .iter()
        .find(|s| s.id.as_str() == board.units[i].section)
        .ok_or_else(|| MediaError::Quality("unknown section".into()))?;
    if i > 0
        && board.units[i - 1].section != board.units[i].section
        && let Some(transition) = section.edit.transition
    {
        let effect = serde_json::to_value(transition)
            .map_err(|e| EngineError::Setup(e.to_string()))?
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let count = (d.0 / 3).clamp(1, 12);
        if count < d.0 {
            clip.transition = Some(Transition {
                effect,
                frames: u32::try_from(count).unwrap_or(12),
            });
        }
    }
    for effect in &section.edit.effects {
        let id = serde_json::to_value(effect)
            .map_err(|e| EngineError::Setup(e.to_string()))?
            .as_str()
            .unwrap_or_default()
            .to_owned();
        clip.effects.push(effect_values(
            &id,
            &[
                ("amount", json!(3.0)),
                ("rotation", json!(0.2)),
                ("zoom", json!(1.0)),
            ],
        ));
    }
    if i + 1 == board.units.len() {
        let fade = frames(m.edit.ending.seconds)?.0.min(d.0.saturating_sub(1));
        if fade > 0 {
            clip.effects.push(animated(
                "opacity",
                "opacity",
                vec![
                    key(d.0 - fade - 1, json!(100.0), Easing::Linear),
                    key(d.0 - 1, json!(0.0), Easing::Linear),
                ],
            ));
        }
    }
    Ok(())
}

/// A camera move and strength that suit the shot's feeling, when the storyboard gives one:
/// a slow push for dread, a quick punch for action, near-stillness for grief, a gentle
/// pull-back for warmth.
fn mood_motion(shot: &crate::story::Shot) -> Option<(Motion, f64)> {
    use crate::story::{Pace, Tone};
    let delivery = shot.delivery?;
    let mood = shot
        .score
        .as_ref()
        .map(|s| s.mood.to_lowercase())
        .unwrap_or_default();
    Some(match (delivery.tone, delivery.pace) {
        (_, Pace::Fast) | (Tone::Excited, _) => (Motion::Punch, 0.8),
        (Tone::Tense | Tone::Dark, _) => (Motion::ZoomIn, 0.7),
        (Tone::Whisper, _) => (Motion::ZoomIn, 0.4),
        _ if mood.contains("sad") || mood.contains("grief") => (Motion::ZoomOut, 0.3),
        (Tone::Warm, _) => (Motion::ZoomOut, 0.6),
        _ => return None,
    })
}

/// The frames unit `i` is on screen: from its cut to the next cut (or the end).
fn scene_span(
    units: &[retold_lang::TimedUnit],
    i: usize,
    count: usize,
    duration: Frames,
    frames: &dyn Fn(f64) -> Result<Frames, MediaError>,
) -> Result<(Frames, Frames), MediaError> {
    let start = if i == 0 {
        Frames(0)
    } else {
        frames(cut_time(units, i))?
    };
    let end = if i + 1 == count {
        duration
    } else {
        frames(cut_time(units, i + 1))?
    };
    if end.0 <= start.0 {
        return Err(MediaError::Quality(
            "unit has no whole frame of screen time".into(),
        ));
    }
    Ok((start, end))
}

/// The guide's section move wins; then the opening punch, then the shot's feeling.
fn scene_motion(
    section: Option<Motion>,
    i: usize,
    shot: &crate::story::Shot,
    focus: [f64; 2],
    history: &[Motion],
) -> (Motion, f64) {
    match (section, i, mood_motion(shot)) {
        (Some(m), ..) => (m, 1.0),
        (None, 0, Some(_)) => (Motion::Punch, 1.0),
        (None, _, Some((m, k))) => (choose_motion(Some(m), &shot.shot, focus, history), k),
        (None, ..) => (choose_motion(None, &shot.shot, focus, history), 1.0),
    }
}

/// Where the cut to unit `i` lands: inside the silence before its first word when there
/// is one (the picture leads the line), otherwise a few frames before the word.
fn cut_time(units: &[retold_lang::TimedUnit], i: usize) -> f64 {
    let start = units[i].start;
    let previous_end = units[i - 1].end.min(start);
    if start - previous_end >= 0.25 {
        previous_end + (start - previous_end) * 0.5
    } else {
        (start - 0.08).max(previous_end)
    }
}

fn choose_motion(
    preferred: Option<Motion>,
    shot: &str,
    focus: [f64; 2],
    history: &[Motion],
) -> Motion {
    let desired = preferred.unwrap_or_else(|| match shot {
        "wide" | "high angle" => {
            if focus[0] < 0.45 {
                Motion::PanRight
            } else {
                Motion::PanLeft
            }
        }
        "close-up" | "extreme close-up" => Motion::ZoomOut,
        _ => Motion::ZoomIn,
    });
    let recent_start = history.len().saturating_sub(2);
    let repeated = history.len() >= 2 && history[recent_start..].iter().all(|m| *m == desired);
    match (repeated, desired) {
        (true, Motion::ZoomOut) => Motion::ZoomIn,
        (true, _) => Motion::ZoomOut,
        (false, _) => desired,
    }
}
fn magnify(effect: &mut Effect, image: [u32; 2], canvas: [u32; 2], focus: [f64; 2], factor: f64) {
    if let Some(scale) = effect.params.get_mut("scale")
        && let Some(value) = scale.as_f64()
    {
        *scale = json!(value * factor);
        effect.params.insert(
            "position".into(),
            json!(crop_position(image, canvas, value * factor, focus)),
        );
    }
    for animation in &mut effect.animations {
        if animation.param == "scale" {
            for key in &mut animation.keys {
                if let Some(value) = key.value.as_f64() {
                    key.value = json!(value * factor);
                }
            }
        }
        // Keep the close-up focused while respecting crop bounds.
        if animation.param == "position" {
            for key in &mut animation.keys {
                let cover = (f64::from(canvas[0]) / f64::from(image[0]))
                    .max(f64::from(canvas[1]) / f64::from(image[1]))
                    * 100.0
                    * factor;
                key.value = json!(crop_position(image, canvas, cover, focus));
            }
        }
    }
}

fn prepared_image(context: &Context, path: &Path) -> Result<(PathBuf, [u32; 2]), MediaError> {
    let bytes = std::fs::read(path).map_err(|e| EngineError::io(path, e))?;
    let dimensions = retold_providers::media::image_dimensions(&bytes)?;
    let canvas = context.guide.manifest().format.resolution;
    let target = [
        canvas[0].saturating_mul(3) / 2,
        canvas[1].saturating_mul(3) / 2,
    ];
    if dimensions[0] >= target[0] && dimensions[1] >= target[1] {
        return Ok((path.into(), dimensions));
    }
    let key = crate::store::hash(&(
        blake3::hash(&bytes).to_hex().to_string(),
        target,
        "lanczos3-v1",
    ))?;
    let output = context
        .layout
        .meta()
        .join("derived-images")
        .join(format!("{key}.png"));
    if output.exists() {
        let data = std::fs::read(&output).map_err(|e| EngineError::io(&output, e))?;
        return Ok((output, retold_providers::media::image_dimensions(&data)?));
    }
    let scale = (f64::from(target[0]) / f64::from(dimensions[0]))
        .max(f64::from(target[1]) / f64::from(dimensions[1]));
    let resized = [
        num::trunc_u32((f64::from(dimensions[0]) * scale).ceil()),
        num::trunc_u32((f64::from(dimensions[1]) * scale).ceil()),
    ];
    if resized.iter().any(|n| *n > 8192) {
        return Err(MediaError::Quality(
            "upscale exceeds the image safety limit".into(),
        ));
    }
    let image = image::load_from_memory(&bytes)
        .map_err(|e| MediaError::Quality(e.to_string()))?
        .resize_exact(
            resized[0],
            resized[1],
            image::imageops::FilterType::Lanczos3,
        );
    let mut data = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut data, image::ImageFormat::Png)
        .map_err(|e| MediaError::Quality(e.to_string()))?;
    fsx::write_atomic(&output, data.get_ref())?;
    fsx::write_json_atomic(
        &output.with_extension("json"),
        &serde_json::json!({"source":path,"source_dimensions":dimensions,"output_dimensions":resized,"method":"Lanczos3 interpolation; no new source detail"}),
    )?;
    Ok((output, resized))
}

const fn key(offset: u64, value: Value, easing: Easing) -> Key {
    Key {
        offset: Frames(offset),
        value,
        easing,
    }
}
fn animated(id: &str, param: &str, keys: Vec<Key>) -> Effect {
    Effect {
        id: id.into(),
        params: BTreeMap::new(),
        animations: vec![Animation {
            param: param.into(),
            keys,
        }],
    }
}
fn effect_values(id: &str, values: &[(&str, Value)]) -> Effect {
    Effect {
        id: id.into(),
        params: values
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
        animations: vec![],
    }
}

fn camera(
    image: [u32; 2],
    canvas: [u32; 2],
    duration: Frames,
    motion: Motion,
    focus: [f64; 2],
) -> Effect {
    camera_with(image, canvas, duration, motion, focus, 1.0)
}

/// A camera move whose size is scaled by `strength` (1 is the standard move; 0.3 is
/// almost still; 1.4 is pronounced).
fn camera_with(
    image: [u32; 2],
    canvas: [u32; 2],
    duration: Frames,
    motion: Motion,
    focus: [f64; 2],
    strength: f64,
) -> Effect {
    let cover = (f64::from(canvas[0]) / f64::from(image[0]))
        .max(f64::from(canvas[1]) / f64::from(image[1]))
        * 100.0;
    let (zoom, pan, punch) = (
        1.0 + 0.10 * strength,
        0.08 * strength,
        1.0 + 0.20 * strength,
    );
    let motion_config = [
        (Motion::ZoomOut, zoom, 1.0, 0.0, 0.0),
        (Motion::Punch, 1.0, punch, 0.0, 0.0),
        (Motion::PanRight, zoom, zoom, -pan, pan),
        (Motion::PanLeft, zoom, zoom, pan, -pan),
    ]
    .into_iter()
    .find(|(candidate_motion, ..)| *candidate_motion == motion)
    .unwrap_or((motion, 1.0, zoom, 0.0, 0.0));

    let (a, b) = (cover * motion_config.1, cover * motion_config.2);
    let fa = [(focus[0] + motion_config.3).clamp(0.0, 1.0), focus[1]];
    let fb = [(focus[0] + motion_config.4).clamp(0.0, 1.0), focus[1]];
    let end = duration.0.saturating_sub(1);
    let mut effect = effect_values(
        "motion",
        &[
            ("scale", json!(a)),
            ("position", json!(crop_position(image, canvas, a, fa))),
        ],
    );
    if end > 0 {
        effect.animations = vec![
            Animation {
                param: "scale".into(),
                keys: vec![
                    key(0, json!(a), Easing::EaseOut),
                    key(end, json!(b), Easing::EaseIn),
                ],
            },
            Animation {
                param: "position".into(),
                keys: vec![
                    key(
                        0,
                        json!(crop_position(image, canvas, a, fa)),
                        Easing::EaseOut,
                    ),
                    key(
                        end,
                        json!(crop_position(image, canvas, b, fb)),
                        Easing::EaseIn,
                    ),
                ],
            },
        ];
    }
    effect
}

/// The opening cover sits under the title; the title sits above it, below the look layer.
const COVER_TRACK: &str = "V3";
const TITLE_TRACK: &str = "V4";
/// Frames over which the cover dissolves into the first scene.
const COVER_DISSOLVE_FRAMES: u64 = 6;

fn opening(
    context: &Context,
    images: &Images,
    timeline: &mut Timeline,
    fonts: &dyn FontResolver,
) -> Result<(), MediaError> {
    let seconds = context.guide.manifest().edit.opening.seconds;
    let duration = Seconds::new(seconds)
        .ok_or_else(|| MediaError::Quality("invalid opening duration".into()))?
        .to_frames(timeline.fps);
    let duration = Frames(duration.0.min(timeline.duration.0));
    let plates = load_plates(context, &timeline.name)?;
    let (cover_path, mut effects) = if let Some(plate) = plates.get(&0) {
        let mut clip = Clip {
            track: COVER_TRACK.into(),
            start: Frames(0),
            duration,
            source: Source::Media {
                path: plate.path.clone(),
            },
            effects: vec![],
            transition: None,
            audio_gain_db: None,
        };
        native_reframe(plate, &mut clip, duration);
        (plate.path.clone(), clip.effects)
    } else {
        let (path, dimensions) = prepared_image(context, &images.cover.path)?;
        (
            path,
            vec![camera(
                dimensions,
                [timeline.width, timeline.height],
                duration,
                Motion::ZoomOut,
                [0.5, 0.42],
            )],
        )
    };
    if duration.0 > 4 {
        effects.push(animated("opacity", "opacity", cover_keys(duration)));
    }
    if let Some(effect) = context.guide.manifest().edit.opening.effect {
        let id = serde_json::to_value(effect)
            .map_err(|e| EngineError::Setup(e.to_string()))?
            .as_str()
            .unwrap_or_default()
            .to_owned();
        effects.push(effect_values(&id, &[("intensity", json!(12.0))]));
    }
    timeline.clips.push(Clip {
        track: COVER_TRACK.into(),
        start: Frames(0),
        duration,
        source: Source::Media { path: cover_path },
        effects,
        transition: None,
        audio_gain_db: None,
    });
    opening_title(context, timeline, fonts, duration)
}

/// The story name over the cover, fading and settling in, in the brand's title style.
pub fn opening_title(
    context: &Context,
    timeline: &mut Timeline,
    fonts: &dyn FontResolver,
    duration: Frames,
) -> Result<(), MediaError> {
    let style = &context.channel.file().title;
    let script = title_script(
        &context.meta.title,
        context.guide.manifest().format.source_language.script(),
    );
    let layout = title_layout(script, style)?;
    let caption_style = &context.channel.file().captions;
    let title_text = if caption_style.uppercase && script.has_case() {
        context.meta.title.to_uppercase()
    } else {
        context.meta.title.clone()
    };
    let font = context
        .channel
        .file()
        .fonts
        .get(&script)
        .ok_or_else(|| MediaError::Quality("no title font configured".into()))?;
    let measure = fonts.face(font)?;
    let size = title_size(&title_text, measure.as_ref(), style, timeline, &layout)?;
    let position = [
        f64::from(timeline.width) / 2.0,
        f64::from(timeline.height) * style.y,
    ];
    let mut effects = Vec::new();
    if duration.0 > 8 {
        effects.push(animated(
            "opacity",
            "opacity",
            vec![
                key(0, json!(0.0), Easing::Linear),
                key(3, json!(100.0), Easing::EaseOut),
                key(duration.0 - 5, json!(100.0), Easing::Linear),
                key(duration.0 - 1, json!(0.0), Easing::Linear),
            ],
        ));
        let mut settle = animated(
            "motion",
            "scale",
            vec![
                key(0, json!(108.0), Easing::EaseOut),
                key(6, json!(100.0), Easing::EaseOut),
            ],
        );
        let center = [position[0], position[1] - size * 0.35];
        settle.params.insert("anchor".into(), json!(center));
        settle.params.insert("position".into(), json!(center));
        effects.push(settle);
    }
    // The title uses the brand's caption colours with a heavier outline, above the cover.
    let stroke_width = num::trunc_u32((size * 0.07).round().max(4.0));
    let props = json!({
        "align": if style.vertical { "left" } else { "center" },
        "vertical": style.vertical,
        "leading": size * f64::from(layout.line_spacing_scale),
        "fill_color": color(context, &caption_style.fill)?,
        "stroke": true,
        "stroke_width": stroke_width,
        "stroke_color": color(context, &caption_style.stroke.color)?,
        "shadow": true,
    });
    timeline.clips.push(Clip {
        track: TITLE_TRACK.into(),
        start: Frames(0),
        duration,
        source: Source::Text {
            text: title_text,
            font: font.family.clone(),
            font_style: font.style.clone(),
            size,
            position,
            props,
            highlight: None,
            emphasis: vec![],
        },
        effects,
        transition: None,
        audio_gain_db: None,
    });
    Ok(())
}

fn title_size(
    text: &str,
    measure: &dyn retold_lang::fit::Measure,
    style: &retold_guide::channel::TitleStyle,
    timeline: &Timeline,
    layout: &retold_lipsync::TypographyLayout,
) -> Result<f64, MediaError> {
    let padding = f64::from(layout.glyph_padding_px);
    let width = f64::from(timeline.width) * (1.0 - 2.0 * style.side_margin) - 2.0 * padding;
    let desired = f64::from(style.size) * f64::from(layout.font_size_scale);
    if style.vertical {
        let height = f64::from(timeline.height) * (1.0 - style.y) - 2.0 * padding;
        let count = f64::from(
            u32::try_from(text.chars().count()).map_err(|e| EngineError::Setup(e.to_string()))?,
        );
        let size = desired.min(height / count.max(1.0)).min(width);
        if size < 24.0 {
            return Err(MediaError::Quality(
                "vertical title cannot fit safely".into(),
            ));
        }
        return Ok(size);
    }
    let (ascent, descent) = measure.vertical_bounds(desired);
    let baseline = f64::from(timeline.height) * style.y;
    let top = ((baseline - padding - desired * 0.08) / ascent.max(1.0)).min(1.0);
    let bottom = ((f64::from(timeline.height) - baseline - padding) / descent.max(1.0)).min(1.0);
    let safe_size = desired * top.min(bottom);
    if safe_size < 24.0 {
        return Err(MediaError::Quality(
            "title cannot fit its baseline without clipping".into(),
        ));
    }
    let size = retold_lang::fit::fit_size(measure, text, safe_size, width, 24.0);
    if measure.width(text, size) > width + 1.0 {
        return Err(MediaError::Quality(
            "title is too wide even at minimum size".into(),
        ));
    }
    Ok(size)
}

fn title_layout(
    script: WritingScript,
    style: &retold_guide::channel::TitleStyle,
) -> Result<retold_lipsync::TypographyLayout, MediaError> {
    let cjk = matches!(
        script,
        WritingScript::Jpan | WritingScript::Hans | WritingScript::Hant
    );
    if style.vertical && !cjk {
        return Err(MediaError::Quality(
            "vertical titles require Japanese or Chinese text".into(),
        ));
    }
    let layout = if style.vertical {
        retold_lipsync::TypographyLayout::cjk_vertical()
    } else {
        retold_lipsync::TypographyLayout::for_language(script_layout_language(script))
    };
    Ok(layout)
}

fn script_layout_language(script: WritingScript) -> &'static str {
    const SCRIPT_LANGUAGES: &[(WritingScript, &str)] = &[
        (WritingScript::Arab, "ur"),
        (WritingScript::Deva, "hi"),
        (WritingScript::Jpan, "ja"),
        (WritingScript::Hans, "zh"),
        (WritingScript::Hant, "zh"),
    ];
    SCRIPT_LANGUAGES
        .iter()
        .find(|(s, _)| *s == script)
        .map_or("en", |(_, language_code)| *language_code)
}

fn title_script(title: &str, preferred: WritingScript) -> WritingScript {
    if title
        .chars()
        .any(|c| ('\u{0900}'..='\u{097F}').contains(&c))
    {
        WritingScript::Deva
    } else if title
        .chars()
        .any(|c| ('\u{0600}'..='\u{06FF}').contains(&c))
    {
        WritingScript::Arab
    } else if title
        .chars()
        .any(|c| ('\u{3040}'..='\u{30FF}').contains(&c))
    {
        WritingScript::Jpan
    } else if title
        .chars()
        .any(|c| ('\u{3400}'..='\u{9FFF}').contains(&c))
    {
        han_title_script(preferred)
    } else {
        WritingScript::Latn
    }
}

const fn han_title_script(preferred: WritingScript) -> WritingScript {
    match preferred {
        WritingScript::Hans | WritingScript::Hant | WritingScript::Jpan => preferred,
        _ => WritingScript::Hans,
    }
}

fn color(context: &Context, color: &retold_guide::channel::ColorRef) -> Result<String, MediaError> {
    if color.0.starts_with('#') {
        return Ok(color.0.clone());
    }
    context
        .channel
        .file()
        .colors
        .get(&color.0)
        .map(ToString::to_string)
        .ok_or_else(|| MediaError::Quality(format!("unknown color {}", color.0)))
}

fn captions(
    context: &Context,
    edition: &Edition,
    keywords: &[String],
    timeline: &mut Timeline,
    fonts: &dyn FontResolver,
) -> Result<(), MediaError> {
    let Some(track) = edition.variant.burned() else {
        return Ok(());
    };
    let style = &context.channel.file().captions;
    let font = context
        .channel
        .file()
        .fonts
        .get(&track.lang.script())
        .ok_or_else(|| MediaError::Quality("caption font is not configured".into()))?;
    let measure = fonts.face(font)?;
    let max_width = f64::from(timeline.width) * (1.0 - 2.0 * style.side_margin);
    for block in &edition.captions {
        let start = Seconds::new(block.start)
            .ok_or_else(|| MediaError::Quality("invalid caption time".into()))?
            .to_frames(timeline.fps);
        let end = Seconds::new(block.end)
            .ok_or_else(|| MediaError::Quality("invalid caption time".into()))?
            .to_frames(timeline.fps);
        let end = Frames(end.0.min(timeline.duration.0));
        if end.0 <= start.0 {
            continue;
        }
        let duration = Frames(end.0 - start.0);
        let text = if block.highlight.is_none() {
            wrap_phrase(&block.text, 32)
        } else {
            block.text.clone()
        };
        let width = text
            .lines()
            .map(|line| measure.width(line, f64::from(style.size)))
            .fold(0.0, f64::max);
        let size = if width > max_width {
            (f64::from(style.size) * max_width / width).floor()
        } else {
            f64::from(style.size)
        };
        if size < 24.0 {
            return Err(MediaError::Quality(
                "caption translation is too long to remain readable".into(),
            ));
        }
        let highlight = block
            .highlight
            .map(|[a, b]| {
                Ok::<_, MediaError>(Highlight {
                    start: a,
                    end: b,
                    color: color(context, &style.highlight)?,
                })
            })
            .transpose()?;
        let emphasis = emphasis_color(context)?
            .map(|color| {
                keyword_ranges(&text, keywords)
                    .into_iter()
                    .map(|(start, end)| Highlight {
                        start,
                        end,
                        color: color.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut effects = Vec::new();
        if block.pop && duration.0 > 4 {
            effects.push(animated(
                "motion",
                "scale",
                vec![
                    key(0, json!(85.0), Easing::EaseOut),
                    key(3, json!(100.0), Easing::EaseIn),
                ],
            ));
        }
        let center = [
            f64::from(timeline.width) / 2.0,
            f64::from(timeline.height) * style.y - size * 0.35,
        ];
        for effect in &mut effects {
            if effect.id == "motion" {
                effect.params.insert("anchor".into(), json!(center));
                effect.params.insert("position".into(), json!(center));
            }
        }
        timeline.clips.push(Clip { track:"V2".into(),start,duration,source:Source::Text { text,font:font.family.clone(),font_style:font.style.clone(),size,position:[f64::from(timeline.width)/2.0,f64::from(timeline.height)*style.y],props:json!({"align":"center","fill_color":color(context,&style.fill)?,"stroke":true,"stroke_width":style.stroke.width,"stroke_color":color(context,&style.stroke.color)?,"shadow":true}),highlight,emphasis },effects,transition:None,audio_gain_db:None });
    }
    Ok(())
}

/// Names worth emphasising in captions: characters, places and props, plus the last word of
/// multi-word names (people are usually called by surname).
fn caption_keywords(board: &Storyboard) -> Vec<String> {
    let mut words: Vec<String> = board
        .plan
        .characters
        .iter()
        .chain(&board.plan.props)
        .chain(board.plan.settings.iter().map(|p| &p.name))
        .flat_map(|name| {
            let last = name
                .split_whitespace()
                .last()
                .filter(|w| w.chars().count() >= 4 && name.contains(' '));
            std::iter::once(name.clone()).chain(last.map(str::to_owned))
        })
        .filter(|w| !w.trim().is_empty())
        .collect();
    // Longest first, so a full name wins over its surname.
    words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    words.dedup();
    words
}

fn emphasis_color(context: &Context) -> Result<Option<String>, MediaError> {
    let style = &context.channel.file().captions;
    match &style.emphasis {
        Some(c) => color(context, c).map(Some),
        None => Ok(context
            .channel
            .file()
            .colors
            .get("accent")
            .map(ToString::to_string)),
    }
}

/// Character ranges of whole-word, case-insensitive keyword matches, without overlaps.
fn keyword_ranges(text: &str, keywords: &[String]) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let mut taken = vec![false; chars.len()];
    let mut out = Vec::new();
    for keyword in keywords {
        let key: Vec<char> = keyword.chars().map(fold).collect();
        let key_len = key.len();
        if key.is_empty() || key_len > chars.len() {
            continue;
        }
        for (start, window) in chars.windows(key_len).enumerate() {
            let end = start + key_len;
            let boundary = |i: Option<&char>| i.is_none_or(|c| !c.is_alphanumeric());
            if window.iter().map(|c| fold(*c)).eq(key.iter().copied())
                && boundary(start.checked_sub(1).and_then(|i| chars.get(i)))
                && boundary(chars.get(end))
                && !taken[start..end].iter().any(|t| *t)
            {
                taken[start..end].iter_mut().for_each(|t| *t = true);
                out.push((start, end));
            }
        }
    }
    out.sort_unstable();
    out
}

fn wrap_phrase(text: &str, max: usize) -> String {
    let words: Vec<_> = text.split_whitespace().collect();
    if text.chars().count() <= max || words.len() < 2 {
        return text.into();
    }
    let middle = words.len() / 2;
    format!(
        "{}\n{}",
        words[..middle].join(" "),
        words[middle..].join(" ")
    )
}

fn look(context: &Context, timeline: &mut Timeline) {
    let look = &context.channel.file().look;
    let effects = vec![
        effect_values(
            "vignette",
            &[
                ("amount", json!(look.vignette.amount)),
                ("midpoint", json!(look.vignette.midpoint)),
                ("feather", json!(look.vignette.feather)),
            ],
        ),
        effect_values(
            "noise",
            &[
                ("amount", json!(look.grain.amount)),
                ("grain_size", json!(look.grain.size)),
            ],
        ),
        effect_values(
            "lumetri",
            &[
                ("contrast", json!(look.grade.contrast)),
                ("saturation", json!(look.grade.saturation)),
                ("highlights", json!(look.grade.highlights)),
            ],
        ),
    ];
    timeline.clips.push(Clip {
        track: "V5".into(),
        start: Frames(0),
        duration: timeline.duration,
        source: Source::Adjustment,
        effects,
        transition: None,
        audio_gain_db: None,
    });
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MusicLibrary {
    tracks: Vec<MusicTrack>,
    /// Short accents for reveals (optional).
    #[serde(default)]
    stings: Vec<MusicTrack>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MusicTrack {
    path: String,
    #[serde(default)]
    moods: Vec<String>,
    lufs: f64,
}

fn load_music_library(manifest: &Path) -> Result<Option<MusicLibrary>, MediaError> {
    match std::fs::read_to_string(manifest) {
        Ok(text) => toml::from_str::<MusicLibrary>(&text)
            .map(Some)
            .map_err(|e| {
                EngineError::Config {
                    path: manifest.to_path_buf(),
                    message: e.to_string(),
                }
                .into()
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(EngineError::io(manifest, e).into()),
    }
}

fn music(
    context: &Context,
    board: &Storyboard,
    edition: &Edition,
    timeline: &mut Timeline,
) -> Result<(), MediaError> {
    let folder = context
        .channel
        .library_path(&context.channel.file().library.music);
    let manifest = folder.join("music.toml");
    let Some(library) = load_music_library(&manifest)? else {
        return Ok(());
    };
    let bed_track = bed_track(context, edition);
    if board.direction.shots.iter().any(|s| s.score.is_some()) {
        return scored_music(
            context, board, edition, timeline, &library, &folder, bed_track,
        );
    }
    let mut tracks: Vec<_> = library
        .tracks
        .iter()
        .filter(|t| t.lufs.is_finite())
        .collect();
    tracks.sort_by_key(|t| {
        (
            !t.moods
                .iter()
                .any(|m| board.plan.tone.to_lowercase().contains(&m.to_lowercase())),
            t.path.clone(),
        )
    });
    let Some(track) = tracks.first() else {
        return Ok(());
    };
    let path = safe_library_file(&folder, &track.path)?;
    let bytes = std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?;
    let seconds = retold_providers::media::audio_duration(&bytes)?;
    let length = Seconds::new(seconds)
        .ok_or_else(|| MediaError::Quality("music duration is invalid".into()))?
        .to_frames(timeline.fps);
    let settings = &context.guide.manifest().edit.music;
    let mut start = 0;
    while start < timeline.duration.0 {
        let duration = Frames(length.0.min(timeline.duration.0 - start));
        if duration.0 == 0 {
            return Err(MediaError::Quality("music is too short".into()));
        }
        let mut points = BTreeMap::new();
        points.insert(0, if start == 0 { -40.0 } else { 0.0 });
        for (a, b) in voice_intervals(edition, timeline.fps) {
            let a = a.saturating_sub(4);
            let b = b.saturating_add(4);
            if a <= start && b > start {
                points.insert(0, settings.duck_under_voice_db);
            }
            for (frame, value) in [
                (a.saturating_sub(4), 0.0),
                (a, settings.duck_under_voice_db),
                (b, settings.duck_under_voice_db),
                (b + 4, 0.0),
            ] {
                if (start..start + duration.0).contains(&frame) {
                    points.insert(frame - start, value);
                }
            }
        }
        if start == 0 {
            points.insert(0, -40.0);
            points
                .entry(8.min(duration.0 - 1))
                .or_insert(settings.duck_under_voice_db);
        }
        if start + duration.0 == timeline.duration.0 {
            points.insert(duration.0 - 1, -40.0);
        }
        let keys = points
            .into_iter()
            .map(|(offset, value)| key(offset, json!(value), Easing::Linear))
            .collect();
        timeline.clips.push(Clip {
            track: bed_track.into(),
            start: Frames(start),
            duration,
            source: Source::Media { path: path.clone() },
            effects: vec![animated("volume", "level", keys)],
            transition: None,
            audio_gain_db: Some(settings.bed_lufs - track.lufs),
        });
        start += duration.0;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SoundLibrary {
    sounds: Vec<SoundFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SoundFile {
    path: String,
    /// Words it answers to, such as `rain` and `storm`.
    tags: Vec<String>,
    /// "ambience" (loops under a place) or "effect" (one shot).
    kind: String,
    lufs: f64,
}

/// The library sound whose tags best match `wanted` (most shared words), if any match.
fn best_sound<'a>(library: &'a SoundLibrary, kind: &str, wanted: &str) -> Option<&'a SoundFile> {
    let words: Vec<String> = wanted
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_owned)
        .collect();
    library
        .sounds
        .iter()
        .filter(|s| s.kind == kind && s.lufs.is_finite())
        .map(|s| {
            (
                s.tags
                    .iter()
                    .filter(|t| {
                        words
                            .iter()
                            .any(|w| t.to_lowercase().split_whitespace().any(|tw| tw == w))
                    })
                    .count(),
                s,
            )
        })
        .filter(|(score, _)| *score > 0)
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.path.cmp(&a.1.path)))
        .map(|(_, s)| s)
}

/// Diegetic sound from the storyboard and the channel's `sfx.toml`: ambience looped under
/// consecutive units that share it (half-second fades, well under the voice), and one-shot
/// effects at the start of their unit.
fn scene_sound(
    context: &Context,
    board: &Storyboard,
    edition: &Edition,
    timeline: &mut Timeline,
) -> Result<(), MediaError> {
    let folder = context
        .channel
        .library_path(&context.channel.file().library.sfx);
    let manifest = folder.join("sfx.toml");
    let library: SoundLibrary = match std::fs::read_to_string(&manifest) {
        Ok(text) => toml::from_str(&text).map_err(|e| EngineError::Config {
            path: manifest.clone(),
            message: e.to_string(),
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(EngineError::io(&manifest, e).into()),
    };
    let (fps, total) = (timeline.fps, timeline.duration.0);
    let at = |seconds: f64| {
        Seconds::new(seconds.max(0.0))
            .unwrap_or(Seconds::ZERO)
            .to_frames(fps)
            .0
            .min(total)
    };
    let bed = context.guide.manifest().edit.music.bed_lufs;
    // Ambience runs: (start, end, wanted words).
    let mut runs: Vec<(u64, u64, String)> = Vec::new();
    for (i, shot) in board.direction.shots.iter().enumerate() {
        let Some(timing) = edition.alignment.units.get(i) else {
            continue;
        };
        let wanted = shot
            .sound
            .as_ref()
            .map(|s| s.ambience.trim().to_lowercase())
            .unwrap_or_default();
        let (start, end) = (
            if i == 0 { 0 } else { at(timing.start) },
            at(edition
                .alignment
                .units
                .get(i + 1)
                .map_or(f64::MAX / 4.0, |n| n.start)),
        );
        match runs.last_mut() {
            Some(last) if last.2 == wanted && last.1 == start => last.1 = end,
            _ => runs.push((start, end, wanted)),
        }
        if let Some(effect) = shot
            .sound
            .as_ref()
            .map(|s| s.effect.trim())
            .filter(|e| !e.is_empty())
            && let Some(file) = best_sound(&library, "effect", effect)
        {
            let path = safe_library_file(&folder, &file.path)?;
            let seconds = retold_providers::media::audio_duration(
                &std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?,
            )?;
            let start = at(timing.start).saturating_sub(3);
            let len = at(seconds.min(4.0)).min(total.saturating_sub(start));
            if len > 0 {
                timeline.clips.push(Clip {
                    track: "A9".into(),
                    start: Frames(start),
                    duration: Frames(len),
                    source: Source::Media { path },
                    effects: vec![],
                    transition: None,
                    audio_gain_db: Some(bed + 6.0 - file.lufs),
                });
            }
        }
    }
    place_ambience(timeline, &library, &folder, runs, bed, &at)
}

/// Music, transition whooshes and scene sound.
fn sound_layers(
    context: &Context,
    board: &Storyboard,
    edition: &Edition,
    timeline: &mut Timeline,
) -> Result<(), MediaError> {
    music(context, board, edition, timeline)?;
    whooshes(context, board, edition, timeline)?;
    scene_sound(context, board, edition, timeline)
}

/// Ambience runs looped under their units, half-second fades, under the music bed.
fn place_ambience(
    timeline: &mut Timeline,
    library: &SoundLibrary,
    folder: &Path,
    runs: Vec<(u64, u64, String)>,
    bed: f64,
    at: &dyn Fn(f64) -> u64,
) -> Result<(), MediaError> {
    let fade = at(0.5).max(1);
    for (k, (start, end, wanted)) in runs.into_iter().enumerate() {
        let Some(file) = (!wanted.is_empty())
            .then(|| best_sound(library, "ambience", &wanted))
            .flatten()
        else {
            continue;
        };
        let path = safe_library_file(folder, &file.path)?;
        let seconds = retold_providers::media::audio_duration(
            &std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?,
        )?;
        let length = at(seconds).max(1);
        let mut piece = start;
        while piece < end {
            let len = length.min(end - piece);
            let edge = fade.min(len / 2);
            let mut keys = BTreeMap::from([
                (0, if piece == start { -40.0 } else { 0.0 }),
                (len - 1, if piece + len == end { -40.0 } else { 0.0 }),
            ]);
            if piece == start {
                keys.insert(edge, 0.0);
            }
            if piece + len == end {
                keys.entry((len - 1).saturating_sub(edge)).or_insert(0.0);
            }
            timeline.clips.push(Clip {
                // Alternate tracks so a run's fade-out can meet the next run's fade-in.
                track: if k % 2 == 0 {
                    "A8".into()
                } else {
                    "A10".into()
                },
                start: Frames(piece),
                duration: Frames(len),
                source: Source::Media { path: path.clone() },
                effects: vec![animated(
                    "volume",
                    "level",
                    keys.into_iter()
                        .map(|(o, v)| key(o, json!(v), Easing::Linear))
                        .collect(),
                )],
                transition: None,
                // Ambience sits under the music bed so the voice stays clear.
                audio_gain_db: Some(bed - 4.0 - file.lufs),
            });
            piece += len;
        }
    }
    Ok(())
}

/// The music track: voices take A1–A3 when narration is voiced turn by turn.
fn bed_track(context: &Context, edition: &Edition) -> &'static str {
    if context.guide.manifest().narration.mode == retold_guide::manifest::NarrationMode::Dialogue
        || crate::voices::local_narrator(context, &edition.variant.narration)
    {
        "A4"
    } else {
        "A2"
    }
}

/// Music level (dB) for a bed clip at `offset` frames: ducked under the voice, dropped out
/// in silences, faded at clip edges that cross into another bed.
struct Bed<'a> {
    start: u64,
    len: u64,
    voice: &'a [(u64, u64)],
    quiet: &'a [(u64, u64)],
    duck: f64,
    fade: (u64, u64),
}

impl Bed<'_> {
    fn level(&self, offset: u64) -> f64 {
        let t = self.start + offset;
        let v = if self.quiet.iter().any(|(a, b)| t >= *a && t <= *b) {
            -40.0
        } else if self.voice.iter().any(|(a, b)| t + 4 >= *a && t <= b + 4) {
            self.duck
        } else {
            0.0
        };
        let ramp = |done: u64, over: u64| {
            if over == 0 || done >= over {
                0.0
            } else {
                -40.0 * (1.0 - num::u64_f64(done) / num::u64_f64(over))
            }
        };
        (v + ramp(offset, self.fade.0) + ramp(self.len - 1 - offset, self.fade.1)).max(-40.0)
    }

    /// Keyframes at every point where the level changes direction.
    fn keys(&self) -> Vec<Key> {
        let mut points = std::collections::BTreeSet::from([
            0,
            self.len - 1,
            self.fade.0,
            (self.len - 1).saturating_sub(self.fade.1),
        ]);
        let local = |frame: u64| frame.checked_sub(self.start).filter(|f| *f < self.len);
        for (a, b) in self.voice {
            points.extend(
                [a.saturating_sub(8), a.saturating_sub(4), b + 4, b + 8]
                    .into_iter()
                    .filter_map(local),
            );
        }
        for (a, b) in self.quiet {
            points.extend(
                [a.saturating_sub(6), *a, *b, b + 6]
                    .into_iter()
                    .filter_map(local),
            );
        }
        points
            .into_iter()
            .filter(|p| *p < self.len)
            .map(|p| key(p, json!(self.level(p)), Easing::Linear))
            .collect()
    }
}

/// Music that follows the story: units grouped into stretches of one mood (stretches
/// under eight seconds join the one before), each with the library track that best
/// matches it, crossfading over a second on alternating tracks; silences drop the music
/// out and stings accent reveals.
fn scored_music(
    context: &Context,
    board: &Storyboard,
    edition: &Edition,
    timeline: &mut Timeline,
    library: &MusicLibrary,
    folder: &Path,
    bed_track: &str,
) -> Result<(), MediaError> {
    let (fps, total) = (timeline.fps, timeline.duration.0);
    let at = |seconds: f64| {
        Seconds::new(seconds.max(0.0))
            .unwrap_or(Seconds::ZERO)
            .to_frames(fps)
            .0
            .min(total)
    };
    let settings = &context.guide.manifest().edit.music;
    let (stretches, quiet, stings) = music_cues(board, &edition.alignment.units, &at);
    let matches = |t: &MusicTrack, m: &str| {
        t.moods
            .iter()
            .any(|x| m.to_lowercase().contains(&x.to_lowercase()))
    };
    let voice = voice_intervals(edition, fps);
    let fade = at(1.0).max(1);
    let mut previous: Option<&str> = None;
    for (k, (start, end, mood)) in stretches.iter().enumerate() {
        let mut ranked: Vec<_> = library
            .tracks
            .iter()
            .filter(|t| t.lufs.is_finite())
            .collect();
        ranked.sort_by_key(|t| {
            (
                !matches(t, mood),
                !matches(t, &board.plan.tone),
                previous == Some(t.path.as_str()),
                t.path.clone(),
            )
        });
        let Some(track) = ranked.first() else {
            return Ok(());
        };
        previous = Some(&track.path);
        let path = safe_library_file(folder, &track.path)?;
        let seconds = retold_providers::media::audio_duration(
            &std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?,
        )?;
        let length = at(seconds).max(1);
        let from = if k == 0 {
            0
        } else {
            start.saturating_sub(fade)
        };
        let to = if k + 1 == stretches.len() {
            total
        } else {
            (end + fade).min(total)
        };
        let mut piece = from;
        while piece < to {
            let len = length.min(to - piece);
            let bed = Bed {
                start: piece,
                len,
                voice: &voice,
                quiet: &quiet,
                duck: settings.duck_under_voice_db,
                fade: (
                    if piece == from { fade.min(len / 2) } else { 0 },
                    if piece + len == to {
                        fade.min(len / 2)
                    } else {
                        0
                    },
                ),
            };
            timeline.clips.push(Clip {
                track: if k % 2 == 0 {
                    bed_track.into()
                } else {
                    "A6".into()
                },
                start: Frames(piece),
                duration: Frames(len),
                source: Source::Media { path: path.clone() },
                effects: vec![animated("volume", "level", bed.keys())],
                transition: None,
                audio_gain_db: Some(settings.bed_lufs - track.lufs),
            });
            piece += len;
        }
    }
    place_stings(timeline, library, folder, &stings, settings.bed_lufs, &at)?;
    Ok(())
}

/// A stretch of one music mood, in frames: `(start, end, mood)`.
type MoodStretch = (u64, u64, String);
/// A span with no music, in frames: `(start, end)`.
type Silence = (u64, u64);
/// A sting at a frame, with its mood: `(frame, mood)`.
type Sting = (u64, String);

/// Mood stretches, silent spans and sting points, in frames.
fn music_cues(
    board: &Storyboard,
    timings: &[retold_lang::align::TimedUnit],
    at: &dyn Fn(f64) -> u64,
) -> (Vec<MoodStretch>, Vec<Silence>, Vec<Sting>) {
    let total = at(f64::MAX / 4.0);
    let mut mood = board.plan.tone.clone();
    let mut stretches: Vec<MoodStretch> = Vec::new();
    let mut quiet = Vec::new();
    let mut stings = Vec::new();
    for (i, shot) in board.direction.shots.iter().enumerate() {
        let Some(timing) = timings.get(i) else {
            continue;
        };
        let start = if i == 0 { 0 } else { at(timing.start) };
        if let Some(score) = &shot.score {
            if !score.mood.trim().is_empty() {
                mood.clone_from(&score.mood);
            }
            if score.silence {
                quiet.push((at(timing.start), at(timing.end)));
            }
            if score.sting {
                stings.push((at(timing.start).saturating_sub(2), mood.clone()));
            }
        }
        match stretches.last_mut() {
            Some(last) if last.2.eq_ignore_ascii_case(&mood) => {}
            Some(last) if start - last.0 < at(8.0) => last.2.clone_from(&mood),
            _ => stretches.push((start, total, mood.clone())),
        }
    }
    for k in 1..stretches.len() {
        stretches[k - 1].1 = stretches[k].0;
    }
    for k in 1..stretches.len() {
        stretches[k - 1].1 = stretches[k].0;
    }
    (stretches, quiet, stings)
}

/// Stings at reveal points, a little above the bed, never overlapping each other.
fn place_stings(
    timeline: &mut Timeline,
    library: &MusicLibrary,
    folder: &Path,
    stings: &[(u64, String)],
    bed_lufs: f64,
    at: &dyn Fn(f64) -> u64,
) -> Result<(), MediaError> {
    let total = timeline.duration.0;
    let matches = |t: &MusicTrack, m: &str| {
        t.moods
            .iter()
            .any(|x| m.to_lowercase().contains(&x.to_lowercase()))
    };
    let mut last_end = 0;
    for (start, mood) in stings {
        let (start, mood) = (*start, mood.as_str());
        let mut ranked: Vec<_> = library
            .stings
            .iter()
            .filter(|t| t.lufs.is_finite())
            .collect();
        ranked.sort_by_key(|t| (!matches(t, mood), t.path.clone()));
        let Some(sting) = ranked.first() else { break };
        let path = safe_library_file(folder, &sting.path)?;
        let seconds = retold_providers::media::audio_duration(
            &std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?,
        )?;
        let start = start.max(last_end);
        let len = at(seconds.min(4.0)).min(total.saturating_sub(start));
        if len == 0 {
            continue;
        }
        last_end = start + len;
        timeline.clips.push(Clip {
            track: "A7".into(),
            start: Frames(start),
            duration: Frames(len),
            source: Source::Media { path },
            effects: vec![],
            transition: None,
            // Stings sit a little above the bed so they read as an accent.
            audio_gain_db: Some(bed_lufs + 8.0 - sting.lufs),
        });
    }
    Ok(())
}

/// Narration spans in frames; words within half a second merge so ducking does not pump.
fn voice_intervals(edition: &Edition, fps: retold_core::Fps) -> Vec<(u64, u64)> {
    let frames = |seconds: f64| {
        Seconds::new(seconds)
            .unwrap_or(Seconds::ZERO)
            .to_frames(fps)
    };
    let words: Vec<_> = edition
        .alignment
        .units
        .iter()
        .flat_map(|u| &u.words)
        .map(|word| (frames(word.start), frames(word.end)))
        .collect();
    retold_filmcraft::ducking::merge_intervals(&words, 12)
        .into_iter()
        .map(|(a, b)| (a.0, b.0))
        .collect()
}

fn safe_library_file(folder: &Path, path: &str) -> Result<PathBuf, MediaError> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(MediaError::Quality(
            "library assets must be relative paths inside their folder".into(),
        ));
    }
    Ok(folder.join(path))
}

fn whooshes(
    context: &Context,
    board: &Storyboard,
    edition: &Edition,
    timeline: &mut Timeline,
) -> Result<(), MediaError> {
    if !context.guide.manifest().edit.sfx_on_transitions {
        return Ok(());
    }
    let folder = context
        .channel
        .library_path(&context.channel.file().library.sfx);
    let mut files = match std::fs::read_dir(&folder) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "wav" || e == "mp3"))
            .collect::<Vec<_>>(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(EngineError::io(&folder, e).into()),
    };
    files.sort();
    let Some(path) = files.first() else {
        return Ok(());
    };
    let length = retold_providers::media::audio_duration(
        &std::fs::read(path).map_err(|e| EngineError::io(path, e))?,
    )?;
    let mut previous_end = 0;
    for i in 1..board.units.len() {
        if board.units[i].section == board.units[i - 1].section
            || context
                .guide
                .manifest()
                .structure
                .sections
                .iter()
                .find(|s| s.id.as_str() == board.units[i].section)
                .is_none_or(|s| s.edit.transition.is_none())
        {
            continue;
        }
        let start = Seconds::new((edition.alignment.units[i].start - 0.15).max(0.0))
            .unwrap_or(Seconds::ZERO)
            .to_frames(timeline.fps)
            .0
            .max(previous_end);
        let duration = Seconds::new(length.min(0.6))
            .unwrap_or(Seconds::ZERO)
            .to_frames(timeline.fps)
            .0
            .min(timeline.duration.0.saturating_sub(start));
        if duration == 0 {
            continue;
        }
        timeline.clips.push(Clip {
            track: if context.guide.manifest().narration.mode
                == retold_guide::manifest::NarrationMode::Dialogue
                || crate::voices::local_narrator(context, &edition.variant.narration)
            {
                "A5"
            } else {
                "A3"
            }
            .into(),
            start: Frames(start),
            duration: Frames(duration),
            source: Source::Media { path: path.clone() },
            effects: vec![],
            transition: None,
            audio_gain_db: Some(-12.0),
        });
        previous_end = start + duration;
    }
    Ok(())
}

fn cover_keys(duration: Frames) -> Vec<Key> {
    let mut keys = vec![
        key(0, json!(0.0), Easing::Linear),
        key(3, json!(100.0), Easing::Linear),
    ];
    let hold = duration.0.saturating_sub(COVER_DISSOLVE_FRAMES + 1);
    if hold > 3 && hold < duration.0 - 1 {
        keys.push(key(hold, json!(100.0), Easing::Linear));
    }
    keys.push(key(duration.0 - 1, json!(0.0), Easing::EaseIn));
    keys
}
#[cfg(test)]
mod short_cover_tests {
    #[test]
    fn short_legal_cover_durations_have_ordered_in_bounds_keyframes() {
        for duration in 5..100 {
            let keys = super::cover_keys(retold_core::Frames(duration));
            assert!(keys.windows(2).all(|p| p[0].offset.0 < p[1].offset.0));
            assert!(keys.iter().all(|k| k.offset.0 < duration));
        }
    }
}

#[cfg(test)]
mod title_layout_tests {
    use super::*;
    #[test]
    fn native_scripts_choose_layout_and_vertical_is_cjk_only() {
        assert_eq!(
            title_script("میری کہانی", WritingScript::Latn),
            WritingScript::Arab
        );
        assert_eq!(
            title_script("मेरी कहानी", WritingScript::Latn),
            WritingScript::Deva
        );
        assert_eq!(
            title_script("静かな庭", WritingScript::Latn),
            WritingScript::Jpan
        );
        assert_eq!(
            title_script("安静花园", WritingScript::Latn),
            WritingScript::Hans
        );
        assert_eq!(
            title_script("安靜花園", WritingScript::Hant),
            WritingScript::Hant
        );
        assert_eq!(title_script("庭", WritingScript::Jpan), WritingScript::Jpan);
        let vertical_style = retold_guide::channel::TitleStyle {
            vertical: true,
            size: 90,
            y: 0.15,
            side_margin: 0.074,
        };
        let horizontal_style = retold_guide::channel::TitleStyle {
            vertical: false,
            ..vertical_style
        };
        assert!(title_layout(WritingScript::Arab, &vertical_style).is_err());
        assert_eq!(
            title_layout(WritingScript::Jpan, &vertical_style)
                .unwrap()
                .orientation,
            retold_lipsync::TextOrientation::VerticalTtb
        );
        assert_eq!(
            title_layout(WritingScript::Arab, &horizontal_style)
                .unwrap()
                .orientation,
            retold_lipsync::TextOrientation::HorizontalRtl
        );
    }
    #[test]
    fn tall_nastaliq_metrics_shrink_the_title_above_its_baseline() {
        struct Tall;
        impl retold_lang::fit::Measure for Tall {
            fn width(&self, _: &str, size: f64) -> f64 {
                size * 4.0
            }
            fn vertical_bounds(&self, size: f64) -> (f64, f64) {
                (size * 2.0, size * 0.8)
            }
        }
        let timeline = Timeline {
            name: "ur".into(),
            width: 960,
            height: 720,
            fps: retold_core::Fps::new(24).unwrap(),
            duration: Frames(48),
            clips: vec![],
            video_tracks: None,
            audio_tracks: None,
        };
        let style = retold_guide::channel::TitleStyle {
            vertical: false,
            size: 90,
            y: 0.15,
            side_margin: 0.074,
        };
        let layout = title_layout(WritingScript::Arab, &style).unwrap();
        let size = title_size("میری کہانی", &Tall, &style, &timeline, &layout).unwrap();
        assert!(size >= 24.0 && size * 2.0 + 6.0 < 108.0);
    }

    #[test]
    fn vertical_title_fits_height_and_rejects_unreadable_text() {
        let timeline = Timeline {
            name: "title".into(),
            width: 540,
            height: 960,
            fps: retold_core::Fps::new(24).unwrap(),
            duration: Frames(48),
            clips: vec![],
            video_tracks: None,
            audio_tracks: None,
        };
        let style = retold_guide::channel::TitleStyle {
            vertical: true,
            size: 100,
            y: 0.2,
            side_margin: 0.1,
        };
        let layout = retold_lipsync::TypographyLayout::cjk_vertical();
        let size = title_size(
            "静かな庭",
            &retold_lang::fit::Estimate,
            &style,
            &timeline,
            &layout,
        )
        .unwrap();
        assert!(size * 4.0 <= 960.0 * 0.8 - 12.0);
        assert!(
            title_size(
                &"庭".repeat(100),
                &retold_lang::fit::Estimate,
                &style,
                &timeline,
                &layout
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod delivery_center_tests {
    use super::*;

    #[test]
    fn focus_stays_at_canvas_center_through_zoom_for_either_speaker() {
        for offset in [-226.8, 226.8] {
            let plate = crate::delivery_plates::Plate {
                path: PathBuf::from("plate.mov"),
                canvas: [1080, 1920],
                zoom: 237.037_037,
                offset: [offset, 0.0],
                moving: false,
                content_hash: String::new(),
            };
            let mut clip = Clip {
                track: "V1".into(),
                start: Frames(0),
                duration: Frames(48),
                source: Source::Media {
                    path: plate.path.clone(),
                },
                effects: vec![],
                transition: None,
                audio_gain_db: None,
            };
            native_reframe(&plate, &mut clip, Frames(48));
            assert_eq!(clip.effects.len(), 1);
            let motion = &clip.effects[0];
            assert_eq!(motion.id, "motion");
            let scale = motion
                .animations
                .iter()
                .find(|a| a.param == "scale")
                .unwrap();
            let position = motion
                .animations
                .iter()
                .find(|a| a.param == "position")
                .unwrap();
            for (scale, position) in scale.keys.iter().zip(&position.keys) {
                let zoom = scale.value.as_f64().unwrap() / 100.0;
                let x = position.value[0].as_f64().unwrap() + offset * zoom;
                assert!((x - 540.0).abs() < 1e-8);
                assert_eq!(position.value[1], json!(960.0));
            }
        }
    }
}

#[cfg(test)]
mod score_tests {
    use super::*;

    #[test]
    fn caption_keywords_match_whole_words_once() {
        let keys = vec![
            "Lawrence Wargrave".to_owned(),
            "Wargrave".to_owned(),
            "Vera".to_owned(),
        ];
        assert_eq!(keyword_ranges("SO WARGRAVE SAID", &keys), [(3, 11)]);
        assert_eq!(
            keyword_ranges("Lawrence Wargrave met Vera", &keys),
            [(0, 17), (22, 26)]
        );
        assert_eq!(
            keyword_ranges("Veranda", &keys),
            Vec::<(usize, usize)>::new()
        );
    }

    #[test]
    fn cuts_land_in_pauses_or_just_before_the_line() {
        let t = |start: f64, end: f64| retold_lang::TimedUnit {
            id: String::new(),
            start,
            end,
            words: vec![],
        };
        let units = [t(0.0, 4.0), t(5.0, 8.0), t(8.05, 10.0)];
        assert!((cut_time(&units, 1) - 4.5).abs() < 1e-9);
        assert!((cut_time(&units, 2) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn camera_follows_the_feeling_and_the_guide_wins() {
        let shot = |delivery: serde_json::Value| -> crate::story::Shot {
            serde_json::from_value(json!({"unit":1,"shot":"medium","location":"l","action":"a","mood":"m","characters":[],"focus":[0.5,0.5],"delivery":delivery})).unwrap()
        };
        let dark = shot(json!({"pace":"slow","tone":"dark","pause_before":0.0}));
        let fast = shot(json!({"pace":"fast","tone":"neutral","pause_before":0.0}));
        assert_eq!(
            scene_motion(None, 1, &dark, [0.5, 0.5], &[]),
            (Motion::ZoomIn, 0.7)
        );
        assert_eq!(
            scene_motion(None, 1, &fast, [0.5, 0.5], &[]).0,
            Motion::Punch
        );
        assert_eq!(
            scene_motion(None, 0, &dark, [0.5, 0.5], &[]).0,
            Motion::Punch
        );
        assert_eq!(
            scene_motion(Some(Motion::PanLeft), 1, &dark, [0.5, 0.5], &[]),
            (Motion::PanLeft, 1.0)
        );
        assert_eq!(
            scene_motion(None, 1, &shot(json!(null)), [0.5, 0.5], &[]),
            (Motion::ZoomIn, 1.0)
        );
    }

    #[test]
    fn beds_duck_under_voice_drop_out_in_silence_and_fade_at_crossfades() {
        let voice = [(100, 200)];
        let quiet = [(300, 350)];
        let bed = Bed {
            start: 0,
            len: 400,
            voice: &voice,
            quiet: &quiet,
            duck: -8.0,
            fade: (25, 25),
        };
        assert!((bed.level(0) + 40.0).abs() < 1e-9);
        assert!((bed.level(50)).abs() < 1e-9);
        assert!((bed.level(150) + 8.0).abs() < 1e-9);
        assert!((bed.level(320) + 40.0).abs() < 1e-9);
        assert!((bed.level(399) + 40.0).abs() < 1e-9);
        let keys = bed.keys();
        assert!(keys.windows(2).all(|w| w[0].offset.0 < w[1].offset.0));
        assert!(keys.last().unwrap().offset.0 < 400);
    }

    #[test]
    fn moods_form_stretches_and_short_ones_join_the_previous() {
        let board: Storyboard = serde_json::from_value(json!({
            "plan": {"storyline":"","characters":[],"tone":"calm","open_loop":"","emotional_peak":"","omitted":[],"beats":[]},
            "units": [],
            "direction": {"shots": (0..6).map(|i| json!({"unit":i,"shot":"wide","location":"l","action":"a","mood":"m","characters":[],"focus":[0.5,0.5],
                "score": match i { 2 => json!({"mood":"tense","sting":false,"silence":false}), 3 => json!({"mood":"dark","sting":true,"silence":true}), _ => json!(null) }})).collect::<Vec<_>>()}
        })).unwrap();
        // Units every 5 s; at 1 frame per second for simplicity.
        let timings: Vec<_> = (0..6)
            .map(|i| retold_lang::align::TimedUnit {
                id: i.to_string(),
                start: f64::from(i) * 5.0,
                end: f64::from(i) * 5.0 + 4.0,
                words: vec![],
            })
            .collect();
        let at = |s: f64| num::trunc_u64(s.min(30.0));
        let (stretches, quiet, stings) = music_cues(&board, &timings, &at);
        // "tense" lasts 5 s (< 8) and becomes "dark" in place; calm then dark.
        assert_eq!(
            stretches.iter().map(|s| s.2.as_str()).collect::<Vec<_>>(),
            ["calm", "dark"]
        );
        assert_eq!(
            (stretches[0].1, stretches[1].0, stretches[1].1),
            (10, 10, 30)
        );
        assert_eq!(quiet, [(15, 19)]);
        assert_eq!(stings, [(13, "dark".to_owned())]);
    }
}

#[cfg(test)]
mod sound_tests {
    use super::*;

    #[test]
    fn sounds_match_by_shared_words_and_kind() {
        let library: SoundLibrary = toml::from_str(
            r#"
            [[sounds]]
            path = "rain.wav"
            tags = ["rain", "storm"]
            kind = "ambience"
            lufs = -20.0
            [[sounds]]
            path = "sea.wav"
            tags = ["sea waves", "ocean"]
            kind = "ambience"
            lufs = -20.0
            [[sounds]]
            path = "door.wav"
            tags = ["door creak", "door"]
            kind = "effect"
            lufs = -16.0
        "#,
        )
        .unwrap();
        assert_eq!(
            best_sound(&library, "ambience", "heavy rain").unwrap().path,
            "rain.wav"
        );
        assert_eq!(
            best_sound(&library, "ambience", "Sea waves").unwrap().path,
            "sea.wav"
        );
        assert_eq!(
            best_sound(&library, "effect", "a door creaks open")
                .unwrap()
                .path,
            "door.wav"
        );
        assert!(best_sound(&library, "ambience", "door").is_none());
        assert!(best_sound(&library, "effect", "gunshot").is_none());
    }
}
