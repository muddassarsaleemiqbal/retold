//! Studio daemon, IPC socket and client helpers.

mod attachment;
#[cfg(unix)]
mod endpoint;
#[cfg(windows)]
#[path = "endpoint_windows.rs"]
mod endpoint;
mod lifecycle;
mod platform;
pub use attachment::Attachment;
pub use lifecycle::{WorkerLaunch, start_worker};
pub mod client;
mod scoped_stop;
pub mod server;
mod startup;
pub use scoped_stop::{ScopedStop, stop_workspace_queue};
mod submission;
pub use startup::{hold_startup, validate_startup};
pub mod types;
mod worker;
pub use submission::{SubmitOptions, submit, submit_job};

pub use client::{
    poll_events, poll_events_after, query_status, resolve_workspace_state, send_request,
    stop_studio,
};
pub use server::StudioServer;
pub use types::{ProgressEvent, StudioError, StudioRequest, StudioResponse, StudioStatus};
