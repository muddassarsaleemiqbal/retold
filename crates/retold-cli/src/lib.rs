//! Shared entry point for the Retold CLI and its transitional alias.

mod account;
mod argument_help;
mod assets_cmd;
mod authoring_cmd;
mod channels_cmd;
mod completions_cmd;
mod creative_cmd;
mod doctor_cmd;
mod foreground;
mod guides_cmd;
mod home;
mod human;
mod interrupt;
mod library_cmd;
mod man_cmd;
mod new_cmd;
mod output;
mod presets_cmd;
mod providers;
mod queue;
mod script_cmd;
mod series_cmd;
mod session_dispatch;
mod setup_cmd;
mod spend;
mod storage;
mod studio;
mod tui;
mod video;
mod video_screen;
mod views_cmd;
mod workspace_cmd;

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Arg, ArgAction, Command, error::ErrorKind};
use retold_app::protocol::{Diagnostic, Envelope, ErrorCode, Outcome, Payload};
use retold_app::{Request, respond};

fn domain_subcommands() -> Vec<Command> {
    vec![
        workspace_cmd::command(),
        creative_cmd::command(),
        video::command(),
        assets_cmd::command(),
        library_cmd::command(),
        series_cmd::command(),
        new_cmd::command(),
        doctor_cmd::command(),
        setup_cmd::command(),
        guides_cmd::command(),
        presets_cmd::command(),
        views_cmd::command(),
        channels_cmd::command(),
        account::command(),
        providers::command(),
        spend::command(),
        storage::command(),
        studio::command(),
        queue::command(),
    ]
}

fn register_domain_subcommands(cmd: Command) -> Command {
    cmd.subcommands(domain_subcommands())
}

fn leaf_paths(command: &Command, prefix: &str) -> Vec<String> {
    command
        .get_subcommands()
        .flat_map(|sub| {
            let path = if prefix.is_empty() {
                sub.get_name().to_owned()
            } else {
                format!("{prefix} {}", sub.get_name())
            };
            let leaf = sub.get_subcommands().next().is_none();
            if leaf {
                vec![path]
            } else {
                leaf_paths(sub, &path)
            }
        })
        .collect()
}

/// Every runnable operation as a space-separated command path, such as `account attempts`.
#[must_use]
pub fn operations() -> Vec<String> {
    leaf_paths(&command(), "")
}

pub mod forms;

/// The complete command tree, for inventory and coverage checks.
#[must_use]
pub fn command_tree() -> Command {
    command()
}

fn command() -> Command {
    let base = Command::new("retold")
        .bin_name("retold")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Retold: saved videos and shared account administration");
    let base = base.color(clap::ColorChoice::Never).args([
        switch("json").conflicts_with("plain"),
        switch("plain"),
        switch("quiet"),
        switch("no-color"),
        library_arg(),
        workspace_global_arg(),
    ]);
    let with_domains = register_domain_subcommands(base.subcommands(basic_commands()));
    let with_config = with_domains.subcommand(config_command());
    let with_dev = with_config.subcommand(dev_command());
    let with_unavailable = with_dev.subcommands(unavailable_commands());
    argument_help::complete(with_unavailable)
        .after_help("Examples:\n  retold --json schema\n  retold explain RG-NOT-AVAILABLE")
}

fn basic_commands() -> Vec<Command> {
    vec![
        Command::new("schema").about("Print the versioned machine-output schema"),
        Command::new("diagnostics").about("Browse diagnostic codes and their explanations"),
        Command::new("usage").about("Print CLI usage and available commands"),
        Command::new("home").about(
            "Print the Home screen's providers, spend, recent videos and actions; read only",
        ),
        Command::new("explain")
            .about("Explain a diagnostic code")
            .arg(Arg::new("code").required(true)),
        completions_cmd::command(),
        man_cmd::command(),
    ]
}

fn dev_command() -> Command {
    let mut kind_arg = Arg::new("kind");
    kind_arg = kind_arg.long("kind");
    kind_arg = kind_arg.value_parser(["events", "manifest", "commands", "config"]);
    kind_arg = kind_arg.help("Kind of schema to print (events, manifest, commands, config)");

    let mut schema_sub = Command::new("schema");
    schema_sub = schema_sub
        .about("Print versioned JSON schemas for machine integration and assistive tooling");
    schema_sub = schema_sub.arg(kind_arg);

    Command::new("dev")
        .about("Developer and schema tooling")
        .subcommand(schema_sub)
}

fn config_command() -> Command {
    Command::new("config")
        .about("Inspect and migrate configuration without contacting providers")
        .subcommand_required(true)
        .subcommands([
            Command::new("validate").arg(
                Arg::new("root")
                    .long("root")
                    .value_parser(clap::value_parser!(std::path::PathBuf)),
            ),
            Command::new("migrate").args([
                Arg::new("root")
                    .long("root")
                    .value_parser(clap::value_parser!(std::path::PathBuf)),
                Arg::new("dry-run")
                    .long("dry-run")
                    .action(ArgAction::SetTrue),
            ]),
            profile_command(),
        ])
}

