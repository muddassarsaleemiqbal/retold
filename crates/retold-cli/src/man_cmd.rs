//! Manual page generation in roff format for retold.

use clap::Command;
use retold_app::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

pub fn command() -> Command {
    Command::new("man").about("Generate roff man page to stdout")
}

#[must_use]
pub fn query(cmd: Command) -> Payload {
    let mut buffer = Vec::new();
    match clap_mangen::Man::new(cmd).render(&mut buffer) {
        Ok(()) => {
            let roff = String::from_utf8_lossy(&buffer).into_owned();
            Payload::Result {
                outcome: Outcome::Completed,
                contents: serde_json::json!({
                    "text": roff,
                    "format": "roff",
                }),
            }
        }
        Err(err) => Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to render man page: {err}"),
        )
        .into_payload(),
    }
}
