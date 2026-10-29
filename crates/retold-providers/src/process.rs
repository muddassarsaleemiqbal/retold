//! Running provider CLIs safely: no shell, prompt on stdin, fresh empty working directory,
//! timeout, output cap, killed when dropped or cancelled.
//!
//! Each CLI runs in its own process group. On timeout or cancellation the whole group gets
//! SIGTERM, then SIGKILL after a grace period, so helpers a CLI spawned (node, python) cannot
//! outlive it. [`kill_all`] does the same for every live group before a forced exit.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(test)]
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::error::ProviderError;

#[cfg(unix)]
pub mod supervision;
#[cfg(not(unix))]
#[path = "process/direct.rs"]
mod supervision;
/// Windows keeps process identity for ownership records; the Unix supervisor is unavailable.
#[cfg(windows)]
pub mod windows_supervision;

/// What a finished CLI call produced.
#[derive(Debug)]
pub struct Output {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub timeout: Duration,
    pub max_stdout: usize,
    /// Extra environment for every call, e.g. `CLAUDE_CONFIG_DIR` for a second login.
    pub env: Vec<(String, String)>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(600),
            max_stdout: 8 * 1024 * 1024,
            env: Vec::new(),
        }
    }
}

/// How long a cancelled or timed-out CLI may take to exit after SIGTERM.
const GRACE: Duration = Duration::from_secs(2);

/// Process groups of CLIs that are still running.
static GROUPS: Mutex<BTreeSet<u32>> = Mutex::new(BTreeSet::new());

/// Sends SIGKILL to every live CLI process group. Call before `std::process::exit`, which
/// skips destructors.
pub fn kill_all() {
    let groups = GROUPS
        .lock()
        .map(|g| g.iter().copied().collect::<Vec<_>>())
        .unwrap_or_default();
    for group in groups {
        signal_group(group, Signal::Kill);
    }
}

#[derive(Debug, Clone, Copy)]
enum Signal {
    Term,
    Kill,
}

#[cfg(unix)]
fn signal_group(group: u32, signal: Signal) {
    use nix::sys::signal::{Signal as Sig, killpg};
    use nix::unistd::Pid;
    let Ok(group) = i32::try_from(group) else {
        return;
    };
    let signal = match signal {
        Signal::Term => Sig::SIGTERM,
        Signal::Kill => Sig::SIGKILL,
    };
    let _ = killpg(Pid::from_raw(group), signal);
}

#[cfg(not(unix))]
const fn signal_group(_group: u32, _signal: Signal) {}

/// Direct hosts own their child group. Supervised clients cancel through the private connection;
/// the independent supervisor retains the child PID until it finishes signaling and reaping.
#[derive(Debug)]
pub struct Group {
    pid: Option<u32>,
    #[cfg(unix)]
    supervisor: Option<std::sync::Arc<tokio::net::UnixStream>>,
}

impl Group {
    pub fn new(pid: Option<u32>) -> Self {
        if let (Some(pid), Ok(mut groups)) = (pid, GROUPS.lock()) {
            groups.insert(pid);
        }
        Self {
            pid,
            #[cfg(unix)]
            supervisor: None,
        }
    }

    #[cfg(unix)]
    pub(super) const fn supervised(connection: std::sync::Arc<tokio::net::UnixStream>) -> Self {
        Self {
            pid: None,
            supervisor: Some(connection),
        }
    }

    /// Stop tracking a group only after the supervisor confirms it no longer exists.
    #[cfg(unix)]
    pub(super) fn disarm(&mut self) {
        let identity = self.pid.take();
        if let (Some(group), Ok(mut groups)) = (identity, GROUPS.lock()) {
            groups.remove(&group);
        }
    }

    #[cfg_attr(
        not(unix),
        expect(dead_code, reason = "only the Unix supervisor reaches this")
    )]
    pub(super) fn interrupt(&self) {
        if let Some(group) = self.pid {
            signal_group(group, Signal::Term);
        }
    }

    /// Immediately stop all helpers, including during inference cancellation.
    pub fn kill(&self) {
        #[cfg(unix)]
        if let Some(connection) = &self.supervisor {
            let _ = rustix::io::write(connection.as_ref(), &[1]);
            return;
        }
        if let Some(group) = self.pid {
            signal_group(group, Signal::Kill);
        }
    }

    /// SIGTERM, escalate after `GRACE`, and reap the direct child before returning.
    pub async fn terminate(&self, child: &mut tokio::process::Child) -> std::io::Result<()> {
        #[cfg(unix)]
        let supervised = self.supervisor.is_some();
        #[cfg(unix)]
        if supervised {
            self.kill();
            child.wait().await?;
            return Ok(());
        }
        if let Some(group) = self.pid {
            signal_group(group, Signal::Term);
        }
        if let Ok(result) = tokio::time::timeout(GRACE, child.wait()).await {
            result?;
        } else {
            self.kill();
            child.wait().await?;
        }
        self.kill();
        Ok(())
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        self.kill();
        if let (Some(group), Ok(mut groups)) = (self.pid, GROUPS.lock()) {
            groups.remove(&group);
        }
    }
}

