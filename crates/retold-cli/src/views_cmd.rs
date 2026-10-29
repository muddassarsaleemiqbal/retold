//! CLI commands for managing saved library views.

use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::library;
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};
use retold_app::views::{self, RemovalConsent};

pub fn command() -> Command {
    let base = Command::new("views").alias("view");
    let described = base.about("Manage saved search, tag, and sort views for library browsing");
    described.subcommand_required(true).subcommands([
        list_command(),
        show_command(),
        save_command(),
        remove_command(),
    ])
}

fn list_command() -> Command {
    Command::new("list").about("List all saved library views")
}

fn show_command() -> Command {
    let base = Command::new("show");
    let described = base.about("Display specifications for a saved view");
    let name_arg = Arg::new("name").required(true).help("Saved view name");
    described.arg(name_arg)
}

fn build_filter_file_arg() -> Arg {
    let base = Arg::new("filter-file")
        .long("filter-file")
        .visible_alias("file")
        .conflicts_with_all(["search", "tag", "channel", "archive", "sort", "limit"]);
    let parsed = base.value_parser(clap::value_parser!(PathBuf));
    parsed.help("Path to filter specification JSON file")
}

fn save_command() -> Command {
    let base = Command::new("save").alias("new");
    let described = base.about("Save a named library view from a filter JSON file");
    let name_arg = Arg::new("name").required(true).help("View name");
    let file_arg = build_filter_file_arg();
    described.args([name_arg, file_arg]).args(typed_arguments())
}

fn typed_arguments() -> Vec<Arg> {
    vec![
        Arg::new("search")
            .long("search")
            .help("Text to match in the video library"),
        Arg::new("tag").long("tag").help("Tag to match"),
        Arg::new("channel").long("channel").help("Channel to match"),
        Arg::new("sort").long("sort").help("Saved sort order"),
        Arg::new("archive")
            .long("archive")
            .action(ArgAction::SetTrue)
            .help("Include archived videos"),
        Arg::new("limit")
            .long("limit")
            .value_parser(clap::value_parser!(usize))
            .help("Maximum number of videos"),
    ]
}

fn build_yes_arg() -> Arg {
    let base = Arg::new("yes").long("yes");
    let flagged = base.action(ArgAction::SetTrue);
    flagged.help("Confirm removal without interactive prompt")
}

fn remove_command() -> Command {
    let base = Command::new("remove").alias("rm").alias("delete");
    let described = base.about("Remove an existing saved view");
    let name_arg = Arg::new("name").required(true).help("View name");
    let yes_arg = build_yes_arg();
    described.args([name_arg, yes_arg])
}

#[must_use]
pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    let resolved_root = root.cloned().map_or_else(library::default_library, Ok);
    resolved_root.map_or_else(Diagnostic::into_payload, |library_path| {
        dispatch_views_action(&library_path, args)
    })
}

fn dispatch_show(root: &Path, sub: &ArgMatches) -> Payload {
    let name = sub.get_one::<String>("name").map(String::as_str);
    name.map_or_else(
        || Diagnostic::new(ErrorCode::InputRequired, "View name is required.").into_payload(),
        |target_name| views::show_view(root, target_name).unwrap_or_else(Diagnostic::into_payload),
    )
}

fn dispatch_save(root: &Path, sub: &ArgMatches) -> Payload {
    let name = sub.get_one::<String>("name").map(String::as_str);
    let file = sub.get_one::<PathBuf>("filter-file");
    match (name, file) {
        (Some(view_name), Some(file_path)) => {
            views::save_view(root, view_name, file_path).unwrap_or_else(Diagnostic::into_payload)
        }
        (Some(name), None) => {
            let text = |id| sub.get_one::<String>(id).cloned();
            let filter = views::ViewFilter {
                search: text("search"),
                tag: text("tag"),
                channel: text("channel"),
                sort: text("sort"),
                archive: sub.get_flag("archive").then_some(true),
                limit: sub.get_one::<usize>("limit").copied(),
                columns: None,
            };
            views::save_filter(root, name, &filter).unwrap_or_else(Diagnostic::into_payload)
        }
        _ => Diagnostic::new(
            ErrorCode::InputRequired,
            "View name and --filter-file PATH are required.",
        )
        .into_payload(),
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
        || Diagnostic::new(ErrorCode::InputRequired, "View name is required.").into_payload(),
        |target_name| {
            views::remove_view(root, target_name, confirmed)
                .unwrap_or_else(Diagnostic::into_payload)
        },
    )
}

fn dispatch_views_action(root: &Path, args: &ArgMatches) -> Payload {
    match args.subcommand() {
        Some(("list", _)) => views::list_views(root).unwrap_or_else(Diagnostic::into_payload),
        Some(("show", sub)) => dispatch_show(root, sub),
        Some(("save" | "new", sub)) => dispatch_save(root, sub),
        _ => dispatch_views_fallback(root, args),
    }
}

fn dispatch_views_fallback(root: &Path, args: &ArgMatches) -> Payload {
    match args.subcommand() {
        Some(("remove" | "rm" | "delete", sub)) => dispatch_remove(root, sub),
        _ => Diagnostic::new(
            ErrorCode::InputRequired,
            "Select a views action (list, show, save, remove).",
        )
        .into_payload(),
    }
}
