//! Temporarily return the terminal to a user's editor, then restore the TUI.
use ratatui::crossterm::{
    event,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Handoff {
    Editor {
        path: PathBuf,
        check: Option<Vec<String>>,
    },
}

/// Hands the terminal over and takes it back; the terminal is returned whatever happened.
pub fn run(
    terminal: ratatui::DefaultTerminal,
    handoff: &Handoff,
) -> (ratatui::DefaultTerminal, std::io::Result<()>) {
    let result = suspend().and_then(|()| edit(handoff));
    let (terminal, restored) = resume(terminal);
    (terminal, result.and(restored))
}

fn suspend() -> std::io::Result<()> {
    terminal::disable_raw_mode()?;
    ratatui::crossterm::execute!(
        std::io::stdout(),
        event::DisableMouseCapture,
        event::DisableBracketedPaste,
        LeaveAlternateScreen
    )
}

fn resume(
    mut terminal: ratatui::DefaultTerminal,
) -> (ratatui::DefaultTerminal, std::io::Result<()>) {
    let entered = ratatui::crossterm::execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        event::EnableMouseCapture,
        event::EnableBracketedPaste
    );
    // `Terminal::clear` asks the terminal for its cursor position, which multiplexers and
    // SSH sessions may not answer in time. Clearing directly and drawing one blank frame
    // makes the next frame repaint everything without that query.
    let restored = entered
        .and_then(|()| terminal::enable_raw_mode())
        .and_then(|()| {
            ratatui::crossterm::execute!(
                std::io::stdout(),
                terminal::Clear(terminal::ClearType::All)
            )
        })
        .and_then(|()| terminal.draw(|_| {}).map(|_| ()));
    (terminal, restored)
}

#[cfg(unix)]
fn edit(handoff: &Handoff) -> std::io::Result<()> {
    use rustix::termios::{OptionalActions, tcgetattr, tcsetattr};
    let original = tcgetattr(std::io::stdin())?;
    let outcome = invoke_editor(handoff);
    let restored =
        tcsetattr(std::io::stdin(), OptionalActions::Now, &original).map_err(std::io::Error::from);
    restored.and(outcome)
}

#[cfg(not(unix))]
fn edit(handoff: &Handoff) -> std::io::Result<()> {
    invoke_editor(handoff)
}

fn invoke_editor(handoff: &Handoff) -> std::io::Result<()> {
    let Handoff::Editor { path, .. } = handoff;
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".into());
    let mut words = editor.split_whitespace();
    let executable = words
        .next()
        .ok_or_else(|| std::io::Error::other("Set EDITOR to an editor program."))?;
    let status = std::process::Command::new(executable)
        .args(words)
        .arg(path)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(
            "The editor exited unsuccessfully; inspect your file before continuing.",
        ))
    }
}
