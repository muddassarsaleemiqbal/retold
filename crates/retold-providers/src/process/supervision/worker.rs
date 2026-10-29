//! A trusted waiting process becomes the provider only after its identity is durable.
use std::ffi::OsString;
use std::io::{self, Read};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::Duration;

pub const WORKER_ENTRY: &str = "__retold_gated_worker_v1";
pub(super) const RELEASE_TOKEN: u8 = 1;
const PROGRAM_INDEX: usize = 3;
const ARGUMENT_INDEX: usize = 4;
pub(super) const STARTUP: Duration = Duration::from_secs(10);

#[must_use]
pub fn is_worker(args: &[OsString]) -> bool {
    args.get(1).is_some_and(|value| value == WORKER_ENTRY)
}

pub fn run_worker(args: &[OsString]) -> io::Result<u8> {
    let endpoint = args.get(2).ok_or_else(incomplete)?;
    let program = args.get(PROGRAM_INDEX).ok_or_else(incomplete)?;
    let mut connection = UnixStream::connect(endpoint)?;
    connection.set_read_timeout(Some(STARTUP))?;
    let mut token = [0];
    connection.read_exact(&mut token)?;
    let authorized = token[0] == RELEASE_TOKEN;
    if !authorized {
        return Err(incomplete());
    }
    Err(Command::new(program).args(&args[ARGUMENT_INDEX..]).exec())
}

fn incomplete() -> io::Error {
    io::Error::other("Incomplete or unapproved gated worker request.")
}
