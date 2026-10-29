//! Workspace root registry, inspection, initialization, and switching.
//! Follows the offline-first, zero-telemetry contract.

pub mod check;
pub mod init;
pub mod inspect;
pub mod list;
pub mod resolution;
pub mod switch;
pub mod types;

pub use check::check_workspace;
pub use init::{WorkspaceInitOptions, init_workspace};
pub use inspect::show_workspace;
pub use list::list_workspaces;
pub use switch::switch_workspace;
pub use types::{WorkspaceDescriptor, WorkspaceToml};
pub mod context;
