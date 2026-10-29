//! Durable execution hooks shared by depth, shots, voices, mouths, delivery and metrics.
//! Feature stages attach here as they ship; disabled features produce no placeholder records.
use std::future::Future;
use std::sync::Arc;

use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{
    EngineError, RunLayout,
    store::{Store, hash},
};

/// Work must commit external assets atomically before returning; keyed finished items are reused.
pub async fn run_stage_step<T, F, Fut>(
    store: &Arc<Mutex<Store>>,
    step: &str,
    inputs: &serde_json::Value,
    cancel: &CancellationToken,
    work: F,
) -> Result<T, EngineError>
where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, EngineError>>,
{
    let stage = crate::store::stage_of(step);
    if !["depth", "shots", "voices", "mouths", "deliver", "metrics"].contains(&stage) {
        return Err(EngineError::Setup("unknown local stage".into()));
    }
    if cancel.is_cancelled() {
        return Err(EngineError::Cancelled);
    }
    let key = hash(
        &serde_json::json!({"version":1,"revision":store.lock().await.revision(step),"inputs":inputs}),
    )?;
    let cached = store.lock().await.cached(step, &key)?;
    if let Some(cached) = cached {
        return Ok(cached);
    }
    store.lock().await.begin(step)?;
    // Adapters receive cancellation themselves. Drain their future before releasing ownership;
    // dropping it here could leave a subprocess or blocking writer alive after a reported stop.
    let result = work().await;
    StageCompletion {
        key,
        result,
        cancelled: cancel.is_cancelled(),
    }
    .settle(store, step)
    .await
}

struct StageCompletion<T> {
    key: String,
    result: Result<T, EngineError>,
    cancelled: bool,
}

