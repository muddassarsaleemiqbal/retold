//! Full workflow coordination. Ready writes reviewable timeline/packaging artifacts;
//! a separate video permission is required before opening the editor or exporting.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use retold_filmcraft::timeline::Timeline;
use retold_filmcraft::{Command, editor, session::Session};
use serde::{Deserialize, Serialize};

use crate::assets::Storyboard;
use crate::context::Context;
use crate::languages::{Edition, edition_id};
use crate::media_runtime::MediaError;
use crate::pipeline::{Pipeline, PipelineError, Until};
use crate::store::hash;
use crate::{EngineError, Stage, fsx};

/// Native editor command used when a session supplies no explicit executable.
#[must_use]
pub const fn default_editor() -> &'static str {
    if cfg!(target_os = "macos") {
        "/Applications/FilmCraft.app/Contents/MacOS/FilmCraft"
    } else if cfg!(target_os = "windows") {
        "filmcraft.exe"
    } else {
        "filmcraft"
    }
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub allow_video: bool,
    /// Stop after saving the editor project; render nothing.
    pub project_only: bool,
    pub filmcraft_binary: Option<PathBuf>,
    pub fonts: Option<Arc<dyn crate::assemble::FontResolver>>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error(transparent)]
    Text(#[from] PipelineError),
    #[error(transparent)]
    Media(#[from] MediaError),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Editor(#[from] retold_filmcraft::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub title: String,
    pub editions: Vec<Delivery>,
    pub video_approved: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    #[serde(default)]
    pub edition: String,
    #[serde(default)]
    pub aspect: String,
    #[serde(default)]
    pub directory: PathBuf,
    pub id: String,
    pub timeline: PathBuf,
    pub video: PathBuf,
    pub thumbnail: PathBuf,
    #[serde(default)]
    pub thumbnail_candidates: Vec<PathBuf>,
    pub subtitles: Vec<PathBuf>,
    pub posts: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectRecord {
    key: String,
    project_hash: String,
    sequences: BTreeMap<String, editor::BuiltSequence>,
    #[serde(default)]
    timeline_keys: BTreeMap<String, String>,
}

pub async fn run(
    context: Arc<Context>,
    until: Until,
    options: &Options,
) -> Result<(), WorkflowError> {
    let result = Box::pin(run_workflow_steps(Arc::clone(&context), until, options)).await;
    let mut store = context.media.store.lock().await;
    match &result {
        Ok(()) => store.finish()?,
        Err(e) => store.fail(&e.to_string())?,
    }
    fsx::write_json_atomic(
        &context.layout.cost_file(),
        &serde_json::json!({"charged_or_reserved":context.media.ledger.total().map_err(MediaError::from)?,"budget":context.meta.budget,"requests":context.media.ledger.entries().map_err(MediaError::from)?,"text_receipts":store.state.receipts}),
    )?;
    result
}

/// Builds one timeline per language edition off the async runtime (font shaping is blocking).
async fn assemble_timelines(
    context: &Arc<Context>,
    board: &Arc<Storyboard>,
    images: &crate::assets::Images,
    editions: &[Edition],
    options: &Options,
) -> Result<Vec<Timeline>, WorkflowError> {
    let mut timelines = Vec::new();
    for edition in editions {
        crate::local_stages::prepare_shots(context, images, board, edition).await?;
        for (name, aspect) in crate::delivery::formats(context, edition)? {
            let context = Arc::new(if let Some(aspect) = aspect {
                crate::delivery::aspect_context(context, aspect)?
            } else {
                (**context).clone()
            });
            if aspect.is_some() {
                crate::delivery_plates::prepare(&context, images, edition, &name).await?;
            }
            let board = Arc::clone(board);
            let images = images.clone();
            let edition = edition.clone();
            let fonts = options.fonts.clone();
            let timeline = tokio::task::spawn_blocking(move || {
                let native = crate::assemble::NativeFonts(retold_lang::fit::Fonts::system());
                crate::assemble::assemble_named_with_fonts(
                    &context,
                    &board,
                    &images,
                    &edition,
                    fonts.as_deref().unwrap_or(&native),
                    &name,
                )
            })
            .await
            .map_err(|e| EngineError::Setup(e.to_string()))??;
            timelines.push(timeline);
        }
    }
    Ok(timelines)
}

async fn produce_media(
    context: &Arc<Context>,
    board: &Arc<Storyboard>,
    until: Until,
) -> Result<(crate::assets::Images, Vec<Edition>), WorkflowError> {
    let images = Box::pin(crate::assets::images(
        Arc::clone(context),
        Arc::clone(board),
    ));
    let editions = crate::languages::languages(Arc::clone(context), Arc::clone(board), until);
    let images =
        crate::events::observe_branch(images, async || context.branch_failed("images", None).await);
    let editions = crate::events::observe_branch(editions, async || {
        context.branch_failed("languages", None).await
    });
    let (images, editions) = tokio::join!(images, editions);
    if images.is_err() || editions.is_err() {
        fsx::write_json_atomic(
            &context.layout.meta().join("failures.json"),
            &serde_json::json!({"images":images.as_ref().err().map(ToString::to_string),"languages":editions.as_ref().err().map(ToString::to_string)}),
        )?;
    }
    let images = images?;
    let editions = editions?;
    Ok((images, editions))
}

async fn run_workflow_steps(
    context: Arc<Context>,
    until: Until,
    options: &Options,
) -> Result<(), WorkflowError> {
    if until == Until::Created {
        return Ok(());
    }
    text_stages(&context, until).await?;
    // A canon run researches a whole work; it never makes a video.
    let canon_build = context
        .guide
        .manifest()
        .research
        .canon
        .as_ref()
        .is_some_and(|rules| crate::canon::is_build(rules, &context.meta.inputs));
    if until <= Until::Storyboard || canon_build {
        return Ok(());
    }
    let board = load_board(&context)?;
    let board = Arc::new(board);
    if until == Until::Characters {
        let (_, sheets) = crate::assets::characters(&context, &board).await?;
        crate::review::characters(&context, &sheets)?;
        return Ok(());
    }
    if until == Until::Images {
        let images = Box::pin(crate::assets::images(
            Arc::clone(&context),
            Arc::clone(&board),
        ))
        .await?;
        crate::review::images_only(&context, &board, &images)?;
        return Ok(());
    }
    if context.guide.manifest().visuals.characters.review && !context.skip_reviews {
        let (_, sheets) = crate::assets::characters(&context, &board).await?;
        if !crate::character_approval::approved(&context.layout, &sheets)? {
            crate::review::characters(&context, &sheets)?;
            return Err(MediaError::Review(format!(
                "inspect the character sheets in {}, approve them with `retold video approve {} characters`, then plan and continue again; video permission is separate",
                context.layout.review_page().display(),
                crate::review::reference(&context)
            ))
            .into());
        }
    }
    let (images, editions) = produce_media(&context, &board, until).await?;
    if until <= Until::Voice {
        return Ok(());
    }
    crate::review::write(&context, &board, &images, &editions)?;
    if until == Until::Captions {
        return Ok(());
    }
    let timelines = assemble_timelines(&context, &board, &images, &editions, options).await?;
    let manifest = manifest(&context, &editions, options)?;
    for delivery in &manifest.editions {
        let edition = editions
            .iter()
            .find(|e| edition_id(&e.variant) == delivery.edition)
            .ok_or_else(|| EngineError::Setup("delivery edition missing".into()))?;
        crate::delivery::metadata(&context, edition, &delivery.directory).await?;
    }
    fsx::write_json_atomic(
        &context.layout.deliverables().join("manifest.json"),
        &manifest,
    )?;
    let key = hash(&timelines)?;
    context
        .media
        .store
        .lock()
        .await
        .commit("timeline", &key, &timelines, None)?;
    if until <= Until::Ready {
        return Ok(());
    }
    if !options.allow_video {
        return Err(MediaError::Review("all assets and timelines are ready; video creation requires explicit --allow-video permission".into()).into());
    }
    edit(&context, &timelines, &manifest, until, options).await?;
    Ok(())
}

fn load_board(context: &Context) -> Result<Storyboard, WorkflowError> {
    let path = context
        .layout
        .stage(Stage::Storyboard)
        .join("storyboard.json");
    let board: Storyboard =
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?)
            .map_err(|e| EngineError::Setup(e.to_string()))?;
    let board = crate::editing::scenes::effective(&context.layout, board)
        .map_err(|error| EngineError::Setup(error.to_string()))?;
    let errors = crate::story::direction_gates(
        &board.direction,
        &board.units,
        &board.plan,
        context.guide.manifest(),
    );
    if !errors.is_empty() {
        return Err(MediaError::Quality(errors.join("; ")).into());
    }
    Ok(board)
}

async fn text_stages(context: &Context, until: Until) -> Result<(), WorkflowError> {
    {
        let progress = |_: &str, _: bool| {};
        let paid = crate::paid_text::PaidText {
            registry: Arc::clone(&context.media.registry),
            ledger: Arc::clone(&context.media.ledger),
            allow_paid: context.media.allow_paid,
        };
        let text = Pipeline {
            scope: "",
            layout: &context.layout,
            meta: &context.meta,
            guide: &context.guide,
            router: &context.text,
            cancel: &context.media.cancel,
            progress: &progress,
            paid: Some(&paid),
        };
        let mut store = context.media.store.lock().await;
        text.run_stages(&mut store, until.min(Until::Storyboard))
            .await?;
    }
    Ok(())
}

pub fn manifest(
    context: &Context,
    editions: &[Edition],
    options: &Options,
) -> Result<Manifest, WorkflowError> {
    let mut deliveries = Vec::new();
    for e in editions {
        let edition = edition_id(&e.variant);
        for (id, aspect) in crate::delivery::formats(context, e)? {
            let ratio = aspect.map_or_else(
                || context.guide.manifest().format.aspect.replace(':', "x"),
                |a| a.id().into(),
            );
            let mut directory = context
                .layout
                .deliverables_for(&e.variant.narration)
                .join(ratio);
            if edition != e.variant.narration.to_string() {
                directory = directory.join(&edition);
            }
            deliveries.push(Delivery {
                edition: edition.clone(),
                aspect: aspect.map_or_else(
                    || context.guide.manifest().format.aspect.clone(),
                    |a| a.ratio().into(),
                ),
                directory: directory.clone(),
                id: id.clone(),
                timeline: context
                    .layout
                    .stage(Stage::Edit)
                    .join(format!("{id}.timeline.json")),
                video: directory.join(format!("{}.{id}.mp4", context.layout.slug())),
                thumbnail: directory.join(format!("thumbnail.{id}.png")),
                thumbnail_candidates: (2..=3)
                    .map(|i| directory.join(format!("thumbnail.{id}.candidate-{i}.png")))
                    .collect(),
                subtitles: e
                    .sidecars
                    .iter()
                    .map(|p| Ok(directory.join(crate::delivery::caption_name(p)?)))
                    .collect::<Result<std::collections::BTreeSet<_>, MediaError>>()?
                    .into_iter()
                    .collect(),
                posts: directory,
            });
        }
    }
    if deliveries
        .iter()
        .map(|d| &d.id)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != deliveries.len()
    {
        return Err(EngineError::Setup(
            "duplicate delivery editions; combine sidecar formats in one language variant".into(),
        )
        .into());
    }
    Ok(Manifest {
        schema: 2,
        title: context.meta.title.clone(),
        editions: deliveries,
        video_approved: options.allow_video,
    })
}

/// Export already assembled, reviewed timelines through the normal durable editor path.
/// Explicit video permission is required even when all source assets already exist.
pub async fn export_prepared(
    context: &Context,
    timelines: &[Timeline],
    manifest: &Manifest,
    options: &Options,
) -> Result<(), WorkflowError> {
    if !options.allow_video || !manifest.video_approved {
        return Err(MediaError::Review(
            "video creation requires explicit --allow-video permission".into(),
        )
        .into());
    }
    if timelines.is_empty()
        || timelines.len() != manifest.editions.len()
        || timelines
            .iter()
            .zip(&manifest.editions)
            .any(|(t, d)| t.name != d.id)
    {
        return Err(EngineError::Setup("prepared delivery does not match timelines".into()).into());
    }
    for timeline in timelines {
        timeline.validate().map_err(EngineError::Setup)?;
    }
    let until = if options.project_only {
        Until::Project
    } else {
        Until::Export
    };
    edit(context, timelines, manifest, until, options).await
}

fn project_timeline_key(timeline: &Timeline) -> Result<String, EngineError> {
    hash(&(timeline, "native-delivery-chapters-v1"))
}

async fn edit(
    context: &Context,
    timelines: &[Timeline],
    manifest: &Manifest,
    until: Until,
    options: &Options,
) -> Result<(), WorkflowError> {
    let binary = options
        .filmcraft_binary
        .clone()
        .unwrap_or_else(|| PathBuf::from(default_editor()));
    let session = Session::launch(
        &binary,
        &context.layout.meta().join("filmcraft"),
        &context.media.cancel,
    )
    .await?;
    let project = context.layout.stage(Stage::Edit).join("project.fcproj");
    let record_file = context.layout.stage(Stage::Edit).join("project.json");
    let key = hash(&timelines)?;
    let existing = std::fs::read(&record_file)
        .ok()
        .and_then(|b| serde_json::from_slice::<ProjectRecord>(&b).ok());
    let record = if let Some(record) = existing.filter(|r| {
        std::fs::read(&project).is_ok_and(|b| blake3::hash(&b).to_hex().as_str() == r.project_hash)
    }) {
        if !session.resumed {
            session
                .client
                .send_command(
                    &Command::OpenProject {
                        path: project.clone(),
                    },
                    &context.media.cancel,
                )
                .await?;
        }
        let mut record = record;
        for timeline in timelines {
            let timeline_key = project_timeline_key(timeline)?;
            if record.timeline_keys.get(&timeline.name) == Some(&timeline_key) {
                continue;
            }
            let built = build_sequence(context, timeline, manifest, &session).await?;
            record.sequences.insert(timeline.name.clone(), built);
            record
                .timeline_keys
                .insert(timeline.name.clone(), timeline_key);
        }
        save_project(&session, &project, &mut record, context).await?;
        record.key = key.clone();
        fsx::write_json_atomic(&record_file, &record)?;
        record
    } else {
        session
            .client
            .send_command(
                &Command::NewProject {
                    name: context.meta.title.clone(),
                },
                &context.media.cancel,
            )
            .await?;
        let mut sequences = BTreeMap::new();
        for timeline in timelines {
            let built = build_sequence(context, timeline, manifest, &session).await?;
            sequences.insert(timeline.name.clone(), built);
            session
                .client
                .send_command(
                    &Command::SaveAs {
                        path: project.clone(),
                    },
                    &context.media.cancel,
                )
                .await?;
        }
        let bytes = std::fs::read(&project).map_err(|e| EngineError::io(&project, e))?;
        let record = ProjectRecord {
            key,
            project_hash: blake3::hash(&bytes).to_hex().to_string(),
            sequences,
            timeline_keys: timelines
                .iter()
                .map(|t| Ok((t.name.clone(), project_timeline_key(t)?)))
                .collect::<Result<_, EngineError>>()?,
        };
        fsx::write_json_atomic(&record_file, &record)?;
        record
    };
    if until == Until::Project {
        return Ok(());
    }
    deliver(context, timelines, manifest, &record, &session).await?;
    Ok(())
}

async fn build_sequence(
    context: &Context,
    timeline: &Timeline,
    manifest: &Manifest,
    session: &Session,
) -> Result<editor::BuiltSequence, WorkflowError> {
    let built = editor::build(&session.client, timeline, &context.media.cancel).await?;
    let delivery = manifest
        .editions
        .iter()
        .find(|d| d.id == timeline.name)
        .ok_or_else(|| EngineError::Setup("chapter delivery missing".into()))?;
    let path = delivery.directory.join("chapters.json");
    let chapters: Vec<retold_deliver::Chapter> =
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?)
            .map_err(|e| EngineError::Setup(e.to_string()))?;
    for chapter in chapters {
        let time = retold_core::Seconds::new(chapter.seconds)
            .ok_or_else(|| EngineError::Setup("invalid chapter time".into()))?
            .to_frames(timeline.fps)
            .to_ticks(timeline.fps);
        session
            .client
            .send_command(
                &Command::Chapter {
                    time,
                    name: chapter.title,
                },
                &context.media.cancel,
            )
            .await?;
    }
    Ok(built)
}

