//! Private launch handshake; the real provider cannot run before durable registration.
use super::worker::{RELEASE_TOKEN, STARTUP};
use std::io::{self, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(20);

pub(super) struct Gate {
    pub path: PathBuf,
    listener: UnixListener,
}

impl Gate {
    pub fn bind(endpoint: &Path) -> io::Result<Self> {
        let directory = endpoint
            .parent()
            .ok_or_else(|| io::Error::other("Missing supervisor directory."))?;
        let path = directory.join("worker.sock");
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        Ok(Self { path, listener })
    }

    pub fn release(
        &self,
        pid: u32,
        register: impl FnOnce(super::identity::Identity) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut connection = self.accept()?;
        let identity = super::identity::read(pid)?;
        if identity.group != identity.pid {
            return Err(io::Error::other("Worker is not its group leader."));
        }
        register(identity)?;
        connection.write_all(&[RELEASE_TOKEN])
    }

    fn accept(&self) -> io::Result<std::os::unix::net::UnixStream> {
        let deadline = Instant::now() + STARTUP;
        loop {
            match self.listener.accept() {
                Ok((connection, _)) => return Ok(connection),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other(
                    "Gated worker did not acknowledge startup.",
                ));
            }
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
