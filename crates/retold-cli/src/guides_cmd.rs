//! CLI commands for inspecting installed creative guides.

use std::path::{Path, PathBuf};

use clap::{Arg, ArgMatches, Command};
use retold_app::creative;
use retold_app::library;
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

pub fn command() -> Command {
    let base = Command::new("guides").alias("guide");
    let described = base.about("Inspect installed creative production guides");
    described
        .subcommand_required(true)
        .subcommands([list_command(), show_command()])
        .subcommands(crate::authoring_cmd::commands("guides"))
}

fn list_command() -> Command {
    Command::new("list").about("List all installed guides in the workspace")
}

fn show_command() -> Command {
    let base = Command::new("show");
    let described = base.about("Display detailed specifications for an installed guide");
    let guide_arg = Arg::new("guide")
        .required(true)
        .help("Guide identifier or slug");
    described.arg(guide_arg)
}

#[must_use]
pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    let resolved_root = root.cloned().map_or_else(library::default_library, Ok);
    resolved_root.map_or_else(Diagnostic::into_payload, |library_path| {
        dispatch_guides_action(&library_path, args)
    })
}

fn dispatch_guides_action(root: &Path, args: &ArgMatches) -> Payload {
    if let Some(payload) = crate::authoring_cmd::query(root, "guides", args) {
        return payload;
    }
    match args.subcommand() {
        Some(("list", _)) => creative::list_guides(root).unwrap_or_else(Diagnostic::into_payload),
        Some(("show", sub)) => {
            let guide_id = sub.get_one::<String>("guide").map(String::as_str);
            guide_id.map_or_else(
                || {
                    Diagnostic::new(ErrorCode::InputRequired, "Guide identifier is required.")
                        .into_payload()
                },
                |target_id| {
                    creative::show_guide(root, target_id).unwrap_or_else(Diagnostic::into_payload)
                },
            )
        }
        _ => Diagnostic::new(
            ErrorCode::InputRequired,
            "Select a guides action (list or show).",
        )
        .into_payload(),
    }
}