async fn deliver(
    context: &Context,
    timelines: &[Timeline],
    manifest: &Manifest,
    record: &ProjectRecord,
    session: &Session,
) -> Result<(), WorkflowError> {
    for (timeline, delivery) in timelines.iter().zip(&manifest.editions) {
        let built = record
            .sequences
            .get(&timeline.name)
            .ok_or_else(|| EngineError::Setup("project is missing an edition sequence".into()))?;
        let export_key = hash(&(timeline, &delivery.video, "local-delivery-v2"))?;
        let step = format!("export-{}", delivery.id.to_lowercase());
        let cached: Option<String> = context
            .media
            .store
            .lock()
            .await
            .cached(&step, &export_key)?;
        if let Some(expected_hash) = cached {
            let bytes =
                std::fs::read(&delivery.video).map_err(|e| EngineError::io(&delivery.video, e))?;
            if blake3::hash(&bytes).to_hex().as_str() != expected_hash {
                return Err(EngineError::Setup(
                    "saved video changed; restore it before resuming".into(),
                )
                .into());
            }
        } else {
            guarded_export(context, session, timeline, built, delivery, &export_key).await?;
        }

        thumbnails(context, session, timeline, built, delivery).await?;
        crate::delivery::finish(context, delivery).await?;
        let bytes =
            std::fs::read(&delivery.video).map_err(|e| EngineError::io(&delivery.video, e))?;
        context.media.store.lock().await.commit(
            &step,
            &export_key,
            &blake3::hash(&bytes).to_hex().to_string(),
            None,
        )?;
    }
    Ok(())
}