/// Runs `program args…` in `cwd`, writing `stdin` to it. Arguments go straight to the
/// process (never through a shell), so nothing in them is interpreted.
pub async fn run(
    program: &Path,
    args: &[String],
    stdin: &str,
    cwd: &Path,
    limits: &Limits,
    cancel: &CancellationToken,
) -> Result<Output, ProviderError> {
    run_with_env(program, args, &[], stdin, cwd, limits, cancel).await
}

/// Runs `program args…` in `cwd` with explicit environment variables.
pub async fn run_with_env(
    program: &Path,
    args: &[String],
    envs: &[(String, String)],
    stdin: &str,
    cwd: &Path,
    limits: &Limits,
    cancel: &CancellationToken,
) -> Result<Output, ProviderError> {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let _ = sender.send(stdin.as_bytes().to_vec()).await;
    drop(sender);
    run_stream(program, args, envs, receiver, cwd, limits, cancel).await
}

/// Streams bounded binary input (e.g. RGBA frames) through the same process safety boundary.
pub async fn run_stream(
    program: &Path,
    args: &[String],
    envs: &[(String, String)],
    input: tokio::sync::mpsc::Receiver<Vec<u8>>,
    cwd: &Path,
    limits: &Limits,
    cancel: &CancellationToken,
) -> Result<Output, ProviderError> {
    let name = program.file_name().map_or_else(
        || program.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let mut launch = supervision::Launch::new(program, args)
        .map_err(|error| ProviderError::Unavailable(error.to_string()))?;
    let cmd = &mut launch.command;
    cmd.envs(limits.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let spawned = launch
        .spawn()
        .await
        .map_err(|e| ProviderError::Unavailable(format!("could not start {name}: {e}")))?;
    let mut child = spawned.child;
    let group = spawned.group;
    let _lease = spawned.lease;

    let writer = write_input(child.stdin.take(), input);

    let cap = u64::try_from(limits.max_stdout)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let read_out = async move {
        let mut buf = Vec::new();
        if let Some(s) = stdout.as_mut() {
            s.take(cap).read_to_end(&mut buf).await.map(|_| buf)
        } else {
            Ok(buf)
        }
    };
    let read_err = async move {
        let mut buf = Vec::new();
        if let Some(s) = stderr.as_mut() {
            s.take(256 * 1024).read_to_end(&mut buf).await.map(|_| buf)
        } else {
            Ok(buf)
        }
    };

    let work = async {
        let (out, err) = tokio::join!(read_out, read_err);
        let status = child.wait().await;
        (out, err, status)
    };

    let outcome = tokio::select! {
        r = work => Ok(r),
        () = tokio::time::sleep(limits.timeout) => Err(ProviderError::Timeout(format!(
            "{name} took longer than {} s",
            limits.timeout.as_secs()
        ))),
        () = cancel.cancelled() => Err(ProviderError::Cancelled),
    };
    writer.abort();
    let (out, err, status) = match outcome {
        Ok(r) => r,
        Err(error) => {
            group
                .terminate(&mut child)
                .await
                .map_err(|reap| reaping_error(&reap))?;
            return Err(error);
        }
    };

    let out = out.map_err(|e| ProviderError::Failed(format!("reading {name} output: {e}")))?;
    if out.len() > limits.max_stdout {
        return Err(ProviderError::Malformed(format!(
            "{name} printed more than {} bytes",
            limits.max_stdout
        )));
    }
    let status = status.map_err(|e| ProviderError::Failed(format!("waiting for {name}: {e}")))?;
    Ok(Output {
        status: status.code(),
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err.unwrap_or_default()).into_owned(),
    })
}

fn write_input(
    mut child_stdin: Option<tokio::process::ChildStdin>,
    mut input: tokio::sync::mpsc::Receiver<Vec<u8>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Some(mut s) = child_stdin.take() {
            while let Some(bytes) = input.recv().await {
                if s.write_all(&bytes).await.is_err() {
                    break;
                }
            }
            let _ = s.shutdown().await;
        }
    })
}

