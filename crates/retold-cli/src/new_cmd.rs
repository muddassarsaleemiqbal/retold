//! New-video input parsing; all validation and draft creation belong to the application.
use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::creation::{self, Options};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

pub fn command() -> Command {
    let command = Command::new("new")
        .about("Create a video draft from your guide; sends no provider requests")
        .arg(Arg::new("guide").help("Installed guide; may be omitted when only one exists"));
    command.args(input_arguments()).args(production_arguments())
}

fn input_arguments() -> Vec<Arg> {
    let work = Arg::new("work").long("work");
    let named = Arg::new("input").long("input").action(ArgAction::Append);
    vec![
        work.visible_aliases(["title", "topic"])
            .help("Subject for the guide's title or topic input"),
        Arg::new("channel")
            .long("channel")
            .help("Installed channel; otherwise use the configured default or only channel"),
        named
            .value_name("NAME=VALUE")
            .help("A named guide input; repeat for multiple inputs"),
        Arg::new("kind").long("kind"),
        Arg::new("year").long("year"),
        Arg::new("scope").long("scope"),
    ]
}

fn production_arguments() -> Vec<Arg> {
    vec![
        Arg::new("lang")
            .long("lang")
            .action(ArgAction::Append)
            .help("Edition, such as en or hi:hi,en.srt; repeat for multiple editions"),
        Arg::new("quality")
            .long("quality")
            .value_parser(["lean", "standard", "premium"])
            .default_value("standard"),
        Arg::new("budget")
            .long("budget")
            .default_value("1.00")
            .help("Cumulative video ceiling in USD; grants no spend allowance"),
        Arg::new("source-file")
            .long("source-file")
            .value_parser(clap::value_parser!(PathBuf)),
        Arg::new("dry-run")
            .long("dry-run")
            .action(ArgAction::SetTrue)
            .help("Validate and preview without writing anything"),
    ]
}

pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    let root = root
        .cloned()
        .map_or_else(retold_app::library::default_library, Ok);
    root.and_then(|root| creation::create(&root, &options(args)?))
        .unwrap_or_else(Diagnostic::into_payload)
}

fn options(args: &ArgMatches) -> Result<Options, Diagnostic> {
    let text = |name| args.get_one::<String>(name).cloned();
    Ok(Options {
        guide: text("guide"),
        work: text("work"),
        channel: text("channel"),
        inputs: named_inputs(args)?,
        languages: args
            .get_many::<String>("lang")
            .map(|values| values.cloned().collect())
            .unwrap_or_default(),
        quality: text("quality"),
        budget: text("budget"),
        source_file: args.get_one::<PathBuf>("source-file").cloned(),
        dry_run: args.get_flag("dry-run"),
    })
}

fn named_inputs(args: &ArgMatches) -> Result<BTreeMap<String, String>, Diagnostic> {
    let mut inputs = BTreeMap::new();
    for pair in args.get_many::<String>("input").into_iter().flatten() {
        let (name, value) = pair
            .split_once('=')
            .ok_or_else(|| input_error("Use --input NAME=VALUE."))?;
        inputs = insert_input(inputs, name, value)?;
    }
    for name in ["kind", "year", "scope"] {
        if let Some(value) = args.get_one::<String>(name) {
            inputs = insert_input(inputs, name, value)?;
        }
    }
    Ok(inputs)
}

fn insert_input(
    mut inputs: BTreeMap<String, String>,
    name: &str,
    value: &str,
) -> Result<BTreeMap<String, String>, Diagnostic> {
    let repeated = inputs.insert(name.into(), value.into()).is_some();
    if repeated {
        return Err(input_error("A guide input was supplied more than once."));
    }
    Ok(inputs)
}

fn input_error(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCode::InputRequired, message)
}
