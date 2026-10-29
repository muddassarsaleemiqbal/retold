//! The worker uses the same plan confirmation, account admission and engine as the foreground.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use retold_engine::events::{Event, EventObserver, Status};
use retold_engine::session::Control;

use super::{ProgressEvent, StudioError};
use crate::continuation::{self, ContinueOptions};
use crate::protocol::{Diagnostic, ErrorCode, Payload};
use crate::queue::{ExecutionIntent, JobRecord, QueueStore};
use crate::queue::{JobStatus, TransitionSpec};

#[derive(Debug)]
pub(super) struct Worker {
    pub id: String,
    pub control: Control,
    handle: JoinHandle<Result<(), StudioError>>,
}

impl Worker {
    pub fn start(queue: QueueStore, job: JobRecord) -> Result<Self, StudioError> {
        let id = job.id.clone();
        let activity = activity(&queue, &id)?;
        let spec = TransitionSpec {
            id: &id,
            target: JobStatus::Running,
            error: None,
        };
        queue.transition(spec, &jiff::Timestamp::now().to_string())?;
        queue.record_event(event(&id, "init", "Worker acquired job"))?;
        let failed_journal = Arc::new(AtomicBool::new(false));
        let control = observed_control(&queue, &id, &failed_journal);
        let worker_control = control.clone();
        let task = Task {
            queue,
            job,
            control: worker_control,
            failed_journal,
            _activity: activity,
        };
        let handle = std::thread::Builder::new()
            .name("retold-job".into())
            .spawn(move || task.run_workflow())?;
        Ok(Self {
            id,
            control,
            handle,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    pub fn join(self) -> Result<(), StudioError> {
        self.handle.join().map_err(|_| {
            StudioError::Protocol(
                "Worker exited unexpectedly; inspect its saved attempts before resuming.".into(),
            )
        })?
    }
}

struct Task {
    queue: QueueStore,
    job: JobRecord,
    control: Control,
    failed_journal: Arc<AtomicBool>,
    _activity: Option<crate::account::Account>,
}

pub(super) fn activity(
    queue: &QueueStore,
    id: &str,
) -> Result<Option<crate::account::Account>, StudioError> {
    if queue.intent(id)?.is_none() {
        return Ok(Option::default());
    }
    crate::account::activity("studio job and durable state")
        .map_err(|error| crate::account::commands::diagnostic(&error).into())
}

impl Task {
    fn run_workflow(self) -> Result<(), StudioError> {
        let result = run_saved(&self.queue, &self.job, &self.control)
            .unwrap_or_else(Diagnostic::into_payload);
        let journal_failed = self.failed_journal.load(Ordering::SeqCst);
        if journal_failed {
            return Err(StudioError::Protocol(
                "Progress persistence failed; execution stopped and requires recovery.".into(),
            ));
        }
        let finished = event(
            &self.job.id,
            "finished",
            "Worker finished; inspect the saved job outcome",
        );
        self.queue.finish(&self.job.id, &result, finished)?;
        Ok(())
    }
}

fn run_saved(
    queue: &QueueStore,
    job: &JobRecord,
    control: &Control,
) -> Result<Payload, Diagnostic> {
    let intent = queue.intent(&job.id).map_err(queue_diagnostic)?
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "This job has no saved executable intent. Review a video plan and submit a new job."))?;
    let catalog = crate::discovery::scan(&intent.library)?;
    let (plan, _) = crate::plan::compile(&catalog, &job.video_ref, &intent.plan)?;
    let options = pinned_options(intent, &job.plan_hash);
    continuation::confirm(&plan, &options)?;
    queue
        .record_event(event(&job.id, "plan", "Reviewed plan verified"))
        .map_err(queue_diagnostic)?;
    continuation::run_plan(&plan, &job.id, Some(control))
}

fn pinned_options(intent: ExecutionIntent, hash: &str) -> ContinueOptions {
    ContinueOptions {
        plan: intent.plan,
        plan_hash: Some(hash.into()),
        yes: false,
    }
}

fn observed_control(queue: &QueueStore, id: &str, failed_journal: &Arc<AtomicBool>) -> Control {
    let queue = queue.clone();
    let id = id.to_owned();
    let failed = Arc::clone(failed_journal);
    let control = Control::default();
    let cancellation = control.clone();
    control.with_observer(Arc::new(JournalObserver {
        queue,
        id,
        failed,
        cancellation,
    }))
}

#[derive(Debug)]
struct JournalObserver {
    queue: QueueStore,
    id: String,
    failed: Arc<AtomicBool>,
    cancellation: Control,
}

impl EventObserver for JournalObserver {
    fn observe(&self, engine_event: &Event) -> Result<(), retold_engine::EngineError> {
        let progress = engine_progress(&self.id, engine_event);
        self.queue.record_event(progress).map(|_| ()).map_err(|_| {
            self.failed.store(true, Ordering::SeqCst);
            self.cancellation.cancel();
            retold_engine::EngineError::Setup(
                "Durable progress could not be recorded; dispatch stopped.".into(),
            )
        })
    }
}

fn engine_progress(id: &str, engine_event: &Event) -> ProgressEvent {
    let message = match engine_event.status {
        Status::Running => "Stage running",
        Status::Cached => "Validated cached result reused",
        Status::Complete => "Stage completed",
        Status::Stopped => "Stage stopped; inspect saved logs",
    };
    let stage: String = engine_event.task.chars().take(100).collect();
    event(id, &stage, message)
}

pub(super) fn event(id: &str, stage: &str, message: &str) -> ProgressEvent {
    ProgressEvent {
        sequence: 0,
        job_id: id.into(),
        timestamp: jiff::Timestamp::now().to_string(),
        stage: stage.into(),
        step: None,
        message: message.into(),
        is_terminal: false,
    }
}

fn queue_diagnostic(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &error.to_string())
}