fn reaping_error(error: &std::io::Error) -> ProviderError {
    ProviderError::Failed(format!("Could not confirm provider process exit: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn termination_reaps_a_child_that_ignores_sigterm() {
        use tokio::io::AsyncBufReadExt;
        let mut command = Command::new(sh());
        command
            .args([
                "-c",
                "trap '' TERM; printf 'ready\\n'; while :; do /bin/sleep 1; done",
            ])
            .process_group(0);
        command.stdout(Stdio::piped()).kill_on_drop(true);
        let mut child = command.spawn().unwrap();
        let group = Group::new(child.id());
        let mut output = tokio::io::BufReader::new(child.stdout.take().unwrap());
        let mut ready = String::new();
        output.read_line(&mut ready).await.unwrap();
        assert_eq!(ready, "ready\n");
        group.terminate(&mut child).await.unwrap();
        let reaped = child.try_wait().unwrap().is_some();
        group.kill();
        child.wait().await.unwrap();
        assert!(reaped, "termination returned before the child was reaped");
    }

    fn sh() -> &'static Path {
        Path::new("/bin/sh")
    }

    #[tokio::test]
    async fn passes_stdin_and_args_without_a_shell_in_between() {
        let tmp = tempfile::tempdir().unwrap();
        let args = vec![
            "-c".into(),
            "cat; printf ' %s' \"$0\"".into(),
            "$(rm -rf /)".into(),
        ];
        let out = run(
            sh(),
            &args,
            "hello",
            tmp.path(),
            &Limits::default(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(out.stdout, "hello $(rm -rf /)");
        assert_eq!(out.status, Some(0));
    }

    #[tokio::test]
    async fn passes_environment_variables_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let args = vec![
            "-c".into(),
            "printf '%s:%s' \"$TEST_KEY_A\" \"$TEST_KEY_B\"".into(),
        ];
        let envs = vec![
            ("TEST_KEY_A".into(), "val_a".into()),
            ("TEST_KEY_B".into(), "val_b".into()),
        ];
        let out = run_with_env(
            sh(),
            &args,
            &envs,
            "",
            tmp.path(),
            &Limits::default(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(out.stdout, "val_a:val_b");
        assert_eq!(out.status, Some(0));
    }

    #[tokio::test]
    async fn limits_env_reaches_the_child_command() {
        let tmp = tempfile::tempdir().unwrap();
        let limits = Limits {
            env: vec![("CLAUDE_CONFIG_DIR".into(), "/tmp/second-login".into())],
            ..Limits::default()
        };
        let out = run(
            sh(),
            &["-c".into(), "printf %s \"$CLAUDE_CONFIG_DIR\"".into()],
            "",
            tmp.path(),
            &limits,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(out.stdout, "/tmp/second-login");
    }

    #[tokio::test]
    async fn times_out() {
        let tmp = tempfile::tempdir().unwrap();
        let limits = Limits {
            timeout: Duration::from_millis(200),
            ..Limits::default()
        };
        let r = run(
            sh(),
            &["-c".into(), "sleep 5".into()],
            "",
            tmp.path(),
            &limits,
            &CancellationToken::new(),
        )
        .await;
        assert!(matches!(r, Err(ProviderError::Timeout(_))));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_kills_helpers_the_cli_spawned() {
        let tmp = tempfile::tempdir().unwrap();
        let pid_file = tmp.path().join("helper.pid");
        let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
        let limits = Limits {
            timeout: Duration::from_millis(300),
            ..Limits::default()
        };
        let r = run(
            sh(),
            &["-c".into(), script],
            "",
            tmp.path(),
            &limits,
            &CancellationToken::new(),
        )
        .await;
        assert!(matches!(r, Err(ProviderError::Timeout(_))));
        let helper: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let alive = || nix::sys::signal::kill(nix::unistd::Pid::from_raw(helper), None).is_ok();
        for _ in 0..50 {
            if !alive() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!alive(), "helper process {helper} outlived its CLI");
    }

    #[tokio::test]
    async fn caps_output() {
        let tmp = tempfile::tempdir().unwrap();
        let limits = Limits {
            max_stdout: 16,
            ..Limits::default()
        };
        let r = run(
            sh(),
            &["-c".into(), "yes a".into()],
            "",
            tmp.path(),
            &limits,
            &CancellationToken::new(),
        )
        .await;
        assert!(matches!(r, Err(ProviderError::Malformed(_))));
    }

    #[tokio::test]
    async fn cancels() {
        let tmp = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            c2.cancel();
        });
        let r = run(
            sh(),
            &["-c".into(), "sleep 5".into()],
            "",
            tmp.path(),
            &Limits::default(),
            &cancel,
        )
        .await;
        assert!(matches!(r, Err(ProviderError::Cancelled)));
    }
}
