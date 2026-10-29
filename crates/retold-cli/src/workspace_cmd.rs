//! CLI commands for workspace management (`retold workspace`).

use std::path::PathBuf;

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};
use retold_app::workspace::{
    WorkspaceInitOptions, init_workspace, list_workspaces, switch_workspace,
};

pub fn command() -> Command {
    Command::new("workspace")
        .about("Workspace root registry, inspection, initialization, and switching")
        .subcommand_required(true)
        .subcommands([
            init_subcommand(),
            show_subcommand(),
            check_subcommand(),
            list_subcommand(),
            switch_subcommand(),
        ])
}

fn init_subcommand() -> Command {
    Command::new("init")
        .about("Initialize a new workspace root and scaffold standard structure")
        .args([
            Arg::new("path")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf))
                .help("Target directory for the new workspace"),
            Arg::new("name")
                .long("name")
                .help("Display name for the workspace"),
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue)
                .help("Preview paths and checks without creating files"),
        ])
}

fn show_subcommand() -> Command {
    Command::new("show")
        .about("Inspect workspace configuration, exact filesystem paths, and active status")
        .arg(workspace_arg())
}

fn check_subcommand() -> Command {
    Command::new("check")
        .about("Validate workspace health, writable space, directory layout, and registration")
        .arg(workspace_arg())
}

fn list_subcommand() -> Command {
    Command::new("list").about("List all registered and remembered workspaces")
}

fn switch_subcommand() -> Command {
    Command::new("switch")
        .about("Set the remembered default workspace for future commands")
        .arg(
            Arg::new("path")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf))
                .help("Workspace directory to switch to"),
        )
}

fn workspace_arg() -> Arg {
    Arg::new("workspace")
        .long("workspace")
        .value_parser(clap::value_parser!(PathBuf))
        .help("Explicit workspace root path")
}

fn input_error() -> Payload {
    Diagnostic::new(
        ErrorCode::InputRequired,
        "Invalid or incomplete workspace command options. See --help.",
    )
    .into_payload()
}

fn resolve_target_path(
    sub_args: &ArgMatches,
    global_workspace: Option<&PathBuf>,
) -> Option<PathBuf> {
    sub_args
        .get_one::<PathBuf>("workspace")
        .or(global_workspace)
        .cloned()
}

fn query_init(sub_args: &ArgMatches) -> Payload {
    let Some(path) = sub_args.get_one::<PathBuf>("path") else {
        return input_error();
    };
    let name = sub_args.get_one::<String>("name").cloned();
    let dry_run = sub_args.get_flag("dry-run");
    init_workspace(path, &WorkspaceInitOptions { name, dry_run })
}

fn query_switch(sub_args: &ArgMatches) -> Payload {
    let Some(path) = sub_args.get_one::<PathBuf>("path") else {
        return input_error();
    };
    switch_workspace(path)
}

pub fn query(global_workspace: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    match args.subcommand() {
        Some(("init", sub_args)) => query_init(sub_args),
        Some(("show", sub_args)) => {
            let target = resolve_target_path(sub_args, global_workspace);
            retold_app::workspace::context::Context::resolve(target.as_deref())
                .map_or_else(Diagnostic::into_payload, |context| {
                    retold_app::workspace::inspect::show_context(&context)
                })
        }
        Some(("check", sub_args)) => {
            let target = resolve_target_path(sub_args, global_workspace);
            retold_app::workspace::context::Context::resolve(target.as_deref())
                .map_or_else(Diagnostic::into_payload, |context| {
                    retold_app::workspace::check::check_context(&context)
                })
        }
        Some(("list", _)) => list_workspaces(),
        Some(("switch", sub_args)) => query_switch(sub_args),
        _ => input_error(),
    }
}