async fn thumbnails(
    context: &Context,
    session: &Session,
    timeline: &Timeline,
    built: &editor::BuiltSequence,
    delivery: &Delivery,
) -> Result<(), WorkflowError> {
    let key = hash(&(timeline, "three-candidates-story-v2"))?;
    let step = format!("deliver-thumbnails-{}", delivery.id);
    let saved: Option<BTreeMap<PathBuf, String>> =
        context.media.store.lock().await.cached(&step, &key)?;
    if let Some(files) = saved {
        crate::delivery::verify(&files)?;
        return Ok(());
    }
    session
        .client
        .send_command(
            &Command::OpenSequence {
                item: built.sequence.clone(),
            },
            &context.media.cancel,
        )
        .await?;
    let captions_path = context.layout.stage(Stage::Captions).join("editions.json");
    let editions: Vec<Edition> = serde_json::from_slice(
        &std::fs::read(&captions_path).map_err(|e| EngineError::io(&captions_path, e))?,
    )
    .map_err(|e| EngineError::Setup(e.to_string()))?;
    let edition = editions
        .iter()
        .find(|e| edition_id(&e.variant) == delivery.edition)
        .ok_or_else(|| EngineError::Setup("thumbnail edition missing".into()))?;
    let mut times = vec![0.8];
    for fraction in [0.4, 0.75] {
        let midpoint = timeline.duration.to_seconds(timeline.fps).get() * fraction;
        let seconds = edition
            .alignment
            .units
            .iter()
            .min_by(|a, b| {
                (a.start.midpoint(a.end) - midpoint)
                    .abs()
                    .total_cmp(&(b.start.midpoint(b.end) - midpoint).abs())
            })
            .map_or(midpoint, |u| u.start.midpoint(u.end));
        times.push(seconds);
    }
    for (path, seconds) in std::iter::once(&delivery.thumbnail)
        .chain(&delivery.thumbnail_candidates)
        .zip(times)
    {
        session
            .client
            .send_command(&Command::Playhead { seconds }, &context.media.cancel)
            .await?;
        session
            .client
            .send_command(
                &Command::ExportFrame { path: path.clone() },
                &context.media.cancel,
            )
            .await?;
        let bytes = std::fs::read(path).map_err(|e| EngineError::io(path, e))?;
        retold_providers::media::image_dimensions(&bytes).map_err(MediaError::from)?;
    }
    crate::delivery::rank(context, delivery).await?;
    let files = crate::delivery::thumbnail_inventory(delivery)?;
    context
        .media
        .store
        .lock()
        .await
        .commit(&step, &key, &files, None)?;
    Ok(())
}

