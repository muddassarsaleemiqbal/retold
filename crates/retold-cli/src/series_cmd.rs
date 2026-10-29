//! CLI commands for multi-part series management and registry.

use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::library;
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};
use retold_app::series::{self, SeriesSpec};

type SeriesHandler = fn(&Path, &ArgMatches) -> Result<Payload, Diagnostic>;

const SERIES_HANDLERS: &[(&str, SeriesHandler)] = &[
    ("new", create_series),
    ("list", catalog_series),
    ("show", inspect_series),
    ("plan", plan_series_action),
    ("run", schedule_series_action),
    ("update", modify_series),
];

pub fn command() -> Command {
    Command::new("series")
        .about("Inspect and manage multi-part video series")
        .subcommand_required(true)
        .subcommands([
            new_command(),
            list_command(),
            show_command(),
            plan_command(),
            run_command(),
            update_command(),
        ])
}

pub fn new_command() -> Command {
    Command::new("new")
        .about("Create a new series registry entry")
        .args([
            Arg::new("guide")
                .required(true)
                .help("Creative guide identifier"),
            Arg::new("work")
                .long("work")
                .required(true)
                .help("Franchise or work title"),
            Arg::new("kind")
                .long("kind")
                .default_value("film")
                .help("Kind of work (film, series, or book)"),
            Arg::new("channel").long("channel").help("Target channel"),
            Arg::new("parts")
                .long("parts")
                .value_parser(clap::value_parser!(usize))
                .help("Initial number of parts"),
            Arg::new("budget")
                .long("budget")
                .help("Overall series budget in USD"),
        ])
}

fn list_command() -> Command {
    Command::new("list").about("List all registered series in the library")
}

fn show_command() -> Command {
    Command::new("show")
        .about("Inspect a series and its part completion matrix")
        .arg(
            Arg::new("series")
                .required(true)
                .help("Series identifier or slug"),
        )
}

fn plan_command() -> Command {
    Command::new("plan")
        .about("Review real generation plans for linked series parts")
        .args(series_selection())
        .args(series_allowance())
}

fn series_selection() -> Vec<Arg> {
    vec![
        Arg::new("series").required(true).help("Series identifier"),
        Arg::new("parts")
            .long("parts")
            .help("Comma-separated unique part IDs"),
    ]
}

fn series_allowance() -> Vec<Arg> {
    vec![
        Arg::new("spend")
            .long("spend")
            .help("Total USD allowance, divided across selected parts; default $0"),
        Arg::new("max-requests")
            .long("max-requests")
            .value_parser(clap::value_parser!(u64))
            .help("Total paid request allowance, divided across selected parts"),
    ]
}

fn run_command() -> Command {
    let run =
        Command::new("run").about("Submit linked parts as real studio jobs after plan review");
    run.args(series_selection()).args(series_allowance()).args([
        Arg::new("plan-hash")
            .long("plan-hash")
            .help("Exact reviewed series plan hash"),
        Arg::new("yes")
            .long("yes")
            .action(ArgAction::SetTrue)
            .help("Accept the current plan without increasing allowances"),
    ])
}

fn link_part() -> Arg {
    let part = Arg::new("part").long("part").requires("video");
    part.conflicts_with("add-parts")
        .help("Part ID to link to an existing video")
}

fn update_command() -> Command {
    Command::new("update")
        .about("Update series metadata or append new parts")
        .args([
            Arg::new("series")
                .required(true)
                .help("Series identifier or slug"),
            link_part(),
            Arg::new("video")
                .long("video")
                .requires("part")
                .help("Existing video reference to link"),
            Arg::new("add-parts")
                .long("add-parts")
                .value_parser(clap::value_parser!(usize))
                .help("Number of additional parts to append"),
        ])
}

#[must_use]
pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    let resolved_root = root.cloned().map_or_else(library::default_library, Ok);
    resolved_root.map_or_else(Diagnostic::into_payload, |library_path| {
        dispatch_action(&library_path, args)
    })
}

