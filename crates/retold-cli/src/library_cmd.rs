//! CLI commands for user audio library management (music and SFX).

use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::audio::{self, AudioCategory, AudioImportSpec, AudioTagSpec};
use retold_app::library;
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

pub fn command() -> Command {
    Command::new("library")
        .about("Inspect and manage user audio assets (music and sound effects)")
        .subcommand_required(true)
        .subcommands([
            category_command("music", "Manage background music tracks"),
            category_command("sfx", "Manage sound effect assets"),
        ])
}

fn category_command(name: &'static str, description: &'static str) -> Command {
    Command::new(name)
        .about(description)
        .subcommand_required(true)
        .subcommands([
            import_command(),
            list_command(),
            tag_command(),
            licence_command(),
            check_command(),
        ])
}

fn import_command() -> Command {
    Command::new("import")
        .about("Import an audio file into the library")
        .args([
            Arg::new("path")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf))
                .help("Source audio file path"),
            value("title"),
            value("license"),
            value("attribution"),
            Arg::new("commercial")
                .long("commercial")
                .action(ArgAction::SetTrue)
                .help("Mark track as cleared for commercial use"),
            value("tags").help("Comma-separated tags"),
        ])
}

fn list_command() -> Command {
    Command::new("list").about("List audio assets in this category")
}

fn tag_command() -> Command {
    Command::new("tag")
        .about("Add tags to an audio asset")
        .args([
            Arg::new("id").required(true).help("Audio asset identifier"),
            Arg::new("tags")
                .required(true)
                .action(ArgAction::Append)
                .help("One or more tags to add"),
        ])
}

fn licence_command() -> Command {
    Command::new("licence")
        .about("Inspect licensing status and attribution requirements")
        .arg(Arg::new("id").help("Optional asset identifier"))
}

fn check_command() -> Command {
    Command::new("check")
        .about("Verify audio file integrity on disk")
        .arg(Arg::new("id").help("Optional asset identifier"))
}

fn value(name: &'static str) -> Arg {
    Arg::new(name).long(name)
}

#[must_use]
pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    let resolved_root = root.cloned().map_or_else(library::default_library, Ok);
    resolved_root.map_or_else(Diagnostic::into_payload, |library_path| {
        dispatch_category(&library_path, args)
    })
}

fn dispatch_category(root: &Path, args: &ArgMatches) -> Payload {
    let (category, sub_args) = match args.subcommand() {
        Some(("music", sub)) => (AudioCategory::Music, sub),
        Some(("sfx", sub)) => (AudioCategory::Sfx, sub),
        _ => {
            return Diagnostic::new(
                ErrorCode::InputRequired,
                "Select an audio category (music or sfx).",
            )
            .into_payload();
        }
    };
    dispatch_action(root, category, sub_args)
}

fn dispatch_action(root: &Path, category: AudioCategory, args: &ArgMatches) -> Payload {
    let result = match args.subcommand() {
        Some(("import", sub)) => import_asset(root, category, sub),
        Some(("list", _)) => audio::list(root, category),
        Some(("tag", sub)) => tag_asset(root, category, sub),
        Some(("licence", sub)) => inspect_licence(root, category, sub),
        Some(("check", sub)) => verify_category_assets(root, category, sub),
        _ => Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Select an action (import, list, tag, licence, check).",
        )),
    };
    result.unwrap_or_else(Diagnostic::into_payload)
}

fn parse_tags(sub: &ArgMatches) -> Vec<String> {
    let comma_tags = sub.get_one::<String>("tags");
    if let Some(joined) = comma_tags {
        return joined
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(String::from)
            .collect();
    }
    sub.get_many::<String>("tags")
        .map(|tags| tags.cloned().collect())
        .unwrap_or_default()
}

fn import_asset(
    root: &Path,
    category: AudioCategory,
    sub: &ArgMatches,
) -> Result<Payload, Diagnostic> {
    let source_path = sub
        .get_one::<PathBuf>("path")
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "Missing audio file path."))?;
    let tags = parse_tags(sub);
    let spec = AudioImportSpec {
        category,
        source_path,
        title: sub.get_one::<String>("title").map(String::as_str),
        license_type: sub.get_one::<String>("license").map(String::as_str),
        attribution: sub.get_one::<String>("attribution").map(String::as_str),
        commercial_use: sub.get_flag("commercial"),
        tags,
    };
    audio::import(root, &spec)
}

fn tag_asset(
    root: &Path,
    category: AudioCategory,
    sub: &ArgMatches,
) -> Result<Payload, Diagnostic> {
    let id = sub
        .get_one::<String>("id")
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "Missing audio ID."))?;
    let tags = parse_tags(sub);
    let spec = AudioTagSpec { category, id, tags };
    audio::tag(root, &spec)
}

fn inspect_licence(
    root: &Path,
    category: AudioCategory,
    sub: &ArgMatches,
) -> Result<Payload, Diagnostic> {
    let target_id = sub.get_one::<String>("id").map(String::as_str);
    audio::licence(root, category, target_id)
}

fn verify_category_assets(
    root: &Path,
    category: AudioCategory,
    sub: &ArgMatches,
) -> Result<Payload, Diagnostic> {
    let target_id = sub.get_one::<String>("id").map(String::as_str);
    audio::check(root, category, target_id)
}
