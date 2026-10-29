//! Replay committed pages, follow new events and detach without controlling the worker.
use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use clap::ArgMatches;
use retold_app::protocol::{Diagnostic, ErrorCode};
use retold_app::queue::{EVENT_PAGE_SIZE, ReplayPage};
use retold_app::studio::{Attachment, resolve_workspace_state};

use crate::interrupt::Interrupt;
use crate::output::Format;
use writer::Writer;

mod writer;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub fn run(root: &Path, args: &ArgMatches, format: Format) -> ExitCode {
    let reference = args.get_one::<String>("id").map_or("", String::as_str);
    let after = args.get_one::<u64>("after-sequence").copied().unwrap_or(0);
    let attachment = Attachment::open(&resolve_workspace_state(Some(root)), reference, after);
    let attachment = match attachment {
        Ok(attachment) => attachment,
        Err(error) => return crate::emit_body(error.diagnostic().into_payload(), format),
    };
    match Observer::new(attachment, args, format) {
        Ok(observer) => ExitCode::from(observer.follow()),
        Err(error) => crate::emit_body(
            Diagnostic::new(ErrorCode::ExecutionFailed, &error.to_string()).into_payload(),
            format,
        ),
    }
}

struct Observer {
    attachment: Attachment,
    writer: Writer,
    detach: Interrupt,
    quiet: bool,
}

#[derive(Debug)]
struct DeliveryFailure {
    cursor: Attachment,
    error: io::Error,
}

impl Observer {
    fn new(attachment: Attachment, args: &ArgMatches, format: Format) -> io::Result<Self> {
        let writer = Writer::new(format, attachment.job_id())?;
        Ok(Self {
            attachment,
            writer,
            detach: Interrupt::new()?,
            quiet: args.get_flag("quiet"),
        })
    }

    fn follow(mut self) -> u8 {
        loop {
            let interrupted = self.detach.requested();
            if interrupted {
                return self.finish(self.attachment.detached());
            }
            let page = match self.attachment.poll() {
                Ok(page) => page,
                Err(error) => {
                    return self.finish(self.attachment.annotate(error.diagnostic()));
                }
            };
            match self.deliver(&page) {
                Ok(cursor) => self.attachment = cursor,
                Err(failure) => {
                    self.attachment = failure.cursor;
                    return self.output_stopped(&failure.error);
                }
            }
            if let Some(exit) = self.after_page(page) {
                return exit;
            }
        }
    }

    fn after_page(&self, page: ReplayPage) -> Option<u8> {
        let full = page.events.len() == EVENT_PAGE_SIZE;
        if page.is_terminal {
            return Some(self.finish(page.terminal_result()));
        }
        if full {
            return Option::default();
        }
        match self.attachment.owner_present() {
            Ok(true) => {
                std::thread::sleep(POLL_INTERVAL);
                Option::default()
            }
            Ok(false) => Some(self.owner_unavailable()),
            Err(error) => Some(self.finish(self.attachment.annotate(error.diagnostic()))),
        }
    }

    fn deliver(&self, page: &ReplayPage) -> Result<Attachment, DeliveryFailure> {
        let mut cursor = self.attachment.clone();
        for event in &page.events {
            if let Err(error) = self.write_event(event) {
                return Err(DeliveryFailure { cursor, error });
            }
            cursor = cursor
                .acknowledged(event)
                .map_err(|error| DeliveryFailure {
                    cursor: cursor.clone(),
                    error: io::Error::other(error),
                })?;
        }
        Ok(cursor)
    }

    fn write_event(&self, event: &retold_app::studio::ProgressEvent) -> io::Result<()> {
        let interrupted = self.detach.requested();
        if interrupted {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.quiet {
            Ok(())
        } else {
            self.writer.progress(event, &self.detach)
        }
    }

    fn output_stopped(&self, error: &io::Error) -> u8 {
        match error.kind() {
            io::ErrorKind::Interrupted => self.finish(self.attachment.detached()),
            io::ErrorKind::BrokenPipe => 0,
            _ => ErrorCode::ExecutionFailed.exit_code(),
        }
    }

    fn finish(&self, payload: retold_app::protocol::Payload) -> u8 {
        self.writer.finish(payload, &self.detach)
    }

    fn owner_unavailable(&self) -> u8 {
        let diagnostic = Diagnostic::new(
            ErrorCode::StudioRequired,
            "Studio is unavailable. This attachment stopped; inspect the saved job and studio status before continuing.",
        );
        self.finish(self.attachment.annotate(diagnostic))
    }
}
