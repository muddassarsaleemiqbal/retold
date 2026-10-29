//! Shared application contracts. No printing or prompting; provider dispatch is gated here,
//! never performed here.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod account;
pub mod assets;
pub mod audio;
pub mod authoring;
pub mod configuration;
pub mod continuation;
pub mod creation;
pub mod creative;
pub mod desktop;
pub mod dev;
pub mod dialogue;
mod discovery;
pub mod doctor;
pub mod editing;
pub mod execution;
pub mod home;
pub mod library;
pub mod narration;
pub mod notes;
pub mod packaging;
pub mod plan;
pub mod presets;
pub mod profile;
pub mod protocol;
pub mod provider_tests;
pub mod providers;
pub mod queue;
pub mod retake;
pub mod review;
pub mod scenes;
pub mod series;
pub mod setup;
pub mod storage;
pub mod studio;
#[cfg(unix)]
pub mod supervision;
pub mod video;
pub mod views;
pub mod workspace;

use protocol::{Diagnostic, ErrorCode, Outcome, Payload};

/// The initial slice exposes only read-only protocol and diagnostic use cases.
#[derive(Debug, Clone, Copy)]
pub enum Request<'a> {
    Usage,
    Schema,
    Explain(&'a str),
    Unavailable,
}

#[must_use]
pub fn respond(request: Request<'_>) -> Payload {
    match request {
        Request::Usage => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": "Usage: retold [--plain | --json] [--quiet] [COMMAND]\n\nWithout a command in an interactive terminal, the full-screen app opens.\n\nCommands: home, video list|show|plan|continue|export|costs, account init|show|set-limits|limit, spend [--today|--month] [--by provider|video|series|day], config validate --root PATH, providers scan|add|route|test, storage backup --output PATH [--dry-run], storage verify <directory>, storage restore <archive> --into PATH [--dry-run], storage activate <root> --dry-run|--plan-hash HASH, schema, explain <code>\nContinue stops at ready; only export opens the editor or renders.",
                "commands": ["home", "video list", "video show <ref>", "video plan <ref>", "video continue <ref>", "video export <ref>", "video costs <ref>", "account show", "account limit <scope> <window>", "spend", "config validate --root PATH", "providers scan", "providers route --root PATH", "providers add <id> --root PATH --file PATH", "providers test [id] --root PATH", "storage backup --output PATH", "storage verify <directory>", "storage restore <archive> --into PATH", "storage activate <root> --dry-run", "schema", "explain <code>"],
                "usage": "retold [--plain | --json] [--quiet] [COMMAND]"
            }),
        },
        Request::Schema => dev::schema::default_events_schema(),
        Request::Explain(code) => explain(code),
        Request::Unavailable => Payload::Error {
            outcome: Outcome::Blocked,
            diagnostic: Diagnostic::new(
                ErrorCode::NotAvailable,
                "This action is not available in the current implementation phase.",
            ),
        },
    }
}

fn explain(code: &str) -> Payload {
    ErrorCode::parse(code).map_or_else(
        || Payload::Error {
            outcome: Outcome::Failed,
            diagnostic: Diagnostic::new(
                ErrorCode::InputRequired,
                "Unknown diagnostic code. Run schema to inspect supported error codes.",
            ),
        },
        |code| Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"code": code, "message": code.explanation()}),
        },
    )
}