impl<T: Serialize> StageCompletion<T> {
    async fn settle(self, store: &Arc<Mutex<Store>>, step: &str) -> Result<T, EngineError> {
        match self.result {
            Ok(value) => {
                store
                    .lock()
                    .await
                    .commit(step, &self.key, &value, Option::default())?;
                if self.cancelled {
                    Err(EngineError::Cancelled)
                } else {
                    Ok(value)
                }
            }
            Err(error) => {
                store.lock().await.task_stopped(step, &error.to_string())?;
                if self.cancelled {
                    Err(EngineError::Cancelled)
                } else {
                    Err(error)
                }
            }
        }
    }
}
pub async fn render_shot(
    layout: &RunLayout,
    shot: &retold_render::Shot,
    item: usize,
    cancel: &CancellationToken,
) -> Result<retold_render::Rendered, EngineError> {
    if item == 0 {
        return Err(EngineError::Setup("shot numbers start at one".into()));
    }
    let mut owned = Store::open(layout)?;
    owned.set_events(Arc::new(crate::events::Events::new(
        &layout.meta().join("events.log"),
        None,
        None,
    )));
    let store = Arc::new(Mutex::new(owned));
    let step = format!("shots-{}", item - 1);
    let result = render_cached(&store, layout, shot, &step, cancel).await;
    match &result {
        Ok(_) => {
            crate::fsx::write_json_atomic(
                &layout.meta().join("shots").join(format!("{item}.json")),
                shot,
            )?;
            store.lock().await.finish()?;
        }
        Err(e) => store.lock().await.fail(&e.to_string())?,
    }
    result
}
async fn render_cached(
    store: &Arc<Mutex<Store>>,
    layout: &RunLayout,
    shot: &retold_render::Shot,
    step: &str,
    cancel: &CancellationToken,
) -> Result<retold_render::Rendered, EngineError> {
    let revision = store.lock().await.revision(step);
    let cache = layout
        .video_dir()
        .join("work/shots")
        .join(format!("revision-{revision}"));
    let inputs = serde_json::json!({"content":retold_render::content_key(shot).map_err(|e|EngineError::Setup(e.to_string()))?});
    let rendered: retold_render::Rendered =
        run_stage_step(store, step, &inputs, cancel, || async {
            retold_render::render(shot, &cache, cancel)
                .await
                .map_err(|e| EngineError::Setup(e.to_string()))
        })
        .await?;
    if std::fs::read(&rendered.path)
        .is_ok_and(|b| blake3::hash(&b).to_hex().as_str() == rendered.clip_hash)
    {
        return Ok(rendered);
    }
    let repaired = retold_render::render(shot, &cache, cancel)
        .await
        .map_err(|e| EngineError::Setup(e.to_string()))?;
    let key = hash(&serde_json::json!({"version":1,"revision":revision,"inputs":inputs}))?;
    store.lock().await.commit(step, &key, &repaired, None)?;
    Ok(repaired)
}
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ShotBinding {
    pub path: std::path::PathBuf,
    pub dimensions: [u32; 2],
    /// The clip has no camera move of its own (lip-sync only); FilmCraft still moves
    /// the camera over it as it would over the still.
    #[serde(default)]
    pub camera: bool,
}
#[must_use]
pub fn bindings_file(
    layout: &RunLayout,
    edition: &crate::languages::Edition,
) -> std::path::PathBuf {
    layout.video_dir().join("work/shots").join(format!(
        "{}.json",
        crate::languages::edition_id(&edition.variant)
    ))
}
/// An explicitly registered shot is rebuilt against current scene artwork and narration duration.
#[expect(
    clippy::too_many_lines,
    reason = "one pass choosing registered, parallax or lip-sync clips per unit"
)]
pub async fn prepare_shots(
    context: &crate::context::Context,
    images: &crate::assets::Images,
    board: &crate::assets::Storyboard,
    edition: &crate::languages::Edition,
) -> Result<(), crate::media_runtime::MediaError> {
    let mut bindings = std::collections::BTreeMap::new();
    let fps = context.guide.manifest().format.fps;
    if edition.alignment.units.len() != images.frames.len() {
        return Err(EngineError::Setup("shot alignment does not cover scene images".into()).into());
    }
    let frames = |s: f64| {
        retold_core::Seconds::new(s)
            .map(|s| s.to_frames(fps))
            .ok_or_else(|| EngineError::Setup("invalid shot timing".into()))
    };
    let total = retold_core::Frames(
        frames(edition.alignment.duration)?.0
            + frames(context.guide.manifest().edit.ending.seconds)?.0,
    );
    for (index, frame) in images.frames.iter().enumerate() {
        let path = context
            .layout
            .meta()
            .join("shots")
            .join(format!("{}.json", index + 1));
        let start = if index == 0 {
            retold_core::Frames(0)
        } else {
            frames(edition.alignment.units[index].start)?
        };
        let end = if index + 1 == images.frames.len() {
            total
        } else {
            frames(edition.alignment.units[index + 1].start)?
        };
        let frames = u32::try_from(
            end.0
                .checked_sub(start.0)
                .ok_or_else(|| EngineError::Setup("invalid shot duration".into()))?,
        )
        .map_err(|e| EngineError::Setup(e.to_string()))?;
        let step = format!(
            "shots-{index}-{}",
            crate::languages::edition_id(&edition.variant)
        );
        #[expect(
            clippy::cast_precision_loss,
            reason = "frame counts are far below 2^52"
        )]
        let start_seconds = start.0 as f64 / f64::from(fps.get());
        let attached =
            crate::mouths::attachments(context, images, edition, index, frames, start_seconds)
                .await?;
        let (rendered_path, camera) = match std::fs::read(&path) {
            Ok(bytes) => {
                let mut shot: retold_render::Shot = serde_json::from_slice(&bytes)
                    .map_err(|e| EngineError::Setup(e.to_string()))?;
                shot.frames = frames;
                shot.fps = fps.get();
                shot.image = frame.image.path.clone();
                shot.attached = attached;
                let rendered = render_cached(
                    &context.media.store,
                    &context.layout,
                    &shot,
                    &step,
                    &context.media.cancel,
                )
                .await?;
                (rendered.path, false)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut moving = None;
                if let Some(mut request) =
                    crate::parallax::request(context, board, frame, frames).await?
                {
                    request.attached.clone_from(&attached);
                    let rendered = render_parallax(context, &request, &step).await?;
                    if rendered.fallback != retold_parallax::render::Fallback::Flat {
                        moving = Some(rendered.path);
                    }
                }
                match moving {
                    Some(path) => (path, false),
                    None if !attached.is_empty() => {
                        let shot = retold_render::Shot {
                            image: frame.image.path.clone(),
                            depth: None,
                            frames,
                            fps: fps.get(),
                            motion_x: 0.0,
                            motion_y: 0.0,
                            sprites: vec![],
                            attached,
                        };
                        let rendered = render_cached(
                            &context.media.store,
                            &context.layout,
                            &shot,
                            &step,
                            &context.media.cancel,
                        )
                        .await?;
                        (rendered.path, true)
                    }
                    None => continue,
                }
            }
            Err(e) => return Err(EngineError::io(&path, e).into()),
        };
        let (width, height) = image::ImageReader::open(&frame.image.path)
            .and_then(image::ImageReader::with_guessed_format)
            .map_err(|e| EngineError::io(&frame.image.path, e))?
            .into_dimensions()
            .map_err(|e| EngineError::Setup(e.to_string()))?;
        bindings.insert(
            index,
            ShotBinding {
                path: rendered_path,
                dimensions: [width, height],
                camera,
            },
        );
    }
    crate::fsx::write_json_atomic(&bindings_file(&context.layout, edition), &bindings)?;
    Ok(())
}

