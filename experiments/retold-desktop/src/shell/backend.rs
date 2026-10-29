//! Blocking shared application requests. The native view never reads workspace files.
use std::path::PathBuf;

use anyhow::anyhow;
use retold_app::creation::{self, Options};
use retold_app::desktop::{self, ReviewedRun, WorkspaceSnapshot};
use retold_app::protocol::Diagnostic;

#[derive(Debug)]
pub enum Request {
    Videos {
        library: PathBuf,
        options: retold_app::library::ListOptions,
        history: Vec<Option<String>>,
    },
    Activity {
        library: PathBuf,
        options: retold_app::queue::ListOptions,
    },
    Job {
        library: PathBuf,
        id: String,
        after: u64,
    },
    Control {
        library: PathBuf,
        id: String,
        action: JobAction,
        after: u64,
    },
    Initialize {
        root: PathBuf,
        name: String,
    },
    Import {
        root: PathBuf,
        config: PathBuf,
        kind: ResourceKind,
        id: String,
        source: PathBuf,
    },
    StartWorker {
        root: PathBuf,
        library: PathBuf,
    },
    ShutdownWorker {
        root: PathBuf,
        state: PathBuf,
    },
    Review {
        root: PathBuf,
        reference: String,
        options: retold_app::plan::PlanOptions,
    },
    Submit(Box<ReviewedRun>),
    Open(PathBuf),
    PreviewSource(PathBuf),
    Create {
        root: PathBuf,
        options: Options,
        source: Option<creation::SourcePreview>,
    },
    Stop {
        root: PathBuf,
        library: PathBuf,
        state: PathBuf,
        control_id: String,
    },
    Enable {
        root: PathBuf,
        state: PathBuf,
        control_id: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ResourceKind {
    Guide,
    Channel,
}

#[derive(Debug, Clone)]
pub enum JobAction {
    Pause,
    Resume,
    Stop,
    MoveBefore(String),
}

#[derive(Debug, Clone)]
pub struct ObservedJob {
    pub library: PathBuf,
    pub page: retold_app::queue::ReplayPage,
    pub owner_present: bool,
}

#[derive(Debug)]
pub enum Reply {
    Source(creation::SourcePreview),
    Videos {
        page: Box<retold_app::library::VideoPage>,
        options: retold_app::library::ListOptions,
        history: Vec<Option<String>>,
    },
    Stopped {
        root: PathBuf,
        stop: retold_app::studio::ScopedStop,
        activity: Box<Result<retold_app::desktop::ActivitySnapshot, Diagnostic>>,
    },
    Activity {
        library: PathBuf,
        options: retold_app::queue::ListOptions,
        activity: Box<retold_app::desktop::ActivitySnapshot>,
    },
    Job(Box<ObservedJob>),
    Workspace {
        snapshot: Box<Result<WorkspaceSnapshot, Diagnostic>>,
        message: String,
        created: bool,
    },
    Review(Box<ReviewedRun>),
    Submitted(retold_app::queue::JobRecord),
}

impl Request {
    pub fn execute(self) -> anyhow::Result<Reply> {
        let (root, message, created) = match self {
            Self::PreviewSource(path) => {
                return creation::preview_source(&path)
                    .map(Reply::Source)
                    .map_err(diagnostic);
            }
            Self::Videos {
                library,
                options,
                history,
            } => {
                let page = retold_app::library::page(&library, &options).map_err(diagnostic)?;
                return Ok(Reply::Videos {
                    page: Box::new(page),
                    options,
                    history,
                });
            }
            Self::Activity { library, options } => {
                let activity = desktop::activity(&library, &options).map_err(diagnostic)?;
                return Ok(Reply::Activity {
                    library,
                    options,
                    activity: Box::new(activity),
                });
            }
            Self::Job { library, id, after } => return observe_job(library, &id, after),
            Self::Control {
                library,
                id,
                action,
                after,
            } => {
                control_job(&library, &id, action)?;
                return observe_job(library, &id, after);
            }
            Self::Initialize { root, name } => {
                let options = retold_app::workspace::WorkspaceInitOptions {
                    name: (!name.trim().is_empty()).then_some(name),
                    dry_run: false,
                };
                completed(retold_app::workspace::init_workspace(&root, &options))?;
                (
                    root,
                    "Workspace created. Add a guide and channel to save your first video draft."
                        .into(),
                    false,
                )
            }
            Self::Import {
                root,
                config,
                kind,
                id,
                source,
            } => {
                let payload = match kind {
                    ResourceKind::Guide => retold_app::authoring::create_guide(
                        &config,
                        &retold_app::authoring::GuideOptions {
                            id,
                            from: Some(source),
                            ..Default::default()
                        },
                    ),
                    ResourceKind::Channel => retold_app::authoring::create_channel(
                        &config,
                        &retold_app::authoring::ChannelOptions {
                            id,
                            from: Some(source),
                            ..Default::default()
                        },
                    ),
                }
                .map_err(diagnostic)?;
                completed(payload)?;
                (
                    root,
                    "Resource imported and validated. Existing resources were preserved.".into(),
                    false,
                )
            }
            Self::StartWorker { root, library } => {
                desktop::queue_state(&library).map_err(diagnostic)?;
                let launch = retold_app::studio::WorkerLaunch {
                    executable: std::env::current_exe()?,
                    arguments: vec![
                        "--worker".into(),
                        "--library".into(),
                        library.as_os_str().to_os_string(),
                    ],
                };
                let status = retold_app::studio::start_worker(&library, &launch)?;
                (
                    root,
                    format!(
                        "Worker running (PID {}). Closing Studio leaves it running.",
                        status.pid.unwrap_or_default()
                    ),
                    false,
                )
            }
            Self::ShutdownWorker { root, state } => {
                let stopped = retold_app::studio::stop_studio(&state)?;
                (
                    root,
                    if stopped {
                        "Worker exited after draining active work. Pending jobs remain saved."
                            .into()
                    } else {
                        "Worker is already offline. Saved queue state remains available.".into()
                    },
                    false,
                )
            }
            Self::Review {
                root,
                reference,
                options,
            } => {
                return desktop::review_run(&root, &reference, &options)
                    .map(|review| Reply::Review(Box::new(review)))
                    .map_err(diagnostic);
            }
            Self::Submit(review) => {
                let key = format!("desktop-{}", review.plan.plan_hash);
                return review
                    .submit(&key)
                    .map(Reply::Submitted)
                    .map_err(diagnostic);
            }
            Self::Open(root) => (root, "Workspace loaded.".into(), false),
            Self::Create {
                root,
                options,
                source,
            } => {
                let draft = match source {
                    Some(source) => creation::save_draft_from_source(&root, &options, &source),
                    None if options.source_file.is_none() => creation::save_draft(&root, &options),
                    None => return Err(anyhow!("Preview the selected source before saving.")),
                }
                .map_err(diagnostic)?;
                (
                    root,
                    format!(
                        "Saved ‘{}’. No generation or export was started.",
                        draft.title
                    ),
                    true,
                )
            }
            Self::Stop {
                root,
                library,
                state,
                control_id,
            } => {
                let stop = retold_app::studio::stop_workspace_queue(&state, &control_id)?;
                let activity =
                    desktop::activity(&library, &retold_app::queue::ListOptions::default());
                return Ok(Reply::Stopped {
                    root,
                    stop,
                    activity: Box::new(activity),
                });
            }
            Self::Enable {
                root,
                state,
                control_id,
            } => {
                let queue = retold_app::queue::QueueStore::open(&state.join("queue.sqlite"))?;
                queue.resume_dispatch(&control_id)?;
                (
                    root,
                    "Dispatch enabled. Stopped jobs remain stopped.".into(),
                    false,
                )
            }
        };
        Ok(Reply::Workspace {
            snapshot: Box::new(desktop::open_workspace(&root)),
            message,
            created,
        })
    }
}

fn observe_job(library: PathBuf, id: &str, after: u64) -> anyhow::Result<Reply> {
    let state = desktop::queue_state(&library).map_err(diagnostic)?;
    let attachment = retold_app::studio::Attachment::open(&state, id, after)?;
    Ok(Reply::Job(Box::new(ObservedJob {
        library,
        page: attachment.poll()?,
        owner_present: attachment.owner_present()?,
    })))
}

fn control_job(library: &std::path::Path, id: &str, action: JobAction) -> anyhow::Result<()> {
    use retold_app::studio::{StudioRequest, StudioResponse};
    let state = desktop::queue_state(library).map_err(diagnostic)?;
    let request = match action {
        JobAction::Pause => StudioRequest::PauseJob { id: id.into() },
        JobAction::Resume => StudioRequest::UnpauseJob { id: id.into() },
        JobAction::Stop => StudioRequest::StopJob { id: id.into() },
        JobAction::MoveBefore(before) => StudioRequest::MoveJob {
            id: id.into(),
            before,
        },
    };
    match retold_app::studio::send_request(&state, &request)? {
        StudioResponse::JobTransitioned { .. } => Ok(()),
        StudioResponse::Error { code, message } => Err(anyhow!("{}: {message}", code.as_str())),
        _ => Err(anyhow!("Worker did not acknowledge this job control.")),
    }
}

fn diagnostic(error: Diagnostic) -> anyhow::Error {
    anyhow!("{}: {}", error.code.as_str(), error.message)
}

fn completed(payload: retold_app::protocol::Payload) -> anyhow::Result<()> {
    use retold_app::protocol::{Outcome, Payload};
    match payload {
        Payload::Result {
            outcome: Outcome::Completed,
            ..
        } => Ok(()),
        Payload::Error {
            diagnostic: error, ..
        } => Err(diagnostic(error)),
        _ => Err(anyhow!(
            "The shared application did not confirm completion."
        )),
    }
}
