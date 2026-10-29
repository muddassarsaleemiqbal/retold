//! The supervisor owns spawning and reaping; a broken owner connection cancels its child.
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus};
use std::time::Duration;

use crate::process::Group;

const POLL: Duration = Duration::from_millis(20);
const GROUP_EXIT_LIMIT: Duration = Duration::from_secs(5);
const PROGRAM_INDEX: usize = 3;
const ARGUMENT_INDEX: usize = 4;
const SIGNAL_EXIT_BASE: i32 = 128;

struct Supervisor {
    connection: UnixStream,
    child: Child,
    group: Group,
    child_group: u32,
    gate: super::gate::Gate,
    _endpoint: EndpointCleanup,
}

#[derive(Debug)]
pub struct Failure {
    pub error: io::Error,
    /// False only when preparation/spawn failed before any provider was created.
    pub started: bool,
}

pub fn serve(
    args: &[OsString],
    register: impl FnOnce(super::identity::Identity) -> io::Result<()>,
) -> Result<u8, Failure> {
    let supervisor = Supervisor::new(args).map_err(|error| Failure {
        error,
        started: false,
    })?;
    supervisor.run(register).map_err(|error| Failure {
        error,
        started: true,
    })
}

impl Supervisor {
    fn new(args: &[OsString]) -> io::Result<Self> {
        let endpoint = args.get(2).ok_or_else(missing)?;
        let program = args.get(PROGRAM_INDEX).ok_or_else(missing)?;
        let connection = UnixStream::connect(endpoint)?;
        connection.set_read_timeout(Some(POLL))?;
        let gate = super::gate::Gate::bind(std::path::Path::new(endpoint))?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg(super::worker::WORKER_ENTRY)
            .arg(&gate.path)
            .arg(program);
        let child = command
            .args(&args[ARGUMENT_INDEX..])
            .process_group(0)
            .spawn()?;
        let child_group = child.id();
        let group = Group::new(Some(child_group));
        Ok(Self {
            connection,
            child,
            group,
            child_group,
            gate,
            _endpoint: EndpointCleanup(PathBuf::from(endpoint)),
        })
    }

    fn run(
        mut self,
        register: impl FnOnce(super::identity::Identity) -> io::Result<()>,
    ) -> io::Result<u8> {
        let identity = self.child.id().to_le_bytes();
        let acknowledged = self
            .gate
            .release(self.child.id(), register)
            .and_then(|()| self.connection.write_all(&identity));
        let (owner, outcome) = match acknowledged {
            Ok(()) => self.monitor(),
            Err(error) => (self, Err(error)),
        };
        owner.finish(outcome)
    }

    fn finish(mut self, outcome: io::Result<ExitCause>) -> io::Result<u8> {
        let termination = self.grace(&outcome);
        self.group.kill();
        // The leader remains waitable until the last signal; its PID cannot be recycled.
        self.group.disarm();
        let status = self.child.wait()?;
        confirm_group_exit(self.child_group)?;
        termination?;
        outcome?;
        Ok(exit_code(status))
    }

    fn grace(&self, outcome: &io::Result<ExitCause>) -> io::Result<()> {
        if matches!(outcome, Ok(ExitCause::ProviderExited)) {
            return Ok(());
        }
        self.group.interrupt();
        let deadline = std::time::Instant::now() + crate::process::GRACE;
        let child = self.child.id();
        while !super::wait::exited(child)? {
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(POLL);
        }
        Ok(())
    }

    fn monitor(mut self) -> (Self, io::Result<ExitCause>) {
        loop {
            match super::wait::exited(self.child.id()) {
                Ok(true) => return (self, Ok(ExitCause::ProviderExited)),
                Err(error) => return (self, Err(error)),
                Ok(false) => (),
            }
            let mut byte = [0];
            let poll_timed_out = match self.connection.read(&mut byte) {
                Err(error) => is_benign_read_error(error.kind()),
                Ok(_) => false,
            };
            if !poll_timed_out {
                return (self, Ok(ExitCause::OwnerGone));
            }
        }
    }
}

/// An interrupted or empty poll means the owner is still there; keep watching it.
const fn is_benign_read_error(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

fn missing() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "Incomplete process supervisor request.",
    )
}

fn exit_code(status: ExitStatus) -> u8 {
    let value = status
        .code()
        .unwrap_or_else(|| SIGNAL_EXIT_BASE + status.signal().unwrap_or(1));
    u8::try_from(value).unwrap_or(1)
}

struct EndpointCleanup(PathBuf);

impl Drop for EndpointCleanup {
    fn drop(&mut self) {
        let Some(directory) = self.0.parent() else {
            return;
        };
        let managed = directory
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(super::DIRECTORY_PREFIX));
        if !managed {
            return;
        }
        let socket = std::fs::symlink_metadata(&self.0)
            .is_ok_and(|metadata| metadata.file_type().is_socket());
        if socket {
            let _ = std::fs::remove_file(&self.0);
        }
        // Preserve any unexpected contents; cleanup is deliberately non-recursive.
        let _ = std::fs::remove_dir(directory);
    }
}

/// Retain durable uncertainty if killed descendants cannot be confirmed gone.
fn confirm_group_exit(group: u32) -> io::Result<()> {
    use nix::errno::Errno;
    use nix::sys::signal::killpg;
    use nix::unistd::Pid;

    let identity = i32::try_from(group).map_err(io::Error::other)?;
    let deadline = std::time::Instant::now() + GROUP_EXIT_LIMIT;
    loop {
        match killpg(
            Pid::from_raw(identity),
            Option::<nix::sys::signal::Signal>::default(),
        ) {
            Err(Errno::ESRCH) => return Ok(()),
            // Permission failure cannot establish exit; keep observing within the finite bound.
            Err(Errno::EPERM) | Ok(()) => (),
            Err(error) => return Err(io::Error::other(error)),
        }
        if std::time::Instant::now() >= deadline {
            return Err(io::Error::other(
                "Provider process group exit is unconfirmed.",
            ));
        }
        std::thread::sleep(POLL);
    }
}

#[derive(Debug)]
enum ExitCause {
    ProviderExited,
    OwnerGone,
}
