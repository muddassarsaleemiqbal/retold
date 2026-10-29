//! Account administration never accepts workspace or library accounting overrides.
use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::{
    account::commands::{self, Request},
    protocol::{Diagnostic, ErrorCode, Payload},
};

pub fn command() -> Command {
    let limits = Command::new("set-limits")
        .about("Replace finite limits without erasing consumption")
        .arg(
            Arg::new("file")
                .required(true)
                .value_parser(clap::value_parser!(std::path::PathBuf)),
        );
    let command = Command::new("account")
        .about("Manage shared installation accounting")
        .subcommands([
            initialize_command(),
            attempts_command(),
            processes_command(),
            reconcile_command(),
            limit_command(),
            backup_command(),
            Command::new("verify-backup")
                .about("Check an account backup's checksum and contents offline")
                .arg(path_arg("path").required(true)),
            Command::new("show").about("Read account balances without modifying storage"),
        ])
        .subcommand(limits);
    command.subcommand_required(true)
}

pub fn query(arguments: &ArgMatches) -> Payload {
    match arguments.subcommand() {
        Some(("init", arguments)) => {
            let timezone = arguments
                .get_one::<String>("timezone")
                .map_or("UTC", String::as_str);
            commands::respond(Request::Initialize {
                timezone,
                dry_run: arguments.get_flag("dry-run"),
            })
        }
        Some(("set-limits", arguments)) => {
            arguments.get_one::<std::path::PathBuf>("file").map_or_else(
                || Diagnostic::new(ErrorCode::InputRequired, "Supply a limit file.").into_payload(),
                |path| commands::respond(Request::SetLimits(path)),
            )
        }
        Some(("attempts", arguments)) => attempts(arguments),
        Some(("processes", arguments)) => processes(arguments),
        Some(("reconcile", arguments)) => reconcile(arguments),
        Some(("limit", arguments)) => limit(arguments),
        Some(("backup", arguments)) => backup(arguments),
        Some(("verify-backup", arguments)) => verify_backup(arguments),
        Some(("show", _)) => commands::respond(Request::Show),
        _ => Diagnostic::new(ErrorCode::InputRequired, "Select an account subcommand.")
            .into_payload(),
    }
}

fn reconcile(arguments: &ArgMatches) -> Payload {
    let attempt = arguments.get_one::<String>("attempt").map(String::as_str);
    let cost_str = arguments.get_one::<String>("cost").map(String::as_str);
    let (Some(attempt), Some(cost_str)) = (attempt, cost_str) else {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "Supply both --attempt <id> and --cost <usd> to reconcile.",
        )
        .into_payload();
    };
    let previewing = arguments.get_flag("dry-run");
    match cost_str.parse::<retold_app::account::types::Usd>() {
        Ok(cost) if previewing => commands::respond(Request::PreviewReconcile {
            attempt_id: attempt,
            cost,
        }),
        Ok(cost) => commands::respond(Request::Reconcile {
            attempt_id: attempt,
            cost,
        }),
        Err(_) => Diagnostic::new(
            ErrorCode::InputRequired,
            "Supply a valid non-negative USD amount for --cost, such as 1.50 or $0.02.",
        )
        .into_payload(),
    }
}

fn attempts_command() -> Command {
    let command =
        Command::new("attempts").about("Inspect recorded attempts without reconciling charges");
    let command = command.arg(Arg::new("after").long("after"));
    command
        .arg(
            Arg::new("limit")
                .long("limit")
                .default_value("50")
                .value_parser(clap::value_parser!(usize)),
        )
        .arg(Arg::new("state").long("state").value_parser([
            "reserved",
            "dispatched",
            "settled",
            "not_dispatched",
        ]))
}

fn reconcile_command() -> Command {
    let command =
        Command::new("reconcile").about("Reconcile an account attempt without dispatch authority");
    let command = command.arg(Arg::new("attempt").long("attempt").required(true));
    let dry_run = Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue);
    command.args([Arg::new("cost").long("cost").required(true), dry_run])
}

fn attempts(arguments: &ArgMatches) -> Payload {
    use retold_app::account::{inspection, types::AttemptState};
    let state = arguments
        .get_one::<String>("state")
        .map(|value| match value.as_str() {
            "reserved" => AttemptState::Reserved,
            "dispatched" => AttemptState::Dispatched,
            "settled" => AttemptState::Settled,
            _ => AttemptState::NotDispatched,
        });
    let selection = inspection::Selection {
        after: arguments.get_one::<String>("after").map(String::as_str),
        limit: arguments
            .get_one::<usize>("limit")
            .copied()
            .unwrap_or(inspection::DEFAULT_PAGE),
        state,
    };
    inspection::respond(&selection)
}

