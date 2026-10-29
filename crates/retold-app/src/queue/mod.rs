//! Durable job queue backed by SQLite (`queue.sqlite`).
//! Enforces state machine transitions, idempotency keys, and priority ordering.
//! Presentation-free and read-only by default when querying.

mod dispatch;
mod durable;
mod inspection;
mod paging;
mod replay;
mod schema;
pub mod store;
pub mod types;
mod validation;
pub use dispatch::{DispatchState, StopReceipt};
pub use durable::{EVENT_PAGE_SIZE, ExecutionIntent};
pub use inspection::JobDetails;
pub use paging::{DEFAULT_PAGE_SIZE, JobPage, ListOptions, MAX_PAGE_SIZE};
pub use replay::ReplayPage;

pub use store::QueueStore;
pub use types::{JobRecord, JobSpec, JobStatus, QueueError, TransitionSpec};
