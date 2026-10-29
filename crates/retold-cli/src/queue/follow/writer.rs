//! A bounded output worker lets the observer detach even when its consumer stops reading.
use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::{Duration, Instant};

use retold_app::protocol::{Envelope, Payload};
use retold_app::studio::ProgressEvent;

use crate::interrupt::Interrupt;
use crate::output::{self, Format};

const OUTPUT_CAPACITY: usize = 1;
const INTERRUPT_POLL: Duration = Duration::from_millis(25);
const FINAL_FLUSH: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub(super) struct Writer {
    messages: SyncSender<Message>,
    format: Format,
    request_id: String,
    job_id: String,
}

#[derive(Debug)]
struct Message {
    line: String,
    receipt: SyncSender<io::Result<()>>,
}

impl Writer {
    pub fn new(format: Format, job_id: &str) -> io::Result<Self> {
        let (messages, receiver) = mpsc::sync_channel(OUTPUT_CAPACITY);
        let stdout = owned_stdout()?;
        // This thread owns only client output. A blocked pipe must not hold the CLI or any job lease.
        std::thread::Builder::new()
            .name("retold-observer-output".into())
            .spawn(move || {
                let _ = OutputSink { stdout }.run(&receiver);
            })?;
        Ok(Self {
            messages,
            format,
            request_id: crate::request_id(),
            job_id: job_id.into(),
        })
    }

    pub fn progress(&self, event: &ProgressEvent, detach: &Interrupt) -> io::Result<()> {
        let mut envelope = self.envelope(Payload::Event(event.detail()));
        envelope.sequence = Some(event.sequence);
        let line = match self.format {
            Format::Json => output::render(&envelope, self.format)?,
            Format::Plain => format!(
                "[seq {}] {}: {}\n",
                event.sequence, event.stage, event.message
            ),
        };
        let receipt = self.offer(line)?;
        wait_for_delivery(&receipt, detach)
    }

    pub fn finish(&self, payload: Payload, detach: &Interrupt) -> u8 {
        let exit = payload.exit_code();
        let delivered = self.final_message(payload, detach);
        match delivered {
            Ok(()) => exit,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::BrokenPipe | io::ErrorKind::Interrupted
                ) =>
            {
                0
            }
            Err(error) if error.kind() == io::ErrorKind::TimedOut && detach.requested() => exit,
            Err(_) => retold_app::protocol::ErrorCode::ExecutionFailed.exit_code(),
        }
    }

    fn final_message(&self, payload: Payload, detach: &Interrupt) -> io::Result<()> {
        let line = output::render(&self.envelope(payload), self.format)?;
        let interrupted = detach.requested();
        if interrupted {
            return self.detached_message(&line);
        }
        let receipt = self.offer(line)?;
        wait_for_delivery(&receipt, detach)
    }

    fn detached_message(&self, line: &str) -> io::Result<()> {
        let started = Instant::now();
        loop {
            match self.offer(line.to_owned()) {
                Ok(receipt) => {
                    return receipt
                        .recv_timeout(FINAL_FLUSH.saturating_sub(started.elapsed()))
                        .map_err(delivery_error)?;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if started.elapsed() >= FINAL_FLUSH {
                        return Err(io::ErrorKind::TimedOut.into());
                    }
                    std::thread::sleep(INTERRUPT_POLL);
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn envelope(&self, payload: Payload) -> Envelope {
        let mut envelope = Envelope::new(self.request_id.clone(), payload);
        envelope.job_id = Some(self.job_id.clone());
        envelope
    }

    fn offer(&self, line: String) -> io::Result<Receiver<io::Result<()>>> {
        let (receipt, receiver) = mpsc::sync_channel(OUTPUT_CAPACITY);
        self.messages
            .try_send(Message { line, receipt })
            .map_err(|error| {
                let kind = match error {
                    TrySendError::Full(_) => io::ErrorKind::WouldBlock,
                    TrySendError::Disconnected(_) => io::ErrorKind::BrokenPipe,
                };
                io::Error::new(kind, "Observer output disconnected or stalled.")
            })?;
        Ok(receiver)
    }
}

fn wait_for_delivery(receipt: &Receiver<io::Result<()>>, detach: &Interrupt) -> io::Result<()> {
    loop {
        if detach.requested() {
            return receipt
                .recv_timeout(INTERRUPT_POLL)
                .unwrap_or_else(|error| {
                    Err(match error {
                        RecvTimeoutError::Timeout => io::ErrorKind::Interrupted.into(),
                        other @ RecvTimeoutError::Disconnected => delivery_error(other),
                    })
                });
        }
        match receipt.recv_timeout(INTERRUPT_POLL) {
            Ok(result) => return result,
            Err(RecvTimeoutError::Timeout) => (),
            Err(error) => return Err(delivery_error(error)),
        }
    }
}

fn delivery_error(error: RecvTimeoutError) -> io::Error {
    let kind = match error {
        RecvTimeoutError::Timeout => io::ErrorKind::TimedOut,
        RecvTimeoutError::Disconnected => io::ErrorKind::BrokenPipe,
    };
    io::Error::new(kind, "Observer output was not delivered.")
}

struct OutputSink {
    stdout: std::fs::File,
}

impl OutputSink {
    fn run(self, receiver: &Receiver<Message>) -> io::Result<()> {
        while let Ok(message) = receiver.recv() {
            self.write_message(&message)?;
        }
        Ok(())
    }

    fn write_message(&self, message: &Message) -> io::Result<()> {
        let mut output = &self.stdout;
        let result = output
            .write_all(message.line.as_bytes())
            .and_then(|()| output.flush());
        let notice = result
            .as_ref()
            .map_err(|error| io::Error::new(error.kind(), error.to_string()))
            .copied();
        let _ = message.receipt.send(notice);
        result
    }
}

/// A second handle to standard output that the output thread can own.
fn owned_stdout() -> io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd as _;
        Ok(std::fs::File::from(
            io::stdout().as_fd().try_clone_to_owned()?,
        ))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsHandle as _;
        Ok(std::fs::File::from(
            io::stdout().as_handle().try_clone_to_owned()?,
        ))
    }
}
