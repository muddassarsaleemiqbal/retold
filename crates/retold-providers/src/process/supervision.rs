//! Re-exec a trusted host binary so an independent owner can reap provider processes.
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod client;
mod gate;
pub mod identity;
mod server;
mod wait;
mod worker;
pub(super) use client::Launch;
pub use server::{Failure, serve};
pub use worker::{is_worker, run_worker};

const DIRECTORY_PREFIX: &str = "retold-supervisor-";

pub const ENTRY: &str = "__retold_process_supervisor_v1";
static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();

pub fn initialize(executable: &Path) -> io::Result<()> {
    let executable = executable.canonicalize()?;
    if let Some(existing) = EXECUTABLE.get() {
        if existing != &executable {
            return Err(io::Error::other(
                "Process supervisor is already configured.",
            ));
        }
        return Ok(());
    }
    EXECUTABLE
        .set(executable)
        .map_err(|_| io::Error::other("Process supervisor initialization raced."))
}

#[must_use]
pub fn is_entry(args: &[OsString]) -> bool {
    args.get(1).is_some_and(|value| value == ENTRY)
}