fn profile_command() -> Command {
    Command::new("profile")
        .about("Manage reproducible preference profiles")
        .subcommand_required(true)
        .subcommands([
            Command::new("list")
                .about("List installed preference profiles and show active profile")
                .arg(config_root_arg()),
            Command::new("show")
                .about("Inspect a named preference profile")
                .args([Arg::new("name").required(true), config_root_arg()]),
            Command::new("new")
                .about("Create a new preference profile")
                .args([
                    Arg::new("name").required(true),
                    Arg::new("file")
                        .long("file")
                        .value_parser(clap::value_parser!(std::path::PathBuf)),
                    config_root_arg(),
                ]),
            Command::new("use")
                .about("Switch the active configuration profile")
                .args([Arg::new("name").required(true), config_root_arg()]),
        ])
}

fn config_root_arg() -> Arg {
    Arg::new("root")
        .long("root")
        .value_parser(clap::value_parser!(std::path::PathBuf))
}

fn library_arg() -> Arg {
    let argument = Arg::new("library").long("library").global(true);
    argument
        .env("RETOLD_LIBRARY")
        .value_parser(clap::value_parser!(std::path::PathBuf))
}

fn workspace_global_arg() -> Arg {
    let argument = Arg::new("workspace").long("workspace").global(true);
    argument
        .env("RETOLD_WORKSPACE")
        .value_parser(clap::value_parser!(std::path::PathBuf))
}

fn switch(name: &'static str) -> Arg {
    let flag = Arg::new(name).long(name);
    flag.global(true).action(ArgAction::SetTrue)
}

fn unavailable_commands() -> impl Iterator<Item = Command> {
    std::iter::once("publish").map(|name| {
        let operands = Arg::new("arguments").num_args(0..).trailing_var_arg(true);
        Command::new(name)
            .about("Unavailable until its implementation phase")
            .arg(operands.allow_hyphen_values(true))
    })
}

/// Run one command in process exactly as the CLI dispatches it, returning the outcome and its
/// plain-text rendering. Terminal-only modes (full-screen app, live attach) are not reachable.
#[must_use]
pub fn run_operation(arguments: &[String]) -> (Payload, String) {
    let args: Vec<OsString> = std::iter::once(OsString::from("retold"))
        .chain(arguments.iter().map(OsString::from))
        .collect();
    let payload = request(args);
    let text = output::plain(&payload).unwrap_or_default();
    (payload, text)
}

fn request(args: Vec<OsString>) -> Payload {
    request_using(args, dispatch)
}

fn request_using(
    args: Vec<OsString>,
    dispatch: impl FnOnce(&clap::ArgMatches) -> Payload,
) -> Payload {
    match command().try_get_matches_from(args) {
        Ok(matches) => dispatch(&matches),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Payload::Result {
                outcome: Outcome::Completed,
                contents: serde_json::json!({"text": error.to_string()}),
            }
        }
        Err(_) => input_error(),
    }
}

fn dispatch(matches: &clap::ArgMatches) -> Payload {
    let root = selected_root(matches);
    let library_payload = matches
        .subcommand()
        .and_then(|(name, args)| dispatch_library_command(root, name, args));
    if let Some(payload) = library_payload {
        return payload;
    }
    dispatch_global(matches)
}

/// An environment library override changes media location, not the configuration root.
/// `Paths::discover` resolves that override when there is no explicit root selection.
fn selected_root(matches: &clap::ArgMatches) -> Option<&PathBuf> {
    ["workspace", "library"]
        .into_iter()
        .find(|name| matches.value_source(name) == Some(clap::parser::ValueSource::CommandLine))
        .and_then(|name| matches.get_one::<PathBuf>(name))
        .or_else(|| matches.get_one::<PathBuf>("workspace"))
}

type LibraryHandler = fn(Option<&PathBuf>, &clap::ArgMatches) -> Payload;

const LIBRARY_HANDLERS: &[(&str, LibraryHandler)] = &[
    ("workspace", workspace_cmd::query),
    ("creative", creative_cmd::query),
    ("assets", assets_cmd::query),
    ("series", series_cmd::query),
    ("new", new_cmd::query),
    ("doctor", doctor_cmd::query),
    ("setup", setup_cmd::query),
    ("guides", guides_cmd::query),
    ("guide", guides_cmd::query),
    ("channels", channels_cmd::query),
    ("channel", channels_cmd::query),
    ("export-presets", presets_cmd::query),
    ("presets", presets_cmd::query),
    ("preset", presets_cmd::query),
    ("views", views_cmd::query),
    ("view", views_cmd::query),
    ("storage", storage::query),
    ("video", video::query),
    ("library", library_cmd::query),
    ("studio", studio::query),
    ("queue", queue::query),
];

