//! The full-screen app. `App` holds state and reacts to keys; `view` draws it; `runtime` runs
//! every command in the background through the same dispatch as the command line. The loop
//! polls input about sixty times a second, so the screen stays fluid while work runs.
mod app;
mod audio;
mod catalog;
mod clipboard;
mod collection;
mod dashboard;
mod defaults;
mod discovery;
mod document;
mod files;
mod form;
mod handoff;
mod input;
mod landing;
mod money;
mod onboarding;
mod page;
mod panic_guard;
mod preview;
mod processes;
mod production;
mod raster;
mod report;
mod review_filter;
mod routes;
mod runtime;
mod scope;
mod section;
pub mod spend_screen;
mod store;
mod stream;
mod theme;
mod view;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crate::interrupt::Interrupt;
use ratatui::crossterm::event;
use ratatui::layout::Rect;
use retold_app::protocol::ErrorCode;

use app::App;
use runtime::{Job, Worker};
pub use theme::Theme;

/// How long the loop waits for input before drawing again, in milliseconds.
const FRAME_MILLIS: u64 = 16;
/// Only spinner animation needs periodic frames; input and results draw immediately.
const ANIMATION_MILLIS: u64 = 80;

/// Where the app's commands look, and how it should look.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub library: Option<PathBuf>,
    pub theme: Theme,
}

#[must_use]
pub fn run(sources: &Sources) -> ExitCode {
    match Interrupt::session() {
        Ok(interrupt) => run_watching(sources, &interrupt),
        Err(error) => {
            eprintln!("Cannot watch terminal signals: {error}");
            ExitCode::from(ErrorCode::ExecutionFailed.exit_code())
        }
    }
}

fn run_watching(sources: &Sources, interrupt: &Interrupt) -> ExitCode {
    let terminal = ratatui::init();
    ratatui::crossterm::execute!(
        std::io::stdout(),
        event::EnableMouseCapture,
        event::EnableBracketedPaste
    )
    .ok();
    let outcome = event_loop(terminal, sources, interrupt);
    audio::stop_all();
    release_input();
    ratatui::restore();
    if outcome.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(ErrorCode::ExecutionFailed.exit_code())
    }
}

fn release_input() {
    ratatui::crossterm::execute!(
        std::io::stdout(),
        event::DisableMouseCapture,
        event::DisableBracketedPaste
    )
    .ok();
}

/// `--workspace PATH` before every command when the app was opened on a workspace.
fn prefix(sources: &Sources) -> Vec<String> {
    sources
        .library
        .as_ref()
        .map(|path| vec!["--workspace".to_owned(), path.display().to_string()])
        .unwrap_or_default()
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn send(worker: &Worker, reads: Vec<Vec<String>>, actions: Vec<Job>) {
    let queries = reads.into_iter().map(Job::Query);
    for job in queries.chain(actions) {
        worker.submit(job);
    }
}

fn event_loop(
    terminal: ratatui::DefaultTerminal,
    sources: &Sources,
    interrupt: &Interrupt,
) -> std::io::Result<()> {
    let mut session = Session::start(terminal, sources)?;
    loop {
        session = session.watch_shutdown(interrupt);
        let quit = session.app.quit;
        if quit {
            return Ok(());
        }
        session = session.step()?;
    }
}

/// Everything the loop owns. Each step takes the session and hands back the next one, so the
/// terminal, worker and state move together and nothing is mutated from outside.
struct Session {
    terminal: ratatui::DefaultTerminal,
    worker: Worker,
    app: App,
    started: Instant,
    /// Input arrived or something changed since the last frame.
    dirty: bool,
    /// The spinner frame last drawn.
    animation: u64,
}

impl Session {
    fn watch_shutdown(mut self, interrupt: &Interrupt) -> Self {
        let requested = interrupt.requested();
        if requested {
            self.app.quit_when_idle = true;
            self.app.quit = self.app.running.is_empty();
        }
        self
    }

    fn start(terminal: ratatui::DefaultTerminal, sources: &Sources) -> std::io::Result<Self> {
        panic_guard::install();
        let mut app = App::new(sources.theme);
        app.workspace.clone_from(&sources.library);
        app.area = terminal_area(terminal.size()?);
        app.load_defaults();
        Ok(Self {
            terminal,
            worker: Worker::new(prefix(sources), sources.library.clone()),
            app,
            started: Instant::now(),
            dirty: true,
            animation: 0,
        })
    }

    /// One turn: take replies, follow a workspace switch, draw if needed, read input, hand off.
    fn step(self) -> std::io::Result<Self> {
        let current = self.receive().switch_workspace();
        let drawn = current.draw()?;
        Ok(drawn.wait_for_input()?.hand_off())
    }

    fn receive(mut self) -> Self {
        let now = elapsed_millis(self.started);
        let replies = self.worker.ready();
        let tick = now / ANIMATION_MILLIS;
        self.dirty = self.dirty || self.animation != tick || !replies.is_empty();
        self.app = receive(self.app, replies, now);
        self
    }

    /// Replace the worker and root-scoped state only after tracked actions have replied.
    fn switch_workspace(mut self) -> Self {
        let Some(root) = self.app.workspace_ready().map(std::path::Path::to_path_buf) else {
            return self;
        };
        self.app.enter_workspace(root.clone());
        let sources = Sources {
            library: Some(root),
            theme: self.app.theme,
        };
        self.worker = Worker::new(prefix(&sources), sources.library);
        self.dirty = true;
        self
    }

    fn draw(mut self) -> std::io::Result<Self> {
        if !self.dirty {
            return Ok(self);
        }
        self.app = load(self.app, &self.worker);
        self.terminal.draw(view::frame(&self.app))?;
        self.animation = elapsed_millis(self.started) / ANIMATION_MILLIS;
        Ok(self)
    }

    fn wait_for_input(mut self) -> std::io::Result<Self> {
        self.dirty = event::poll(Duration::from_millis(FRAME_MILLIS))?;
        if self.dirty {
            self.app.react(&event::read()?);
        }
        Ok(self)
    }

    /// Lends the terminal to an editor, then redraws everything.
    fn hand_off(mut self) -> Self {
        let Some(handoff) = self.app.host.handoff.take() else {
            return self;
        };
        let (terminal, result) = handoff::run(self.terminal, &handoff);
        self.terminal = terminal;
        self.app.handoff_finished(result, &handoff);
        self.dirty = true;
        self
    }
}

fn receive(mut app: App, replies: Vec<runtime::Reply>, now: u64) -> App {
    app.tick(now);
    for reply in replies {
        app.receive(reply.job, reply.payload);
    }
    app
}

fn load(mut app: App, worker: &Worker) -> App {
    let reads = app.store.due(&app.wants(), app.now);
    app.store.mark_loading(&reads);
    send(worker, reads, app.jobs());
    app.clear_jobs();
    app
}

const fn terminal_area(size: ratatui::layout::Size) -> Rect {
    Rect::new(0, 0, size.width, size.height)
}
