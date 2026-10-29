//! CLI commands for managing export presets.

use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::library;
use retold_app::presets::{self, RemovalConsent};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

pub fn command() -> Command {
    let base = Command::new("export-presets")
        .alias("presets")
        .alias("preset");
    let described = base.about("Manage export presets for video deliverables");
    described.subcommand_required(true).subcommands([
        list_command(),
        show_command(),
        new_command(),
        remove_command(),
    ])
}

fn list_command() -> Command {
    Command::new("list").about("List all installed export presets")
}

fn show_command() -> Command {
    let base = Command::new("show");
    let described = base.about("Display specifications for an export preset");
    let name_arg = Arg::new("name")
        .required(true)
        .help("Preset name or identifier");
    described.arg(name_arg)
}

fn build_file_arg() -> Arg {
    let base = Arg::new("file")
        .long("file")
        .required_unless_present("aspect")
        .conflicts_with_all([
            "aspect",
            "container",
            "video-codec",
            "audio-codec",
            "captions",
        ]);
    let parsed = base.value_parser(clap::value_parser!(PathBuf));
    parsed.help("Path to preset JSON file")
}

fn new_command() -> Command {
    let base = Command::new("new");
    let described = base.about("Register a new export preset from a JSON specification file");
    let name_arg = Arg::new("name").required(true).help("Preset name");
    let file_arg = build_file_arg();
    described.args([name_arg, file_arg]).args(typed_arguments())
}

fn typed_arguments() -> Vec<Arg> {
    vec![
        Arg::new("aspect")
            .long("aspect")
            .action(ArgAction::Append)
            .value_parser(["16:9", "9:16", "1:1"])
            .help("Target aspect; repeat to include several"),
        Arg::new("container")
            .long("container")
            .value_parser(["mp4", "mov", "mkv"])
            .default_value("mp4")
            .help("Delivery file container"),
        Arg::new("video-codec")
            .long("video-codec")
            .value_parser(["h264", "h265", "av1", "prores"])
            .default_value("h264")
            .help("Video encoding"),
        Arg::new("audio-codec")
            .long("audio-codec")
            .value_parser(["aac", "pcm", "mp3", "opus"])
            .default_value("aac")
            .help("Audio encoding"),
        Arg::new("captions")
            .long("captions")
            .value_parser(["burned", "subtitles", "none"])
            .default_value("subtitles")
            .help("How captions are delivered"),
    ]
}

fn build_yes_arg() -> Arg {
    let base = Arg::new("yes").long("yes");
    let flagged = base.action(ArgAction::SetTrue);
    flagged.help("Confirm removal without interactive prompt")
}

fn remove_command() -> Command {
    let base = Command::new("remove");
    let described = base.about("Remove an existing export preset");
    let name_arg = Arg::new("name").required(true).help("Preset name");
    let yes_arg = build_yes_arg();
    described.args([name_arg, yes_arg])
}

#[must_use]
pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    let resolved_root = root.cloned().map_or_else(library::default_library, Ok);
    resolved_root.map_or_else(Diagnostic::into_payload, |library_path| {
        dispatch_presets_action(&library_path, args)
    })
}

fn dispatch_show(root: &Path, sub: &ArgMatches) -> Payload {
    let name = sub.get_one::<String>("name").map(String::as_str);
    name.map_or_else(
        || Diagnostic::new(ErrorCode::InputRequired, "Preset name is required.").into_payload(),
        |target_name| {
            presets::show_preset(root, target_name).unwrap_or_else(Diagnostic::into_payload)
        },
    )
}

fn dispatch_new(root: &Path, sub: &ArgMatches) -> Payload {
    let name = sub.get_one::<String>("name").map(String::as_str);
    let file = sub.get_one::<PathBuf>("file");
    match (name, file) {
        (Some(preset_name), Some(file_path)) => {
            presets::create_preset(root, preset_name, file_path)
                .unwrap_or_else(Diagnostic::into_payload)
        }
        (Some(name), None) => presets::create_preset_value(root, name, &preset_options(sub))
            .unwrap_or_else(Diagnostic::into_payload),
        _ => Diagnostic::new(
            ErrorCode::InputRequired,
            "Preset name and --file PATH are required.",
        )
        .into_payload(),
    }
}

/// A preset described with options instead of a JSON file.
fn preset_options(sub: &ArgMatches) -> presets::PresetFile {
    let field = |id| sub.get_one::<String>(id).cloned().unwrap_or_default();
    let aspects = sub.get_many::<String>("aspect").into_iter().flatten();
    presets::PresetFile {
        aspects: aspects.cloned().collect(),
        container: field("container"),
        video_codec: field("video-codec"),
        audio_codec: field("audio-codec"),
        caption_mode: field("captions"),
    }
}

fn dispatch_remove(root: &Path, sub: &ArgMatches) -> Payload {
    let name = sub.get_one::<String>("name").map(String::as_str);
    let confirmed = if sub.get_flag("yes") {
        RemovalConsent::Confirmed
    } else {
        RemovalConsent::Unconfirmed
    };
    name.map_or_else(
        || Diagnostic::new(ErrorCode::InputRequired, "Preset name is required.").into_payload(),
        |target_name| {
            presets::remove_preset(root, target_name, confirmed)
                .unwrap_or_else(Diagnostic::into_payload)
        },
    )
}

fn dispatch_presets_action(root: &Path, args: &ArgMatches) -> Payload {
    match args.subcommand() {
        Some(("list", _)) => presets::list_presets(root).unwrap_or_else(Diagnostic::into_payload),
        Some(("show", sub)) => dispatch_show(root, sub),
        Some(("new", sub)) => dispatch_new(root, sub),
        Some(("remove", sub)) => dispatch_remove(root, sub),
        _ => Diagnostic::new(
            ErrorCode::InputRequired,
            "Select an export-presets action (list, show, new, remove).",
        )
        .into_payload(),
    }
}