fn limit_command() -> Command {
    let command = Command::new("limit")
        .about("Set, raise, lower or remove one limit without touching recorded charges")
        .after_help(
            "Examples:\n  retold account limit account day --usd 5 --requests 200\n  \
             retold account limit provider:openrouter:OPENROUTER_API_KEY lifetime --usd 20 \
             --requests 1000\n  retold account limit video:ID lifetime --remove --dry-run\n\n\
             Amounts merge over an existing limit. A new limit needs both --usd and --requests. \
             Lowering a limit below what is already used blocks new paid work in that scope.",
        );
    command.args(limit_arguments())
}

fn limit_arguments() -> Vec<Arg> {
    vec![
        Arg::new("scope")
            .required(true)
            .help("account, workspace:ID, video:ID, series:ID or provider:ID"),
        Arg::new("window")
            .required(true)
            .value_parser(["day", "month", "lifetime"]),
        Arg::new("usd").long("usd").help("Money ceiling in dollars"),
        Arg::new("requests")
            .long("requests")
            .value_parser(clap::value_parser!(u64)),
        Arg::new("resource")
            .long("resource")
            .action(ArgAction::Append)
            .help("Optional extra limits, such as local_millis=600000 (ten minutes of local work)"),
        switch("remove").help("Remove this limit"),
        switch("dry-run").help("Show the change without writing it"),
    ]
}

fn switch(name: &'static str) -> Arg {
    Arg::new(name).long(name).action(ArgAction::SetTrue)
}

fn limit(arguments: &ArgMatches) -> Payload {
    use retold_app::account::{limits, types::Window};
    let window = match arguments.get_one::<String>("window").map(String::as_str) {
        Some("day") => Window::Day,
        Some("month") => Window::Month,
        _ => Window::Lifetime,
    };
    let resources: Vec<String> = arguments
        .get_many::<String>("resource")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    let text = |name: &str| arguments.get_one::<String>(name).map(String::as_str);
    limits::respond(&limits::Input {
        scope: text("scope").unwrap_or_default(),
        window,
        usd: text("usd"),
        requests: arguments.get_one::<u64>("requests").copied(),
        resources: &resources,
        remove: arguments.get_flag("remove"),
        dry_run: arguments.get_flag("dry-run"),
    })
}

fn backup_command() -> Command {
    Command::new("backup")
        .about("Snapshot the account into a new folder while no job or change is running")
        .args([
            path_arg("output").long("output").required(true),
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue)
                .help("Show what would be captured and who is busy; write nothing"),
        ])
        .after_help(
            "The backup holds accounting only: jobs, attempts, charges, holds and limits. \
             Secrets, login profiles and workspace files are never included. If jobs or changes \
             are running, the backup is refused at once and lists them; nothing waits.",
        )
}

fn path_arg(name: &'static str) -> Arg {
    Arg::new(name).value_parser(clap::value_parser!(std::path::PathBuf))
}

fn backup(arguments: &ArgMatches) -> Payload {
    let dry_run = arguments.get_flag("dry-run");
    arguments
        .get_one::<std::path::PathBuf>("output")
        .map_or_else(missing_path, |output| {
            commands::respond(Request::Backup { output, dry_run })
        })
}

fn missing_path() -> Payload {
    Diagnostic::new(ErrorCode::InputRequired, "Supply a backup path.").into_payload()
}

fn initialize_command() -> Command {
    let initialize =
        Command::new("init").about("Explicitly create installation accounting with zero limits");
    let initialize = initialize.arg(Arg::new("timezone").long("timezone").default_value("UTC"));
    initialize.arg(
        Arg::new("dry-run")
            .long("dry-run")
            .action(ArgAction::SetTrue),
    )
}

fn verify_backup(arguments: &ArgMatches) -> Payload {
    arguments
        .get_one::<std::path::PathBuf>("path")
        .map_or_else(missing_path, |path| {
            commands::respond(Request::VerifyBackup(path))
        })
}

fn processes_command() -> Command {
    Command::new("processes")
        .about("Inspect supervised workers and retained physical uncertainty without changing them")
        .arg(Arg::new("after").long("after"))
        .arg(
            Arg::new("limit")
                .long("limit")
                .default_value("50")
                .value_parser(clap::value_parser!(usize)),
        )
}

fn processes(arguments: &ArgMatches) -> Payload {
    use retold_app::account::processes::{self, Selection};
    processes::respond(Selection {
        after: arguments.get_one::<String>("after").map(String::as_str),
        limit: arguments
            .get_one::<usize>("limit")
            .copied()
            .unwrap_or(processes::DEFAULT_LIMIT),
    })
}