async fn render_parallax(
    context: &crate::context::Context,
    request: &retold_parallax::render::Request,
    step: &str,
) -> Result<retold_parallax::render::Rendered, EngineError> {
    let revision = context.media.store.lock().await.revision(step);
    let cache = context
        .layout
        .video_dir()
        .join("work/parallax-shots")
        .join(format!("revision-{revision}"));
    let inputs = serde_json::json!({"content":retold_parallax::render::content_key(request).map_err(|e|EngineError::Setup(e.to_string()))?});
    let rendered: retold_parallax::render::Rendered = run_stage_step(
        &context.media.store,
        step,
        &inputs,
        &context.media.cancel,
        || async {
            let rendered = retold_parallax::render::render(request, &cache, &context.media.cancel)
                .await
                .map_err(|e| EngineError::Setup(e.to_string()))?;
            context.media.store.lock().await.emit(
                step,
                crate::events::Status::Running,
                None,
                Some(format!(
                    "parallax {:?}, strength {:.3}, holes {:.1}%, stretched {:.2}%",
                    rendered.fallback,
                    rendered.applied_strength,
                    rendered.audit.hole_ratio * 100.0,
                    rendered.audit.stretched_fraction * 100.0
                )),
            )?;
            Ok(rendered)
        },
    )
    .await?;
    let result = if std::fs::read(&rendered.path)
        .is_ok_and(|b| blake3::hash(&b).to_hex().as_str() == rendered.clip_hash)
    {
        rendered
    } else {
        let repaired = retold_parallax::render::render(request, &cache, &context.media.cancel)
            .await
            .map_err(|e| EngineError::Setup(e.to_string()))?;
        let key = hash(&serde_json::json!({"version":1,"revision":revision,"inputs":inputs}))?;
        context
            .media
            .store
            .lock()
            .await
            .commit(step, &key, &repaired, None)?;
        repaired
    };
    crate::fsx::write_json_atomic(
        &context
            .layout
            .video_dir()
            .join("work/parallax-shots")
            .join(format!("{step}.audit.json")),
        &result,
    )?;
    Ok(result)
}
