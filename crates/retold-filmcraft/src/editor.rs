//! Applying a validated timeline. Video export is a separate, explicitly approved call.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::command::{GraphicsCommand, Interpolation};
use crate::timeline::{Animation, Clip, Easing, Source, Timeline};
use crate::{Client, Command, Error};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuiltSequence {
    pub sequence: Value,
    pub clip_ids: Vec<Value>,
    pub duration_frames: u64,
}

pub async fn build(
    client: &Client,
    timeline: &Timeline,
    cancel: &CancellationToken,
) -> Result<BuiltSequence, Error> {
    timeline.validate().map_err(Error::Protocol)?;
    let registry = client.send_command(&Command::List, cancel).await?;
    verify_registry(&registry)?;
    let sequence = client
        .send_command(
            &Command::NewSequence {
                name: timeline.name.clone(),
                width: timeline.width,
                height: timeline.height,
                fps: timeline.fps,
                video_tracks: timeline.required_video_tracks(),
                audio_tracks: timeline.required_audio_tracks(),
            },
            cancel,
        )
        .await?;
    let sequence = sequence
        .get("sequence")
        .or_else(|| sequence.get("item"))
        .cloned()
        .ok_or_else(|| Error::Protocol("newSequence returned no sequence id".into()))?;
    client
        .send_command(
            &Command::OpenSequence {
                item: sequence.clone(),
            },
            cancel,
        )
        .await?;
    let paths: std::collections::BTreeSet<_> = timeline
        .clips
        .iter()
        .filter_map(|c| {
            if let Source::Media { path } = &c.source {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect();
    let mut media = BTreeMap::new();
    for path in paths {
        let response = client
            .send_command(
                &Command::Import {
                    paths: vec![path.clone()],
                },
                cancel,
            )
            .await?;
        if response["errors"].as_array().is_some_and(|e| !e.is_empty()) {
            return Err(Error::Protocol(format!(
                "import failed: {}",
                response["errors"]
            )));
        }
        let item = response["items"]
            .as_array()
            .and_then(|a| a.first())
            .cloned()
            .ok_or_else(|| Error::Protocol("import returned no media id".into()))?;
        media.insert(path, item);
    }
    let mut clips = Vec::new();
    for clip in &timeline.clips {
        let id = place(client, timeline, clip, &media, cancel).await?;
        apply_clip(client, timeline, clip, &id, cancel).await?;
        clips.push(id);
    }
    let readback = client
        .send_command(&Command::InspectSequence, cancel)
        .await?;
    verify_readback(&readback, &clips)?;
    Ok(BuiltSequence {
        sequence,
        clip_ids: clips,
        duration_frames: timeline.duration.0,
    })
}

async fn apply_clip(
    client: &Client,
    timeline: &Timeline,
    clip: &Clip,
    id: &Value,
    cancel: &CancellationToken,
) -> Result<(), Error> {
    for effect in &clip.effects {
        if !["motion", "opacity", "volume"].contains(&effect.id.as_str()) {
            client
                .send_command(
                    &Command::ApplyEffect {
                        clips: vec![id.clone()],
                        effect: effect.id.clone(),
                    },
                    cancel,
                )
                .await?;
        }
        for (param, value) in &effect.params {
            client
                .send_command(
                    &Command::SetParam {
                        clip: id.clone(),
                        effect: effect.id.clone(),
                        param: param.clone(),
                        value: value.clone(),
                        time: None,
                    },
                    cancel,
                )
                .await?;
        }
        apply_animations(
            client,
            timeline,
            clip,
            id,
            &effect.id,
            &effect.animations,
            cancel,
        )
        .await?;
    }
    if let Some(transition) = &clip.transition {
        client
            .send_command(
                &Command::Transition {
                    clip: id.clone(),
                    effect: transition.effect.clone(),
                    frames: transition.frames,
                },
                cancel,
            )
            .await?;
    }
    if let Some(db) = clip.audio_gain_db {
        client
            .send_command(
                &Command::AudioGain {
                    clips: vec![id.clone()],
                    db,
                },
                cancel,
            )
            .await?;
    }
    Ok(())
}

async fn apply_animations(
    client: &Client,
    timeline: &Timeline,
    clip: &Clip,
    id: &Value,
    effect_id: &str,
    animations: &[Animation],
    cancel: &CancellationToken,
) -> Result<(), Error> {
    for animation in animations {
        client
            .send_command(
                &Command::Animate {
                    clip: id.clone(),
                    effect: effect_id.to_string(),
                    param: animation.param.clone(),
                },
                cancel,
            )
            .await?;
        for key in &animation.keys {
            client
                .send_command(
                    &Command::SetParam {
                        clip: id.clone(),
                        effect: effect_id.to_string(),
                        param: animation.param.clone(),
                        value: key.value.clone(),
                        time: Some(
                            retold_core::Frames(clip.start.0 + key.offset.0).to_ticks(timeline.fps),
                        ),
                    },
                    cancel,
                )
                .await?;
            let interpolation = match key.easing {
                Easing::Linear => Interpolation::Linear,
                Easing::EaseIn => Interpolation::EaseIn,
                Easing::EaseOut => Interpolation::EaseOut,
                Easing::Hold => Interpolation::Hold,
            };
            client
                .send_command(
                    &Command::Interpolate {
                        clip: id.clone(),
                        effect: effect_id.to_string(),
                        param: animation.param.clone(),
                        media_time: key.offset.to_ticks(timeline.fps),
                        interpolation,
                    },
                    cancel,
                )
                .await?;
        }
    }
    Ok(())
}

async fn place(
    client: &Client,
    timeline: &Timeline,
    clip: &Clip,
    media: &BTreeMap<std::path::PathBuf, Value>,
    cancel: &CancellationToken,
) -> Result<Value, Error> {
    let response = match &clip.source {
        Source::Media { path } => {
            let item = media
                .get(path)
                .cloned()
                .ok_or_else(|| Error::Protocol("media was not imported".into()))?;
            client
                .send_command(
                    &Command::Place {
                        item,
                        track: clip.track.clone(),
                        time: clip.start.to_ticks(timeline.fps),
                        duration: clip.duration.to_ticks(timeline.fps),
                    },
                    cancel,
                )
                .await?
        }
        Source::Adjustment => {
            let item = client
                .send_command(
                    &Command::NewAdjustment {
                        seconds: clip.duration.to_seconds(timeline.fps).get(),
                    },
                    cancel,
                )
                .await?;
            let item = item
                .get("item")
                .cloned()
                .ok_or_else(|| Error::Protocol("adjustment layer has no id".into()))?;
            client
                .send_command(
                    &Command::Place {
                        item,
                        track: clip.track.clone(),
                        time: clip.start.to_ticks(timeline.fps),
                        duration: clip.duration.to_ticks(timeline.fps),
                    },
                    cancel,
                )
                .await?
        }
        Source::Text { .. } => return place_text(client, timeline, clip, cancel).await,
    };
    response["clips"]
        .as_array()
        .and_then(|a| a.first())
        .cloned()
        .ok_or_else(|| Error::Protocol("timeline.place returned no clip".into()))
}

async fn place_text(
    client: &Client,
    timeline: &Timeline,
    clip: &Clip,
    cancel: &CancellationToken,
) -> Result<Value, Error> {
    let Source::Text {
        text,
        font,
        font_style,
        size,
        position,
        props,
        highlight,
        emphasis,
    } = &clip.source
    else {
        return Err(Error::Protocol("expected text source".into()));
    };
    let track = clip.track[1..]
        .parse::<u32>()
        .map_err(|_| Error::Protocol("invalid text track".into()))?;
    let response = client
        .send_command(
            &Command::Graphics(GraphicsCommand::NewText {
                vertical: props
                    .get("vertical")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                text: text.clone(),
                font: font.clone(),
                font_style: font_style.clone(),
                size: *size,
                position: *position,
                seconds: clip.duration.to_seconds(timeline.fps).get(),
                track: track
                    .checked_sub(1)
                    .ok_or_else(|| Error::Protocol("text track must start at V1".into()))?,
                time: clip.start.to_ticks(timeline.fps),
            }),
            cancel,
        )
        .await?;
    let id = response
        .get("clip")
        .cloned()
        .ok_or_else(|| Error::Protocol("text has no clip id".into()))?;
    let mut props = props.clone();
    props["font"] = json!(font);
    props["font_style"] = json!(font_style);
    props["size"] = json!(size);
    client
        .send_command(
            &Command::Graphics(GraphicsCommand::GraphicProps {
                clip: id.clone(),
                props,
            }),
            cancel,
        )
        .await?;
    for h in emphasis.iter().chain(highlight) {
        client
            .send_command(
                &Command::Graphics(GraphicsCommand::CharStyle {
                    clip: id.clone(),
                    start: h.start,
                    end: h.end,
                    color: h.color.clone(),
                }),
                cancel,
            )
            .await?;
    }
    Ok(id)
}

pub fn verify_registry(registry: &Value) -> Result<(), Error> {
    let text = registry.to_string();
    for required in [
        "file.newSequence",
        "file.import",
        "timeline.place",
        "effects.setParam",
        "effects.toggleAnimation",
        "graphics.newText",
        "graphics.setCharStyle",
        "file.saveAs",
        "file.exportMedia",
        "jobs.list",
        "sequence.inspect",
    ] {
        if !text.contains(required) {
            return Err(Error::Protocol(format!(
                "installed FilmCraft is missing `{required}`"
            )));
        }
    }
    Ok(())
}

fn verify_readback(readback: &Value, clips: &[Value]) -> Result<(), Error> {
    fn ids(value: &Value, out: &mut Vec<Value>) {
        match value {
            Value::Object(object) => {
                if let Some(id) = object.get("clip").or_else(|| object.get("id")) {
                    out.push(id.clone());
                }
                for child in object.values() {
                    ids(child, out);
                }
            }
            Value::Array(items) => {
                for item in items {
                    ids(item, out);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    ids(readback, &mut found);
    if clips.iter().any(|clip| !found.contains(clip)) {
        return Err(Error::Protocol(
            "sequence readback did not contain all placed clips".into(),
        ));
    }
    Ok(())
}

/// Called only after explicit video approval. Completion must come from job status and
/// a nonempty output file; an unchanged file size is never treated as successful export.
pub async fn export_approved(
    client: &Client,
    timeline: &Timeline,
    built: &BuiltSequence,
    path: &Path,
    cancel: &CancellationToken,
) -> Result<Value, Error> {
    let response = client
        .export_approved(
            &Command::ExportMedia {
                loudness_lufs: -14.0,
                path: path.into(),
                sequence: built.sequence.clone(),
                width: timeline.width,
                height: timeline.height,
                fps: timeline.fps,
            },
            cancel,
        )
        .await?;
    let job = response
        .get("job")
        .cloned()
        .ok_or_else(|| Error::Protocol("export returned no job id".into()))?;
    wait_job(client, &job, cancel).await?;
    if !std::fs::metadata(path).is_ok_and(|m| m.len() > 0) {
        return Err(Error::Protocol(
            "export job finished without a nonempty output file".into(),
        ));
    }
    Ok(job)
}

pub async fn wait_job(
    client: &Client,
    job: &Value,
    cancel: &CancellationToken,
) -> Result<(), Error> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3600);
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        let response = client.send_command(&Command::Jobs, cancel).await?;
        let jobs = response.get("jobs").unwrap_or(&response);
        if let Some(entry) = jobs.as_array().and_then(|a| {
            a.iter().find(|j| {
                j["id"] == *job
                    || j["id"].to_string().trim_matches('"') == job.to_string().trim_matches('"')
            })
        }) {
            if entry["finished"].as_bool() == Some(true) {
                let result = &entry["result"];
                if result["path"].is_string()
                    && result["bytes"].as_u64().is_some_and(|n| n > 0)
                    && result["frames"].as_u64().is_some_and(|n| n > 0)
                {
                    return Ok(());
                }
                return Err(Error::Rejected {
                    command: "file.exportMedia".into(),
                    message: format!("finished export has no successful result: {entry}"),
                });
            }
            let state = entry["state"]
                .as_str()
                .or_else(|| entry["status"].as_str())
                .unwrap_or_default()
                .to_lowercase();
            if ["failed", "error", "cancelled", "canceled"].contains(&state.as_str()) {
                return Err(Error::Rejected {
                    command: "file.exportMedia".into(),
                    message: entry.to_string(),
                });
            }
            if [
                "done",
                "complete",
                "completed",
                "finished",
                "succeeded",
                "success",
            ]
            .contains(&state.as_str())
            {
                return Ok(());
            }
        }
        tokio::select! { ()=cancel.cancelled()=>return Err(Error::Cancelled), ()=tokio::time::sleep(Duration::from_secs(2))=>{} }
    }
}
