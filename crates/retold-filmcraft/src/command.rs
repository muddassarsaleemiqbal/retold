//! The verified command signatures in docs/filmcraft-commands.json.

use std::path::PathBuf;

use retold_core::{Fps, Ticks};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub enum Command {
    List,
    Chapter {
        time: Ticks,
        name: String,
    },
    NewProject {
        name: String,
    },
    NewSequence {
        name: String,
        width: u32,
        height: u32,
        fps: Fps,
        video_tracks: u32,
        audio_tracks: u32,
    },
    OpenSequence {
        item: Value,
    },
    Import {
        paths: Vec<PathBuf>,
    },
    ImportImageSequence {
        path: PathBuf,
    },
    Place {
        item: Value,
        track: String,
        time: Ticks,
        duration: Ticks,
    },
    ApplyEffect {
        clips: Vec<Value>,
        effect: String,
    },
    SetParam {
        clip: Value,
        effect: String,
        param: String,
        value: Value,
        time: Option<Ticks>,
    },
    Animate {
        clip: Value,
        effect: String,
        param: String,
    },
    Interpolate {
        clip: Value,
        effect: String,
        param: String,
        media_time: Ticks,
        interpolation: Interpolation,
    },
    Transition {
        clip: Value,
        effect: String,
        frames: u32,
    },
    Graphics(GraphicsCommand),
    AudioGain {
        clips: Vec<Value>,
        db: f64,
    },
    NewAdjustment {
        seconds: f64,
    },
    InspectProject,
    InspectSequence,
    Fonts,
    Jobs,
    OpenProject {
        path: PathBuf,
    },
    SaveAs {
        path: PathBuf,
    },
    Save,
    Playhead {
        seconds: f64,
    },
    ExportFrame {
        path: PathBuf,
    },
    ExportAudio {
        path: PathBuf,
        sequence: Value,
        loudness_lufs: f64,
    },
    ExportMedia {
        loudness_lufs: f64,
        path: PathBuf,
        sequence: Value,
        width: u32,
        height: u32,
        fps: Fps,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum Interpolation {
    Linear,
    EaseIn,
    EaseOut,
    Hold,
}

impl Command {
    /// All translations are pure and testable; media export is identified separately so
    /// the client can enforce the permission boundary before writing to the socket.
    fn effect_wire(&self) -> Option<(&'static str, Value)> {
        match self {
            Self::ApplyEffect { clips, effect } => {
                Some(("effects.apply", json!({"clips": clips, "effect": effect})))
            }
            Self::SetParam {
                clip,
                effect,
                param,
                value,
                time,
            } => {
                let mut params =
                    json!({"clip": clip, "effect": effect, "param": param, "value": value});
                if let Some(t) = time {
                    params["time"] = json!(t);
                }
                Some(("effects.setParam", params))
            }
            Self::Animate {
                clip,
                effect,
                param,
            } => Some((
                "effects.toggleAnimation",
                json!({"clip": clip, "effect": effect, "param": param}),
            )),
            Self::Interpolate {
                clip,
                effect,
                param,
                media_time,
                interpolation,
            } => Some((
                "effects.setInterpolation",
                json!({
                    "clip": clip,
                    "effect": effect,
                    "param": param,
                    "mediaTime": media_time,
                    "interpolation": match interpolation {
                        Interpolation::Linear => "linear",
                        Interpolation::EaseIn => "easeIn",
                        Interpolation::EaseOut => "easeOut",
                        Interpolation::Hold => "hold",
                    }
                }),
            )),
            Self::Transition {
                clip,
                effect,
                frames,
            } => Some((
                "sequence.applyVideoTransition",
                json!({"clip": clip, "effect": effect, "frames": frames, "edge": "start"}),
            )),
            _ => None,
        }
    }

    #[must_use]
    pub fn wire(&self) -> (&'static str, Value) {
        if let Some(res) = self.effect_wire() {
            return res;
        }
        match self {
            Self::List => ("command.list", json!({})),
            Self::Chapter { time, name } => {
                ("markers.addChapter", json!({"time":time,"name":name}))
            }
            Self::NewProject { name } => ("file.newProject", json!({"name":name})),
            Self::NewSequence {
                name,
                width,
                height,
                fps,
                video_tracks,
                audio_tracks,
            } => (
                "file.newSequence",
                json!({
                    "name": name,
                    "width": width,
                    "height": height,
                    "fps": fps.get(),
                    "video": video_tracks,
                    "audio": audio_tracks,
                }),
            ),
            Self::OpenSequence { item } => ("sequence.open", json!({"item":item})),
            Self::Import { paths } => ("file.import", json!({"paths":paths})),
            Self::ImportImageSequence { path } => {
                ("file.importImageSequence", json!({"path":path}))
            }
            Self::Place {
                item,
                track,
                time,
                duration,
            } => (
                "timeline.place",
                json!({"item":item,"track":track,"time":time,"duration":duration,"insert":false}),
            ),
            Self::Graphics(command) => command.wire(),
            Self::ApplyEffect { .. }
            | Self::SetParam { .. }
            | Self::Animate { .. }
            | Self::Interpolate { .. }
            | Self::Transition { .. } => unreachable!(),
            Self::AudioGain { clips, db } => (
                "clip.audioGain",
                json!({"clips":clips,"mode":"set","db":db}),
            ),
            Self::NewAdjustment { seconds } => {
                ("file.newAdjustmentLayer", json!({"seconds":seconds}))
            }
            Self::InspectProject => ("project.inspect", json!({})),
            Self::InspectSequence => ("sequence.inspect", json!({})),
            Self::Fonts => ("fonts.list", json!({"system":true})),
            Self::Jobs => ("jobs.list", json!({})),
            Self::OpenProject { path } => ("file.open", json!({"path":path})),
            Self::SaveAs { path } => ("file.saveAs", json!({"path":path})),
            Self::Save => ("file.save", json!({})),
            Self::Playhead { seconds } => ("playhead.set", json!({"seconds":seconds})),
            Self::ExportFrame { path } => ("file.exportFrame", json!({"path":path,"format":"png"})),
            Self::ExportAudio { .. } | Self::ExportMedia { .. } => self.export_wire(),
        }
    }
    fn export_wire(&self) -> (&'static str, Value) {
        match self {
            Self::ExportAudio {
                path,
                sequence,
                loudness_lufs,
            } => (
                "file.exportMedia",
                json!({"path":path,"sequence":sequence,"format":"wav","loudnessLufs":loudness_lufs,"wait":false}),
            ),
            Self::ExportMedia {
                loudness_lufs,
                path,
                sequence,
                width,
                height,
                fps,
            } => (
                "file.exportMedia",
                json!({"path":path,"sequence":sequence,"format":"h264","width":width,"height":height,"fps":fps.get(),"bitrateKbps":10000,"bitrateMode":"vbr1Pass","loudnessLufs":loudness_lufs,"wait":false}),
            ),
            _ => unreachable!("export_wire is called only for export commands"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum GraphicsCommand {
    NewText {
        vertical: bool,
        text: String,
        size: f64,
        seconds: f64,
        track: u32,
        time: Ticks,
        position: [f64; 2],
        font: String,
        font_style: String,
    },
    GraphicProps {
        clip: Value,
        props: Value,
    },
    CharStyle {
        clip: Value,
        start: usize,
        end: usize,
        color: String,
    },
}

impl GraphicsCommand {
    #[must_use]
    pub fn wire(&self) -> (&'static str, Value) {
        match self {
            Self::NewText {
                vertical,
                text,
                size,
                seconds,
                track,
                time,
                position,
                font,
                font_style,
            } => (
                "graphics.newText",
                json!({"text":text,"vertical":vertical,"size":size,"seconds":seconds,"track":track,"time":time,"position":position,"font":font,"fontStyle":font_style}),
            ),
            Self::GraphicProps { clip, props } => {
                ("graphics.set", json!({"clip":clip,"layer":0,"props":props}))
            }
            Self::CharStyle {
                clip,
                start,
                end,
                color,
            } => (
                "graphics.setCharStyle",
                json!({"clip":clip,"layer":0,"start":start,"end":end,"style":{"color":color}}),
            ),
        }
    }
}
