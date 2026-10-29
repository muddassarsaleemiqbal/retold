//! The connection lifetime, rather than a reusable PID, identifies a live owning client.
use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, Command};

use super::{DIRECTORY_PREFIX, ENTRY, EXECUTABLE};
use crate::process::Group;

const STARTUP: Duration = Duration::from_secs(10);

pub(in crate::process) struct Launch {
    pub command: Command,
    endpoint: Option<Endpoint>,
}

struct Endpoint {
    listener: UnixListener,
    directory: tempfile::TempDir,
}

pub(in crate::process) struct Spawned {
    pub child: Child,
    pub group: Group,
    pub lease: Option<Lease>,
}

pub(in crate::process) struct Lease {
    connection: std::sync::Arc<UnixStream>,
    _directory: tempfile::TempDir,
}

impl Launch {
    pub fn new(program: &Path, args: &[String]) -> io::Result<Self> {
        let Some(executable) = EXECUTABLE.get() else {
            let mut command = Command::new(program);
            command.args(args).kill_on_drop(true).process_group(0);
            return Ok(Self {
                command,
                endpoint: None,
            });
        };
        let directory = tempfile::Builder::new()
            .prefix(DIRECTORY_PREFIX)
            .tempdir()?;
        let path = directory.path().join("owner.sock");
        let listener = UnixListener::bind(&path)?;
        let mut command = Command::new(executable);
        command.arg(ENTRY).arg(path);
        command.arg(program).args(args);
        Ok(Self {
            command,
            endpoint: Some(Endpoint {
                listener,
                directory,
            }),
        })
    }

    pub async fn spawn(mut self) -> io::Result<Spawned> {
        let child = self.command.spawn()?;
        let Some(endpoint) = self.endpoint else {
            let group = Group::new(child.id());
            return Ok(Spawned {
                child,
                group,
                lease: None,
            });
        };
        let lease = tokio::time::timeout(STARTUP, endpoint.connect()).await??;
        let group = Group::supervised(std::sync::Arc::clone(&lease.1.connection));
        Ok(Spawned {
            child,
            group,
            lease: Some(lease.1),
        })
    }
}

impl Endpoint {
    async fn connect(self) -> io::Result<(u32, Lease)> {
        let (mut connection, _) = self.listener.accept().await?;
        let pid = connection.read_u32_le().await?;
        if pid == 0 {
            return Err(io::Error::other(
                "Supervisor returned an invalid child identity.",
            ));
        }
        Ok((
            pid,
            Lease {
                connection: std::sync::Arc::new(connection),
                _directory: self.directory,
            },
        ))
    }
}
