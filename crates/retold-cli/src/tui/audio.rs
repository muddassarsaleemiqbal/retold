//! One preview at a time. Replaying the selected track stops it; quitting stops playback.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// The track playing now, if any.
enum Playback {
    Idle,
    Playing(PathBuf, Child),
}

impl Playback {
    fn is(&self, path: &Path) -> bool {
        matches!(self, Self::Playing(playing, _) if playing == path)
    }

    fn stop(&mut self) {
        let previous = std::mem::replace(self, Self::Idle);
        if let Self::Playing(_, mut child) = previous {
            child.kill().ok();
            child.wait().ok();
        }
    }
}

static PLAYBACK: Mutex<Playback> = Mutex::new(Playback::Idle);

/// Plays `path`, or stops it when it is already playing.
pub fn play(path: &Path) -> std::io::Result<()> {
    let path = std::fs::canonicalize(path)?;
    let mut playback = PLAYBACK
        .lock()
        .map_err(|_| std::io::Error::other("Audio preview is unavailable."))?;
    let same = playback.is(&path);
    playback.stop();
    if same {
        return Ok(());
    }
    let mut command = player();
    command.arg(&path);
    let child = spawn_quietly(command)?;
    *playback = Playback::Playing(path, child);
    Ok(())
}

/// Starts the player with no terminal input or output, so it cannot disturb the screen.
fn spawn_quietly(mut command: Command) -> std::io::Result<Child> {
    let silent = command.stdin(Stdio::null()).stdout(Stdio::null());
    silent.stderr(Stdio::null()).spawn()
}

fn player() -> Command {
    if cfg!(target_os = "macos") {
        Command::new("afplay")
    } else {
        let mut command = Command::new("ffplay");
        command.args(["-nodisp", "-autoexit"]);
        command
    }
}

pub fn stop_all() {
    if let Ok(mut playback) = PLAYBACK.lock() {
        playback.stop();
    }
}
