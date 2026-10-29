//! Bounded host clipboard work. Private file input avoids blocking on a child's stdin pipe.
use std::io::{self, Seek, Write};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const COPY_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Clipboard programs in preference order; the first installed program receives the text.
const CLIPBOARDS: &[&[&str]] = if cfg!(target_os = "macos") {
    &[&["pbcopy"]]
} else {
    &[
        &["wl-copy"],
        &["xclip", "-selection", "clipboard"],
        &["xsel", "--clipboard", "--input"],
    ]
};

pub fn copy(text: &str) -> io::Result<()> {
    CLIPBOARDS
        .iter()
        .find_map(|program| match copy_with(program, text) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            outcome => Some(outcome),
        })
        .unwrap_or_else(|| {
            Err(io::Error::other(
                "No clipboard program found (pbcopy, wl-copy, xclip or xsel).",
            ))
        })
}

fn copy_with(program: &[&str], text: &str) -> io::Result<()> {
    let (executable, arguments) = program
        .split_first()
        .ok_or_else(|| io::Error::other("Clipboard unavailable."))?;
    let mut input = tempfile::tempfile()?;
    input.write_all(text.as_bytes())?;
    input.rewind()?;
    let mut command = Command::new(executable);
    command.args(arguments).stdin(Stdio::from(input));
    command.stdout(Stdio::null()).stderr(Stdio::null());
    wait(command.spawn()?, COPY_TIMEOUT)
}

fn wait(mut child: Child, timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(io::Error::other("Clipboard unavailable.")),
            Err(error) => return terminate(child, error),
            Ok(None) => {}
        }
        if Instant::now() >= deadline {
            let error = io::Error::new(
                io::ErrorKind::TimedOut,
                "Clipboard timed out; try copying again.",
            );
            return terminate(child, error);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn terminate(mut child: Child, error: io::Error) -> io::Result<()> {
    child.kill().ok();
    child.wait()?;
    Err(error)
}

#[cfg(test)]
#[path = "tests/clipboard_host.rs"]
mod tests;