fn dispatch_action(root: &Path, args: &ArgMatches) -> Payload {
    let Some((sub_name, sub_matches)) = args.subcommand() else {
        return Diagnostic::new(ErrorCode::InputRequired, "Select a series action.").into_payload();
    };
    for &(name, handler) in SERIES_HANDLERS {
        if name == sub_name {
            return handler(root, sub_matches).unwrap_or_else(Diagnostic::into_payload);
        }
    }
    Diagnostic::new(ErrorCode::InputRequired, "Unknown series action.").into_payload()
}

pub fn parse_series_spec(sub: &ArgMatches) -> SeriesSpec<'_> {
    let guide = sub.get_one::<String>("guide").map_or("", String::as_str);
    let work = sub.get_one::<String>("work").map_or("", String::as_str);
    let kind = sub.get_one::<String>("kind").map_or("film", String::as_str);
    let channel = sub.get_one::<String>("channel").map(String::as_str);
    let parts_count = sub.get_one::<usize>("parts").copied().unwrap_or(1);
    let budget_micros = sub.get_one::<String>("budget").and_then(|text| {
        text.parse::<retold_app::video::Usd>()
            .ok()
            .map(|usd| usd.micros().cast_unsigned())
    });
    SeriesSpec {
        guide,
        work,
        kind,
        channel,
        budget_micros,
        parts_count,
    }
}

fn create_series(root: &Path, sub: &ArgMatches) -> Result<Payload, Diagnostic> {
    let spec = parse_series_spec(sub);
    series::new_series(root, &spec)
}

fn catalog_series(root: &Path, _sub: &ArgMatches) -> Result<Payload, Diagnostic> {
    series::list_series(root)
}

fn inspect_series(root: &Path, sub: &ArgMatches) -> Result<Payload, Diagnostic> {
    let target = sub
        .get_one::<String>("series")
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "Series ID is required."))?;
    series::show_series(root, target)
}

fn plan_series_action(root: &Path, sub: &ArgMatches) -> Result<Payload, Diagnostic> {
    series::execution::plan(series_target(root, sub)?, &execution_options(sub))
}

fn schedule_series_action(root: &Path, sub: &ArgMatches) -> Result<Payload, Diagnostic> {
    series::execution::run(series_target(root, sub)?, &execution_options(sub))
}

fn series_target<'a>(
    root: &'a Path,
    sub: &'a ArgMatches,
) -> Result<series::execution::Target<'a>, Diagnostic> {
    let id = sub
        .get_one::<String>("series")
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "Series ID is required."))?;
    let parts = sub.get_one::<String>("parts").map(String::as_str);
    Ok(series::execution::Target { root, id, parts })
}

fn modify_series(root: &Path, sub: &ArgMatches) -> Result<Payload, Diagnostic> {
    let target = sub
        .get_one::<String>("series")
        .ok_or_else(|| Diagnostic::new(ErrorCode::InputRequired, "Series ID is required."))?;
    if let (Some(part), Some(video)) = (
        sub.get_one::<String>("part"),
        sub.get_one::<String>("video"),
    ) {
        return series::link_video(root, target, (part, video));
    }
    let add_parts = sub.get_one::<usize>("add-parts").copied().unwrap_or(0);
    series::update_series(root, target, add_parts)
}

fn execution_options(sub: &ArgMatches) -> series::execution::Options {
    let running = sub
        .try_get_one::<String>("plan-hash")
        .ok()
        .flatten()
        .cloned();
    let yes = sub
        .try_get_one::<bool>("yes")
        .ok()
        .flatten()
        .copied()
        .unwrap_or_default();
    series::execution::Options {
        spend: sub.get_one::<String>("spend").cloned(),
        max_requests: sub.get_one::<u64>("max-requests").copied(),
        plan_hash: running,
        yes,
    }
}