fn dispatch_library_command(
    library: Option<&PathBuf>,
    name: &str,
    args: &clap::ArgMatches,
) -> Option<Payload> {
    for (command_name, handler) in LIBRARY_HANDLERS {
        if *command_name == name {
            if matches!(name, "workspace" | "doctor") {
                return Some(handler(library, args));
            }
            if library.is_none() && name == "setup" {
                return Some(handler(library, args));
            }
            let context = retold_app::workspace::context::Context::resolve(
                library.map(std::path::PathBuf::as_path),
            );
            return Some(context.map_or_else(Diagnostic::into_payload, |context| {
                let root = domain_root(&context, name);
                handler(Some(root), args)
            }));
        }
    }
    None
}

fn domain_root<'a>(
    context: &'a retold_app::workspace::context::Context,
    name: &str,
) -> &'a PathBuf {
    match name {
        "guides" | "guide" | "channels" | "channel" | "export-presets" | "presets" | "preset" => {
            &context.config
        }
        "setup" | "doctor" | "storage" => &context.root,
        _ => &context.library,
    }
}

fn dispatch_meta_command(matches: &clap::ArgMatches) -> Option<Payload> {
    match matches.subcommand() {
        Some(("diagnostics", _)) => Some(retold_app::protocol::diagnostics()),
        Some(("completions", args)) => Some(completions_cmd::query(args, command())),
        Some(("man", _)) => Some(man_cmd::query(command())),
        Some(("explain", args)) => {
            let code = args.get_one::<String>("code");
            Some(code.map_or_else(input_error, |err_code| respond(Request::Explain(err_code))))
        }
        _ => None,
    }
}

fn dispatch_global(matches: &clap::ArgMatches) -> Payload {
    if let Some(payload) = dispatch_meta_command(matches) {
        return payload;
    }
    match matches.subcommand() {
        Some(("account", args)) => account::query(args),
        Some(("config", args)) => config_query(args, selected_root(matches).map(PathBuf::as_path)),
        Some(("dev", args)) => dev_query(args),
        Some(("providers", args)) => {
            providers::query(args, selected_root(matches).map(PathBuf::as_path))
        }
        Some(("home", _)) => retold_app::home::respond(&retold_app::home::HomeOptions {
            library: selected_root(matches).cloned(),
        }),
        Some(("schema", _)) => respond(Request::Schema),
        Some(("spend", args)) => spend::query(args),
        None | Some(("usage", _)) => respond(Request::Usage),
        Some(_) => respond(Request::Unavailable),
    }
}

fn input_error() -> Payload {
    Payload::Error {
        outcome: Outcome::Failed,
        diagnostic: Diagnostic::new(
            ErrorCode::InputRequired,
            "Invalid or incomplete command inputs. See --help.",
        ),
    }
}

/// Runs the command interface, including internal studio worker entry points.
#[must_use]
pub fn run() -> ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    #[cfg(unix)]
    let _supervisor_signals = match supervise(&args) {
        Ok(signals) => signals,
        Err(exit) => return exit,
    };
    if let Some(sources) = interactive(&args) {
        return tui::run(&sources);
    }
    let format = output_format(&args);
    if let Some(exit) = streaming(&args, format) {
        return exit;
    }
    emit_body(request(args), format)
}

/// Internal supervisor and worker entry points; they exist only where the supervisor does.
#[cfg(unix)]
fn supervise(args: &[std::ffi::OsString]) -> Result<Option<interrupt::Interrupt>, ExitCode> {
    let signals = if retold_app::supervision::is_entry(args) {
        match interrupt::Interrupt::session() {
            Ok(signals) => Some(signals),
            Err(_) => return Err(ExitCode::FAILURE),
        }
    } else {
        Option::default()
    };
    match retold_app::supervision::bootstrap(args) {
        Ok(Some(code)) => return Err(ExitCode::from(code)),
        Err(error) => {
            return Err(emit_body(
                Diagnostic::new(ErrorCode::ExecutionFailed, &error.to_string()).into_payload(),
                output_format(args),
            ));
        }
        Ok(None) => (),
    }
    Ok(signals)
}

fn emit_body(body: Payload, format: output::Format) -> ExitCode {
    let exit = body.exit_code();
    let envelope = Envelope::new(request_id(), body);
    let mut stdout = io::stdout().lock();
    let result = output::render(&envelope, format)
        .map_err(io::Error::from)
        .and_then(|line| stdout.write_all(line.as_bytes()))
        .and_then(|()| stdout.flush());
    ExitCode::from(if result.is_ok() {
        exit
    } else {
        ErrorCode::ExecutionFailed.exit_code()
    })
}