async fn save_project(
    session: &Session,
    path: &std::path::Path,
    record: &mut ProjectRecord,
    context: &Context,
) -> Result<(), WorkflowError> {
    session
        .client
        .send_command(
            &Command::SaveAs { path: path.into() },
            &context.media.cancel,
        )
        .await?;
    let bytes = std::fs::read(path).map_err(|e| EngineError::io(path, e))?;
    record.project_hash = blake3::hash(&bytes).to_hex().to_string();
    Ok(())
}
#[derive(Debug, Serialize, Deserialize)]
struct ExportAttempt {
    key: String,
    state: String,
    job: Option<serde_json::Value>,
    #[serde(default)]
    output_hash: Option<String>,
}
async fn guarded_export(
    context: &Context,
    session: &Session,
    timeline: &Timeline,
    built: &editor::BuiltSequence,
    delivery: &Delivery,
    key: &str,
) -> Result<(), WorkflowError> {
    let journal = context
        .layout
        .stage(Stage::Edit)
        .join(format!("{}.export-attempt.json", delivery.id));
    if recover_export(context, session, delivery, key, &journal).await? {
        return Ok(());
    }
    let loudness_lufs = crate::loudness::calibrate(context, session, built, delivery, key).await?;
    fsx::write_json_atomic(
        &journal,
        &ExportAttempt {
            key: key.into(),
            state: "dispatching".into(),
            job: None,
            output_hash: None,
        },
    )?;
    let response = session
        .client
        .export_approved(
            &Command::ExportMedia {
                loudness_lufs,
                path: delivery.video.clone(),
                sequence: built.sequence.clone(),
                width: timeline.width,
                height: timeline.height,
                fps: timeline.fps,
            },
            &context.media.cancel,
        )
        .await?;
    let job = response
        .get("job")
        .cloned()
        .ok_or_else(|| EngineError::Setup("export returned no job id".into()))?;
    fsx::write_json_atomic(
        &journal,
        &ExportAttempt {
            key: key.into(),
            state: "running".into(),
            job: Some(job.clone()),
            output_hash: None,
        },
    )?;
    editor::wait_job(&session.client, &job, &context.media.cancel).await?;
    if !std::fs::metadata(&delivery.video).is_ok_and(|m| m.len() > 0) {
        return Err(EngineError::Setup("finished export has no output".into()).into());
    }
    verify_loudness(context, delivery).await?;
    fsx::write_json_atomic(
        &journal,
        &ExportAttempt {
            key: key.into(),
            state: "complete".into(),
            job: Some(job),
            output_hash: Some(video_hash(delivery)?),
        },
    )?;
    Ok(())
}

