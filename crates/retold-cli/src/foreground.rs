//! Foreground signals cancel the engine; scoped ownership waits for physical work to drain.
use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use retold_app::continuation::{self, ContinueOptions, Control};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

use crate::interrupt::Interrupt;

const SIGNAL_POLL: Duration = Duration::from_millis(25);

pub fn run(root: &Path, reference: &str, options: &ContinueOptions) -> Payload {
    run_interruptible(root, reference, options).unwrap_or_else(|error| {
        Diagnostic::new(ErrorCode::ExecutionFailed, &error.to_string()).into_payload()
    })
}

fn run_interruptible(
    root: &Path,
    reference: &str,
    options: &ContinueOptions,
) -> io::Result<Payload> {
    let interrupt = Interrupt::new()?;
    let control = Control::default();
    std::thread::scope(|scope| {
        let (done, finished) = mpsc::channel();
        let cancellation = control.clone();
        let signal = &interrupt;
        let _watcher = std::thread::Builder::new()
            .name("retold-foreground-signals".into())
            .spawn_scoped(scope, move || watch(signal, &cancellation, &finished))?;
        let result = continuation::ContinueRequest {
            root,
            reference,
            options,
            control: &control,
        }
        .respond();
        drop(done);
        Ok(result)
    })
}

fn watch(interrupt: &Interrupt, control: &Control, finished: &Receiver<()>) {
    loop {
        if interrupt.requested() {
            control.cancel();
            return;
        }
        match finished.recv_timeout(SIGNAL_POLL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => (),
        }
    }
}
