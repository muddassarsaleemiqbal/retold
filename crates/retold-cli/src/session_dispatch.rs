//! A TUI session supplies its resolved paths; shared account commands keep global scope.
use clap::ArgMatches;
use retold_app::{
    protocol::{Diagnostic, Payload},
    workspace::context::Context,
};

type Handler = fn(&Context, &ArgMatches) -> Payload;
const HANDLERS: &[(&str, Handler)] = &[
    ("home", |context, _| {
        retold_app::home::respond_in_context(context)
    }),
    ("providers", |context, args| {
        super::providers::query(args, Some(&context.config))
    }),
    ("config", |context, args| {
        super::config_query(args, Some(&context.config))
    }),
    ("doctor", super::doctor_cmd::dispatch_doctor),
    ("setup", |context, _| {
        retold_app::setup::run_setup(context).unwrap_or_else(Diagnostic::into_payload)
    }),
    ("workspace", workspace),
];
const GLOBAL_COMMANDS: &[&str] = &[
    "account",
    "spend",
    "schema",
    "dev",
    "diagnostics",
    "completions",
    "man",
    "explain",
    "usage",
];

pub fn independent(matches: &ArgMatches) -> Option<Payload> {
    let Some((name, args)) = matches.subcommand() else {
        return Some(super::dispatch_global(matches));
    };
    let shared = GLOBAL_COMMANDS.contains(&name);
    if shared {
        return Some(super::dispatch_global(matches));
    }
    match name {
        "workspace" if matches!(args.subcommand_name(), Some("init" | "switch" | "list")) => {
            Some(super::workspace_cmd::query(Option::default(), args))
        }
        "providers" if args.subcommand_name() == Some("scan") => {
            Some(super::providers::query(args, Option::default()))
        }
        "config" | "providers" if explicit_root(args) => Some(super::dispatch_global(matches)),
        _ => None,
    }
}

fn explicit_root(args: &ArgMatches) -> bool {
    std::iter::successors(Some(args), |args| args.subcommand().map(|(_, sub)| sub)).any(|args| {
        let root = args.try_get_one::<std::path::PathBuf>("root");
        root.ok().flatten().is_some()
    })
}

pub fn scoped(context: &Context, matches: &ArgMatches) -> Payload {
    let Some((name, args)) = matches.subcommand() else {
        return super::dispatch_global(matches);
    };
    let handler = HANDLERS.iter().find(|(candidate, _)| *candidate == name);
    handler.map_or_else(
        || library(context, (name, args)).unwrap_or_else(|| super::dispatch_global(matches)),
        |(_, handler)| handler(context, args),
    )
}

fn library(context: &Context, command: (&str, &ArgMatches)) -> Option<Payload> {
    let (name, args) = command;
    super::LIBRARY_HANDLERS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, handler)| handler(Some(super::domain_root(context, name)), args))
}

fn workspace(context: &Context, args: &ArgMatches) -> Payload {
    match args.subcommand_name() {
        Some("show") => retold_app::workspace::inspect::show_context(context),
        Some("check") => retold_app::workspace::check::check_context(context),
        _ => super::workspace_cmd::query(Some(&context.root), args),
    }
}
