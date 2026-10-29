//! The Home screen's snapshot. Each section is exactly the data an existing read-only command
//! returns (`video list`, `providers health`, `spend`), with that command beside it, so the app,
//! plain output and JSON always agree. A failing section carries its diagnostic; the others still
//! show. Nothing here writes files, contacts providers or grants authority.
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

use crate::account::report::{self, Grouping, Query};
use crate::account::types::Window;
use crate::library::{self, ListOptions};
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

/// Recent videos shown on Home; the full list is one command away.
pub const RECENT_VIDEOS: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct HomeOptions {
    /// An explicit library instead of the configured one.
    pub library: Option<PathBuf>,
}

/// A Home action: what it runs, and whether the command is implemented.
#[derive(Debug, Clone, Serialize)]
pub struct Action {
    pub group: &'static str,
    pub label: &'static str,
    pub command: String,
    pub available: bool,
}

#[must_use]
pub fn respond(options: &HomeOptions) -> Payload {
    let context = crate::workspace::context::Context::resolve(options.library.as_deref());
    snapshot(context.as_ref().ok())
}

#[must_use]
pub fn respond_in_context(context: &crate::workspace::context::Context) -> Payload {
    snapshot(Some(context))
}

fn snapshot(context: Option<&crate::workspace::context::Context>) -> Payload {
    let library = context.map(|context| context.library.as_path());
    let config = context.map(|context| context.config.as_path());
    let home = json!({
        "videos": videos(library),
        "providers": providers(config),
        "today": spend(Window::Day, "retold spend --today"),
        "month": spend(Window::Month, "retold spend --month"),
        "actions": actions(config),
    });
    Payload::Result {
        outcome: Outcome::Completed,
        contents: json!({ "home": home }),
    }
}

/// The configured settings folder, when settings can be read.
#[must_use]
pub fn config_root() -> Option<PathBuf> {
    retold_engine::Paths::discover()
        .ok()
        .map(|paths| paths.config)
}

fn videos(library: Option<&Path>) -> Value {
    let command = format!("retold video list --limit {RECENT_VIDEOS}");
    let Some(library) = library else {
        return failed(&command, &unreadable_settings());
    };
    let options = ListOptions {
        limit: Some(RECENT_VIDEOS),
        ..ListOptions::default()
    };
    section(
        &command,
        library::query(library, library::Query::List(options)),
    )
}

fn providers(config: Option<&Path>) -> Value {
    let Some(config) = config else {
        return failed("retold providers health", &unreadable_settings());
    };
    let command = format!("retold providers health --root {}", word(config));
    section(&command, crate::providers::health(config))
}

fn spend(window: Window, command: &str) -> Value {
    let query = Query {
        window,
        by: Grouping::Provider,
    };
    section(command, report::respond(query))
}

fn section(command: &str, payload: Payload) -> Value {
    match payload {
        Payload::Result { contents, .. } => json!({ "command": command, "data": contents }),
        Payload::Error { diagnostic, .. } => failed(command, &diagnostic),
        Payload::Event(_) => failed(
            command,
            &Diagnostic::new(ErrorCode::ExecutionFailed, "Unexpected event output."),
        ),
    }
}

fn failed(command: &str, diagnostic: &Diagnostic) -> Value {
    json!({ "command": command, "error": diagnostic })
}

fn unreadable_settings() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::DataInvalid,
        "Settings could not be read. Supply --library PATH or repair the settings file.",
    )
}

fn actions(config: Option<&Path>) -> Vec<Action> {
    let root = config.map_or_else(|| "CONFIG_DIR".into(), word);
    let action = |group, label, command: String, available| Action {
        group,
        label,
        command,
        available,
    };
    vec![
        action("new", "New video", "retold new".into(), true),
        action("new", "New series", "retold series new".into(), true),
        action("library", "Videos", "retold video list".into(), true),
        action("library", "Spend", "retold spend".into(), true),
        action(
            "setup",
            "Providers",
            format!("retold providers list --root {root}"),
            true,
        ),
        action("setup", "Account", "retold account show".into(), true),
        action("setup", "Doctor", "retold doctor".into(), true),
    ]
}

/// A path as one shell word, quoted when it needs to be.
fn word(path: &Path) -> String {
    let text = path.display().to_string();
    let plain = text
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "/._-~".contains(character));
    if plain {
        text
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}
