//! Frame-exact editor-independent timeline data. Assembly and validation do not open an
//! editor, render a frame or export a video.

use std::collections::BTreeMap;
use std::path::PathBuf;

use retold_core::{Fps, Frames};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timeline {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: Fps,
    pub duration: Frames,
    pub clips: Vec<Clip>,
    #[serde(default)]
    pub video_tracks: Option<u32>,
    #[serde(default)]
    pub audio_tracks: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub track: String,
    pub start: Frames,
    pub duration: Frames,
    pub source: Source,
    pub effects: Vec<Effect>,
    pub transition: Option<Transition>,
    pub audio_gain_db: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Media {
        path: PathBuf,
    },
    Adjustment,
    Text {
        text: String,
        font: String,
        font_style: String,
        size: f64,
        position: [f64; 2],
        props: Value,
        highlight: Option<Highlight>,
        /// Ranges styled for the whole clip (names, key words); `highlight` is applied
        /// after them, so the spoken word wins where they overlap.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        emphasis: Vec<Highlight>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Highlight {
    pub start: usize,
    pub end: usize,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub id: String,
    pub params: BTreeMap<String, Value>,
    pub animations: Vec<Animation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Animation {
    pub param: String,
    pub keys: Vec<Key>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Key {
    pub offset: Frames,
    pub value: Value,
    pub easing: Easing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub effect: String,
    pub frames: u32,
}

impl Timeline {
    /// Creates a new timeline with default track calculations.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        width: u32,
        height: u32,
        fps: Fps,
        duration: Frames,
    ) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            fps,
            duration,
            clips: Vec::new(),
            video_tracks: None,
            audio_tracks: None,
        }
    }

    /// Dynamically computes the required video track count from clip tracks or explicit override.
    #[must_use]
    pub fn required_video_tracks(&self) -> u32 {
        let max_clip = self
            .clips
            .iter()
            .filter(|c| c.track.starts_with('V'))
            .filter_map(|c| c.track[1..].parse::<u32>().ok())
            .max()
            .unwrap_or(1);
        self.video_tracks.unwrap_or(max_clip).max(max_clip).max(1)
    }

    /// Dynamically computes the required audio track count from clip tracks or explicit override.
    #[must_use]
    pub fn required_audio_tracks(&self) -> u32 {
        let max_clip = self
            .clips
            .iter()
            .filter(|c| c.track.starts_with('A'))
            .filter_map(|c| c.track[1..].parse::<u32>().ok())
            .max()
            .unwrap_or(1);
        self.audio_tracks.unwrap_or(max_clip).max(max_clip).max(1)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0
            || self.height == 0
            || self.duration.0 == 0
            || self.duration.0 > 10_000_000
        {
            return Err("timeline needs valid dimensions and duration".into());
        }
        let mut tracks: BTreeMap<&str, Vec<&Clip>> = BTreeMap::new();
        for clip in &self.clips {
            if clip.duration.0 == 0
                || clip.start.0.saturating_add(clip.duration.0) > self.duration.0
            {
                return Err("clip lies outside the sequence".into());
            }
            if !clip.track.starts_with(['V', 'A']) || clip.track[1..].parse::<u32>().is_err() {
                return Err("invalid track name".into());
            }
            tracks.entry(&clip.track).or_default().push(clip);
            if let Source::Text {
                text,
                size,
                position,
                highlight,
                emphasis,
                ..
            } = &clip.source
            {
                if text.trim().is_empty()
                    || !size.is_finite()
                    || *size <= 0.0
                    || position.iter().any(|p| !p.is_finite())
                {
                    return Err("invalid text geometry".into());
                }
                if highlight
                    .iter()
                    .chain(emphasis)
                    .any(|h| h.start >= h.end || h.end > text.chars().count())
                {
                    return Err("highlight range is outside the text".into());
                }
            }
            for effect in &clip.effects {
                for animation in &effect.animations {
                    if animation.keys.is_empty()
                        || animation
                            .keys
                            .windows(2)
                            .any(|w| w[0].offset.0 >= w[1].offset.0)
                        || animation.keys.iter().any(|k| k.offset.0 >= clip.duration.0)
                    {
                        return Err("keyframes need ordered offsets inside their clip".into());
                    }
                }
            }
            if clip
                .transition
                .as_ref()
                .is_some_and(|t| t.frames == 0 || u64::from(t.frames) >= clip.duration.0)
            {
                return Err("transition does not fit its clip".into());
            }
        }
        for clips in tracks.values_mut() {
            clips.sort_by_key(|c| c.start.0);
            if clips
                .windows(2)
                .any(|w| w[0].start.0 + w[0].duration.0 > w[1].start.0)
            {
                return Err("clips overlap on one track".into());
            }
        }
        if let Some(images) = tracks.get("V1") {
            if images.first().is_none_or(|c| c.start.0 != 0)
                || images
                    .last()
                    .is_none_or(|c| c.start.0 + c.duration.0 != self.duration.0)
                || images
                    .windows(2)
                    .any(|w| w[0].start.0 + w[0].duration.0 != w[1].start.0)
            {
                return Err("picture track contains a gap".into());
            }
        } else {
            return Err("timeline has no picture track".into());
        }
        Ok(())
    }
}

/// Cover crop at an image focus point, clamped so no edge of the frame is uncovered.
#[must_use]
pub fn crop_position(image: [u32; 2], canvas: [u32; 2], scale: f64, focus: [f64; 2]) -> [f64; 2] {
    let dimensions = [
        f64::from(image[0]) * scale / 100.0,
        f64::from(image[1]) * scale / 100.0,
    ];
    let center = [f64::from(canvas[0]) / 2.0, f64::from(canvas[1]) / 2.0];
    std::array::from_fn(|i| {
        let margin = ((dimensions[i] - f64::from(canvas[i])) / 2.0).max(0.0);
        (center[i] - (focus[i] - 0.5) * dimensions[i]).clamp(center[i] - margin, center[i] + margin)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crop_clamps_extreme_focus_without_uncovering_the_frame() {
        let image = [1200, 2000];
        let canvas = [1080, 1920];
        for focus in [[0.0, 0.0], [1.0, 1.0], [0.3, 0.5]] {
            let scale = 110.0;
            let p = crop_position(image, canvas, scale, focus);
            for i in 0..2 {
                let half = f64::from(image[i]) * scale / 200.0;
                assert!(p[i] - half <= 0.001);
                assert!(p[i] + half >= f64::from(canvas[i]) - 0.001);
            }
        }
    }
    #[test]
    fn gaps_and_overlaps_fail_before_any_editor_command() {
        let clip = Clip {
            track: "V1".into(),
            start: Frames(0),
            duration: Frames(90),
            source: Source::Media {
                path: "still.png".into(),
            },
            effects: vec![],
            transition: None,
            audio_gain_db: None,
        };
        let mut t = Timeline {
            name: "test".into(),
            width: 1080,
            height: 1920,
            fps: Fps::new(30).unwrap(),
            duration: Frames(90),
            clips: vec![clip.clone()],
            video_tracks: None,
            audio_tracks: None,
        };
        t.validate().unwrap();
        t.duration = Frames(91);
        assert!(t.validate().is_err());
        t.duration = Frames(90);
        t.clips.push(clip);
        assert!(t.validate().is_err());
    }
}