async fn recover_export(
    context: &Context,
    session: &Session,
    delivery: &Delivery,
    key: &str,
    journal: &std::path::Path,
) -> Result<bool, WorkflowError> {
    if journal.exists() {
        let saved: ExportAttempt = serde_json::from_slice(
            &std::fs::read(journal).map_err(|e| EngineError::io(journal, e))?,
        )
        .map_err(|e| EngineError::Setup(e.to_string()))?;
        if saved.key == key {
            if saved.state == "complete"
                && std::fs::metadata(&delivery.video).is_ok_and(|m| m.len() > 0)
            {
                let bytes = std::fs::read(&delivery.video)
                    .map_err(|e| EngineError::io(&delivery.video, e))?;
                if saved
                    .output_hash
                    .as_ref()
                    .is_some_and(|expected| blake3::hash(&bytes).to_hex().as_str() != expected)
                {
                    return Err(EngineError::Setup(
                        "completed export changed before packaging".into(),
                    )
                    .into());
                }
                verify_loudness(context, delivery).await?;
                return Ok(true);
            }
            if session.resumed
                && let Some(job) = saved.job
            {
                editor::wait_job(&session.client, &job, &context.media.cancel).await?;
                if !std::fs::metadata(&delivery.video).is_ok_and(|m| m.len() > 0) {
                    return Err(EngineError::Setup("recovered job has no output".into()).into());
                }
                verify_loudness(context, delivery).await?;
                fsx::write_json_atomic(
                    journal,
                    &ExportAttempt {
                        key: key.into(),
                        state: "complete".into(),
                        job: Some(job),
                        output_hash: Some(video_hash(delivery)?),
                    },
                )?;
                return Ok(true);
            }
            if session.resumed {
                return Err(EngineError::Setup("old export dispatch has no job id; close the run's private editor before resuming".into()).into());
            }
            // A fresh owned process proves the previous process no longer owns this job.
            // Rebuild from the saved project and re-export under the existing permission.
        }
    }
    Ok(false)
}

fn video_hash(delivery: &Delivery) -> Result<String, EngineError> {
    let bytes = std::fs::read(&delivery.video).map_err(|e| EngineError::io(&delivery.video, e))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

async fn verify_loudness(context: &Context, delivery: &Delivery) -> Result<(), WorkflowError> {
    let measured = crate::loudness::measure(context, &delivery.video).await?;
    if (measured + 14.0).abs() > 0.2 {
        return Err(EngineError::Setup(format!(
            "exported {} measured {measured:.1} LUFS; expected -14 LUFS within 0.2 LU",
            delivery.id
        ))
        .into());
    }
    Ok(())
}
