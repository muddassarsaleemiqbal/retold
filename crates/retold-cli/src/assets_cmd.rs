//! CLI commands for browsing assets, verifying cryptographic integrity, and inspecting usages.

use std::path::{Path, PathBuf};

use clap::{Arg, ArgMatches, Command};
use retold_app::assets::{self, AssetsQuerySpec};
use retold_app::library;
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

pub fn command() -> Command {
    let base = Command::new("assets");
    let described = base.about("Browse assets, verify cryptographic integrity, and inspect usages");
    described.subcommand_required(true).subcommands([
        list_command(),
        show_command(),
        verify_command(),
        uses_command(),
    ])
}

fn list_command() -> Command {
    Command::new("list")
        .about("List media assets across the workspace, scoped by video or series")
        .args([
            Arg::new("video")
                .long("video")
                .help("Scope assets to a specific video reference"),
            Arg::new("series")
                .long("series")
                .help("Scope assets to a specific series reference"),
            Arg::new("kind")
                .long("kind")
                .help("Filter assets by kind: image, audio, video, subtitle, or other"),
        ])
}

fn show_command() -> Command {
    Command::new("show")
        .about("Inspect detailed asset metadata with provenance and take details")
        .arg(
            Arg::new("asset-id")
                .required(true)
                .help("Asset identifier, relative path, or filename"),
        )
}

fn verify_command() -> Command {
    Command::new("verify")
        .about("Cryptographically verify asset hashes against manifest checksums")
        .arg(
            Arg::new("asset-id")
                .required(true)
                .help("Asset identifier, relative path, or filename"),
        )
}

fn uses_command() -> Command {
    Command::new("uses")
        .about("Inspect all dependents and production references to an asset")
        .arg(
            Arg::new("asset-id")
                .required(true)
                .help("Asset identifier, relative path, or filename"),
        )
}

#[must_use]
pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    let resolved_root = root.cloned().map_or_else(library::default_library, Ok);
    resolved_root.map_or_else(Diagnostic::into_payload, |library_path| {
        dispatch_assets_action(&library_path, args)
    })
}

fn dispatch_inspection_action(root: &Path, sub: &ArgMatches, name: &str) -> Option<Payload> {
    match name {
        "show" => Some(dispatch_target_action(root, sub, assets::show_asset)),
        "verify" => Some(dispatch_target_action(root, sub, assets::verify_asset)),
        "uses" => Some(dispatch_target_action(root, sub, assets::uses_asset)),
        _ => None,
    }
}

fn dispatch_assets_action(root: &Path, args: &ArgMatches) -> Payload {
    if let Some(("list", sub)) = args.subcommand() {
        return dispatch_list(root, sub);
    }
    let inspected = args
        .subcommand()
        .and_then(|(name, sub)| dispatch_inspection_action(root, sub, name));
    if let Some(payload) = inspected {
        return payload;
    }
    Diagnostic::new(
        ErrorCode::InputRequired,
        "An assets subcommand is required: list, show, verify, or uses.",
    )
    .into_payload()
}

fn dispatch_list(root: &Path, sub: &ArgMatches) -> Payload {
    let spec = parse_assets_spec(sub);
    assets::list_assets(root, &spec).unwrap_or_else(Diagnostic::into_payload)
}

fn parse_assets_spec(sub: &ArgMatches) -> AssetsQuerySpec<'_> {
    let video = sub.get_one::<String>("video").map(String::as_str);
    let series = sub.get_one::<String>("series").map(String::as_str);
    let kind = sub.get_one::<String>("kind").map(String::as_str);
    AssetsQuerySpec {
        video,
        series,
        kind,
    }
}

fn dispatch_target_action(
    root: &Path,
    sub: &ArgMatches,
    action: fn(&Path, &str) -> Result<Payload, Diagnostic>,
) -> Payload {
    let target = sub.get_one::<String>("asset-id").map(String::as_str);
    target.map_or_else(
        || Diagnostic::new(ErrorCode::InputRequired, "Asset ID is required.").into_payload(),
        |asset_ref| action(root, asset_ref).unwrap_or_else(Diagnostic::into_payload),
    )
}
