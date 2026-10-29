//! Singleton socket owner. Execution runs outside the responsive control loop.
use super::platform::{UnixListener, is_socket, make_private, owner_only_options};
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::worker::Worker;
use super::{StudioError, StudioRequest, StudioResponse};
use crate::queue::{JobStatus, QueueStore, TransitionSpec};

mod control;
mod ipc;

pub const SOCKET_NAME: &str = "studio.sock";
pub const LOCK_NAME: &str = "locks/studio.lock";
pub const QUEUE_DB_NAME: &str = "queue.sqlite";
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const PRIVATE_PERMISSIONS: u32 = 0o700;
const FILE_PERMISSIONS: u32 = 0o600;

#[derive(Debug)]
pub struct StudioServer {
    workspace_state: PathBuf,
    _lock_file: File,
    listener: UnixListener,
    queue: QueueStore,
    start_time: Instant,
    running: AtomicBool,
    worker: Mutex<Option<Worker>>,
}

impl StudioServer {
    pub fn bind(workspace_state: &Path) -> Result<Self, StudioError> {
        let (lock_file, queue, listener) = prepare_components(workspace_state)?;
        Ok(Self {
            workspace_state: workspace_state.to_path_buf(),
            _lock_file: lock_file,
            listener,
            queue,
            start_time: Instant::now(),
            running: AtomicBool::new(true),
            worker: Mutex::default(),
        })
    }

    #[must_use]
    pub const fn queue(&self) -> &QueueStore {
        &self.queue
    }

    pub fn run(&self) -> Result<(), StudioError> {
        self.run_until(&AtomicBool::new(false))
    }

    /// Drains active work before returning after the caller requests process shutdown.
    pub fn run_until(&self, shutdown: &AtomicBool) -> Result<(), StudioError> {
        let outcome = self.serve(shutdown);
        self.drain_worker()?;
        outcome
    }

    fn serve(&self, shutdown: &AtomicBool) -> Result<(), StudioError> {
        while self.is_running() && !shutdown.load(Ordering::SeqCst) {
            self.accept_pending_client()?;
            if self.is_running() && !shutdown.load(Ordering::SeqCst) {
                self.tick_worker()?;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn accept_pending_client(&self) -> Result<(), StudioError> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                // Bad or disconnected clients never terminate execution or the daemon.
                let _ = self.serve_client(&stream);
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn tick_worker(&self) -> Result<(), StudioError> {
        let mut worker = self.worker.lock().map_err(|_| worker_lock_error())?;
        let finished = worker.as_ref().is_some_and(Worker::is_finished);
        if finished && let Some(finished) = worker.take() {
            finished.join()?;
        }
        if let Some(active) = worker.as_ref() {
            self.cancel_requested(active)?;
        }
        if worker.is_some() {
            return Ok(());
        }
        let Some(job) = self.queue.next_queued()? else {
            return Ok(());
        };
        match Worker::start(self.queue.clone(), job) {
            Ok(started) => *worker = Some(started),
            Err(StudioError::Remote {
                code: crate::protocol::ErrorCode::AccountBusy,
                ..
            }) => (),
            Err(_) if self.queue.dispatch_inhibited()? => (),
            Err(error) => return Err(error),
        }
        Ok(())
    }

    fn drain_worker(&self) -> Result<(), StudioError> {
        let shutdown = self.begin_shutdown();
        let worker = self.worker.lock().map_err(|_| worker_lock_error())?.take();
        if let Some(worker) = worker {
            worker.join()?;
        }
        shutdown
    }

    fn dispatch(&self, request: StudioRequest) -> Result<StudioResponse, StudioError> {
        match request {
            StudioRequest::Ping => self.pong_status(),
            StudioRequest::Shutdown => {
                self.begin_shutdown()?;
                self.running.store(false, Ordering::SeqCst);
                Ok(StudioResponse::ShutdownAccepted)
            }
            StudioRequest::ListQueue { options } => Ok(StudioResponse::Queue {
                page: self.queue.list_page(&options)?,
            }),
            StudioRequest::GetJob { id } => self.inspect_job(&id),
            StudioRequest::PauseJob { id } => self.control_job(&id, JobStatus::Pausing),
            StudioRequest::UnpauseJob { id } => self.control_job(&id, JobStatus::Queued),
            StudioRequest::StopJob { id } => self.control_job(&id, JobStatus::Stopping),
            StudioRequest::MoveJob { id, before } => self.move_pending(&id, &before),
            StudioRequest::AttachEvents {
                job_id,
                after_sequence,
            } => self.attach_events(&job_id, after_sequence),
            control => self.dispatch_control(control),
        }
    }

    fn pong_status(&self) -> Result<StudioResponse, StudioError> {
        let active = usize::from(
            self.worker
                .lock()
                .map_err(|_| worker_lock_error())?
                .is_some(),
        );
        let queue_depth = self.queue.pending_count()?;
        Ok(StudioResponse::Pong {
            pid: std::process::id(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            active_jobs: active,
            queue_depth,
        })
    }
}

fn prepare_components(root: &Path) -> Result<(File, QueueStore, UnixListener), StudioError> {
    prepare_state(root)?;
    let (lock_path, database_path, _) = studio_paths(root);
    let lock = acquire_lock(&lock_path)?;
    let queue = QueueStore::open(&database_path)?;
    recover_interrupted(&queue)?;
    let socket_path = super::endpoint::prepare(root)?;
    let listener = bind_socket(&socket_path)?;
    Ok((lock, queue, listener))
}

fn studio_paths(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        root.join(LOCK_NAME),
        root.join(QUEUE_DB_NAME),
        root.join(SOCKET_NAME),
    )
}

fn prepare_state(path: &Path) -> Result<(), StudioError> {
    super::validate_startup(Option::default(), path)?;
    if path.is_symlink() {
        return Err(StudioError::Protocol(
            "Studio state must be a regular directory.".into(),
        ));
    }
    std::fs::create_dir_all(path)?;
    make_private(path, PRIVATE_PERMISSIONS)?;
    Ok(())
}

fn acquire_lock(path: &Path) -> Result<File, StudioError> {
    if path.is_symlink() {
        return Err(StudioError::Protocol(
            "Studio lock must not be a symlink.".into(),
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock_file = owner_only_options(FILE_PERMISSIONS).open(path)?;
    match lock_file.try_lock() {
        Ok(()) => Ok(lock_file),
        Err(TryLockError::WouldBlock) => Err(StudioError::AlreadyRunning),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

fn bind_socket(path: &Path) -> Result<UnixListener, StudioError> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        let socket = is_socket(metadata.file_type());
        if !socket {
            return Err(StudioError::Protocol(
                "The studio socket path contains another file; it was preserved.".into(),
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    listener.set_nonblocking(true)?;
    make_private(path, PRIVATE_PERMISSIONS)?;
    Ok(listener)
}

fn recover_interrupted(queue: &QueueStore) -> Result<(), StudioError> {
    for job in queue.interrupted()? {
        let _activity = super::worker::activity(queue, &job.id)?;
        let event = super::worker::event(&job.id, "recovery", "Interrupted worker requires review");
        queue.recover(&job.id, event)?;
    }
    Ok(())
}

fn worker_lock_error() -> StudioError {
    StudioError::Protocol("Worker ownership lock failed.".into())
}

impl Drop for StudioServer {
    fn drop(&mut self) {
        let _ = self.drain_worker();
        let _ = std::fs::remove_file(self.workspace_state.join(SOCKET_NAME));
        super::endpoint::cleanup(&self.workspace_state);
    }
}
