//! CLI commands for inspecting installed channel brand profiles.

use std::path::{Path, PathBuf};

use clap::{Arg, ArgMatches, Command};
use retold_app::creative;
use retold_app::library;
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

pub fn command() -> Command {
    let base = Command::new("channels").alias("channel");
    let described = base.about("Inspect installed channel brand profiles");
    described
        .subcommand_required(true)
        .subcommands([list_command(), show_command()])
        .subcommands(crate::authoring_cmd::commands("channels"))
}

fn list_command() -> Command {
    Command::new("list").about("List all installed channels in the workspace")
}

fn show_command() -> Command {
    let base = Command::new("show");
    let described = base.about("Display brand profile details for an installed channel");
    let channel_arg = Arg::new("channel")
        .required(true)
        .help("Channel identifier or slug");
    described.arg(channel_arg)
}

#[must_use]
pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    let resolved_root = root.cloned().map_or_else(library::default_library, Ok);
    resolved_root.map_or_else(Diagnostic::into_payload, |library_path| {
        dispatch_channels_action(&library_path, args)
    })
}

fn dispatch_channels_action(root: &Path, args: &ArgMatches) -> Payload {
    if let Some(payload) = crate::authoring_cmd::query(root, "channels", args) {
        return payload;
    }
    match args.subcommand() {
        Some(("list", _)) => creative::list_channels(root).unwrap_or_else(Diagnostic::into_payload),
        Some(("show", sub)) => {
            let channel_id = sub.get_one::<String>("channel").map(String::as_str);
            channel_id.map_or_else(
                || {
                    Diagnostic::new(ErrorCode::InputRequired, "Channel identifier is required.")
                        .into_payload()
                },
                |target_id| {
                    creative::show_channel(root, target_id).unwrap_or_else(Diagnostic::into_payload)
                },
            )
        }
        _ => Diagnostic::new(
            ErrorCode::InputRequired,
            "Select a channels action (list or show).",
        )
        .into_payload(),
    }
}
