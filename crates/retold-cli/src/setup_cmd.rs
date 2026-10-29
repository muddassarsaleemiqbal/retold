//! CLI commands for first-run workspace onboarding and setup.

use std::path::PathBuf;

use clap::{ArgMatches, Command};
use retold_app::protocol::{Diagnostic, Payload};
use retold_app::setup;

pub fn command() -> Command {
    Command::new("setup").about("Initialize workspace directories and verify host providers")
}

#[must_use]
pub fn query(root: Option<&PathBuf>, _args: &ArgMatches) -> Payload {
    let context = retold_app::workspace::context::Context::resolve(root.map(PathBuf::as_path));
    context.map_or_else(Diagnostic::into_payload, |context| {
        setup::run_setup(&context).unwrap_or_else(Diagnostic::into_payload)
    })
}