fn streaming(args: &[OsString], format: output::Format) -> Option<ExitCode> {
    let matches = command().try_get_matches_from(args).ok()?;
    let ("queue", queue_args) = matches.subcommand()? else {
        return None;
    };
    let ("attach", attach_args) = queue_args.subcommand()? else {
        return None;
    };
    if attach_args.get_flag("once") {
        return None;
    }
    let context = retold_app::workspace::context::Context::resolve(
        selected_root(&matches).map(PathBuf::as_path),
    );
    Some(context.map_or_else(
        |error| emit_body(error.into_payload(), format),
        |context| queue::follow(&context.library, attach_args, format),
    ))
}

/// The full-screen app opens only for a bare invocation on an interactive terminal; scripts,
/// pipes and `--json`/`--plain` keep line output.
fn interactive(args: &[OsString]) -> Option<tui::Sources> {
    use std::io::IsTerminal;
    let matches = command().try_get_matches_from(args).ok()?;
    let line_output = ["json", "plain", "quiet"]
        .into_iter()
        .any(|flag| matches.get_flag(flag));
    let terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
    let bare = matches.subcommand().is_none();
    let theme = if matches.get_flag("no-color") || std::env::var_os("NO_COLOR").is_some() {
        tui::Theme::NoColor
    } else {
        tui::Theme::Default
    };
    (bare && terminal && !line_output).then(|| tui::Sources {
        library: selected_root(&matches).cloned(),
        theme,
    })
}

fn request_id() -> String {
    format!(
        "req-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn output_format(args: &[OsString]) -> output::Format {
    let json_requested = args.iter().take_while(|arg| *arg != "--").any(|arg| {
        arg.to_str()
            .is_some_and(|value| value == "--json" || value.starts_with("--json="))
    });
    if json_requested {
        output::Format::Json
    } else {
        output::Format::Plain
    }
}

fn dev_query(args: &clap::ArgMatches) -> Payload {
    match args.subcommand() {
        Some(("schema", sub_args)) => {
            let kind = sub_args.get_one::<String>("kind").map(String::as_str);
            kind.map_or_else(
                retold_app::dev::schema::default_events_schema,
                retold_app::dev::schema::dev_schema,
            )
        }
        _ => input_error(),
    }
}

fn config_query(args: &clap::ArgMatches, workspace: Option<&std::path::Path>) -> Payload {
    match args.subcommand() {
        Some(("validate", sub_args)) => config_root_for(sub_args, workspace)
            .map_or_else(Diagnostic::into_payload, |root| {
                retold_app::configuration::validate(&root)
            }),
        Some(("migrate", sub_args)) => {
            let dry_run = sub_args.get_flag("dry-run");
            config_root_for(sub_args, workspace).map_or_else(Diagnostic::into_payload, |root| {
                if dry_run {
                    retold_app::configuration::dry_run_migrate(&root)
                } else {
                    retold_app::configuration::migrate(&root)
                }
            })
        }
        Some(("profile", sub_args)) => profile_query(sub_args, workspace),
        _ => input_error(),
    }
}

fn config_root_for(
    sub: &clap::ArgMatches,
    workspace: Option<&std::path::Path>,
) -> Result<PathBuf, Diagnostic> {
    sub.get_one::<PathBuf>("root").cloned().map_or_else(
        || {
            retold_app::workspace::context::Context::resolve(workspace)
                .map(|context| context.config)
        },
        Ok,
    )
}

fn profile_name(sub: &clap::ArgMatches) -> &str {
    sub.get_one::<String>("name").map_or("", String::as_str)
}

fn profile_query(args: &clap::ArgMatches, workspace: Option<&std::path::Path>) -> Payload {
    let Some((_, sub)) = args.subcommand() else {
        return input_error();
    };
    config_root_for(sub, workspace)
        .map_or_else(Diagnostic::into_payload, |root| profile_action(args, &root))
}

fn profile_action(args: &clap::ArgMatches, root: &std::path::Path) -> Payload {
    match args.subcommand() {
        Some(("list", _)) => {
            retold_app::profile::list_profiles(Some(root)).unwrap_or_else(Diagnostic::into_payload)
        }
        Some(("show", sub)) => retold_app::profile::show_profile(Some(root), profile_name(sub))
            .unwrap_or_else(Diagnostic::into_payload),
        Some(("new", sub)) => {
            let file = sub
                .get_one::<std::path::PathBuf>("file")
                .map(std::path::PathBuf::as_path);
            retold_app::profile::create_profile(Some(root), profile_name(sub), file)
                .unwrap_or_else(Diagnostic::into_payload)
        }
        Some(("use", sub)) => retold_app::profile::use_profile(Some(root), profile_name(sub))
            .unwrap_or_else(Diagnostic::into_payload),
        _ => input_error(),
    }
}
