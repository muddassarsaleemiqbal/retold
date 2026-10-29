//! A private, caller-owned FilmCraft process. No connection is made to an existing
//! editor port; dropping the session kills only the process it launched.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use crate::{Client, Error};

#[derive(Debug)]
pub struct Session {
    pub client: Client,
    pub process: Option<Child>,
    pub resumed: bool,
    owner: Owner,
    pub port: u16,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Owner {
    pid: u32,
    port: u16,
    binary: std::path::PathBuf,
    data_dir: std::path::PathBuf,
}
impl Owner {
    fn matches(&self) -> bool {
        std::process::Command::new("ps")
            .args(["-p", &self.pid.to_string(), "-o", "command="])
            .output()
            .is_ok_and(|output| {
                let command = String::from_utf8_lossy(&output.stdout);
                command.starts_with(self.binary.to_string_lossy().as_ref())
                    && command.contains(&format!("--control {}", self.port))
                    && command.contains(&format!("--data-dir {}", self.data_dir.display()))
            })
    }
    #[cfg(unix)]
    fn terminate(&self) {
        if let Ok(pid) = i32::try_from(self.pid) {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGTERM,
            );
        }
    }
    #[cfg(not(unix))]
    const fn terminate(&self) {}

    /// Persists ownership atomically so a crash mid-write never leaves a half file that would
    /// orphan the editor on the next resume.
    fn save(&self, path: &Path) -> Result<(), Error> {
        let bytes = serde_json::to_vec(self).map_err(|e| Error::Protocol(e.to_string()))?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        std::io::Write::write_all(&mut tmp, &bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path).map_err(|e| Error::from(e.error))?;
        Ok(())
    }
    fn owns_port(&self) -> bool {
        let output = std::process::Command::new("lsof")
            .args([
                "-a",
                "-p",
                &self.pid.to_string(),
                &format!("-iTCP:{}", self.port),
                "-sTCP:LISTEN",
                "-Fn",
            ])
            .output();
        output.is_ok_and(|o| {
            o.status.success()
                && String::from_utf8_lossy(&o.stdout).contains(&format!(":{}", self.port))
        })
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if self.process.is_none() && self.owner.matches() {
            self.owner.terminate();
        }
    }
}
impl Session {
    pub async fn launch(
        binary: &Path,
        data_dir: &Path,
        cancel: &CancellationToken,
    ) -> Result<Self, Error> {
        std::fs::create_dir_all(data_dir)?;
        let owner_file = data_dir.join("owner.json");
        if let Ok(bytes) = std::fs::read(&owner_file)
            && let Ok(owner) = serde_json::from_slice::<Owner>(&bytes)
            && owner.binary == binary
            && owner.data_dir == data_dir
            && owner.matches()
        {
            if !owner.owns_port() {
                return Err(Error::Protocol("the previous private editor is still running but its port is unavailable; wait for it to stop".into()));
            }
            let client = Client::connect(owner.port, Duration::from_secs(120), cancel).await?;
            return Ok(Self {
                port: owner.port,
                client,
                process: None,
                resumed: true,
                owner,
            });
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let mut process = Command::new(binary)
            .args([
                "--empty",
                "--no-recover",
                "--control",
                &port.to_string(),
                "--data-dir",
            ])
            .arg(data_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let owner = Owner {
            pid: process
                .id()
                .ok_or_else(|| Error::Protocol("editor returned no pid".into()))?,
            port,
            binary: binary.into(),
            data_dir: data_dir.into(),
        };
        owner.save(&owner_file)?;
        for _ in 0..120 {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if process.try_wait()?.is_some() {
                return Err(Error::Protocol(
                    "private FilmCraft process exited before opening its port".into(),
                ));
            }
            match Client::connect(port, Duration::from_secs(120), cancel).await {
                Ok(client) => {
                    if !owner.owns_port() {
                        return Err(Error::Protocol(
                            "private port is not owned by the launched editor".into(),
                        ));
                    }
                    return Ok(Self {
                        client,
                        process: Some(process),
                        port,
                        resumed: false,
                        owner,
                    });
                }
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
            }
        }
        Err(Error::Timeout)
    }
}
