//! Provider setup never implicitly tests logins or sends requests; only `providers test`, with
//! confirmed authority, does.
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

pub fn command() -> Command {
    Command::new("providers")
        .about("Discover, configure and explicitly test providers")
        .subcommand_required(true)
        .subcommands([
            Command::new("scan").about("Offline tool, profile and environment inventory"),
            Command::new("list")
                .about("List configured providers; health and logins remain unchecked")
                .arg(path_arg("root")),
            Command::new("health")
                .about("Inspect recorded cooldowns without testing providers")
                .arg(path_arg("root")),
            add_command(),
            definition_command(
                "edit",
                "Replace an existing typed definition; preserve routes",
            ),
            route_command(),
            remove_command(),
            test_command(),
            login_command(),
        ])
}

fn add_command() -> Command {
    definition_command(
        "add",
        "Import one typed provider definition; preserve existing routes",
    )
}

fn definition_command(name: &'static str, about: &'static str) -> Command {
    let id = Arg::new("id")
        .required(true)
        .help("Name for this provider, such as claude or codex-personal");
    let file = definition_file();
    let dry_run = Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue)
        .help("Check the definition without changing anything");
    let command = Command::new(name)
        .about(about)
        .args([id, path_arg("root"), file]);
    command
        .args(inline_args())
        .arg(dry_run)
        .after_help(DEFINITION_HELP)
}

fn definition_file() -> Arg {
    let file = Arg::new("file")
        .long("file")
        .value_parser(clap::value_parser!(PathBuf));
    file.required_unless_present("kind")
        .conflicts_with("kind")
        .help("A TOML definition file; or describe the provider with --kind and the options below")
}

const DEFINITION_HELP: &str = "Describe the provider with --kind and its options, or pass --file with the same fields as TOML.\nExamples:\n  retold providers add claude --kind claude_cli --tier fast=haiku --tier smart=sonnet --tier best=opus\n  retold providers add codex --kind codex_cli --max-parallel 2\n  retold providers add openrouter --kind openrouter --key-env OPENROUTER_API_KEY";

const KINDS: [&str; 7] = [
    "claude_cli",
    "codex_cli",
    "gemini_cli",
    "openrouter",
    "local_text",
    "local_voice",
    "local_whisper",
];

/// `(flag, TOML field, help)` for options that become one quoted value.
const TEXT_OPTIONS: [(&str, &str, &str); 7] = [
    (
        "binary",
        "binary",
        "Program to run; found on PATH when omitted (CLI and local kinds)",
    ),
    (
        "profile-dir",
        "config_dir",
        "A separate login folder, such as ~/.claude-work (claude_cli, codex_cli)",
    ),
    (
        "key-env",
        "key_env",
        "Environment variable holding the API key (openrouter)",
    ),
    (
        "base-url",
        "base_url",
        "Alternative API address (openrouter)",
    ),
    ("espeak", "espeak", "espeak-ng program (local_voice)"),
    ("ffmpeg", "ffmpeg", "ffmpeg program (local_whisper)"),
    ("model-file", "model", "Whisper model file (local_whisper)"),
];

fn inline_args() -> Vec<Arg> {
    let kind = Arg::new("kind")
        .long("kind")
        .value_parser(KINDS)
        .help("What kind of provider this is");
    let parallel = Arg::new("max-parallel").long("max-parallel");
    let parallel = parallel
        .value_parser(clap::value_parser!(u32).range(1..))
        .default_value("2")
        .help("How many requests may run at once");
    let timeout = Arg::new("timeout")
        .long("timeout")
        .value_parser(clap::value_parser!(u64).range(1..))
        .help("Seconds a request may take; 600 when omitted");
    let text = TEXT_OPTIONS
        .iter()
        .map(|(flag, _, help)| Arg::new(*flag).long(*flag).help(*help));
    [kind, parallel, timeout]
        .into_iter()
        .chain(model_arguments())
        .chain(text)
        .collect()
}

fn model_arguments() -> [Arg; 2] {
    let tier = Arg::new("tier").long("tier").value_name("NAME=MODEL");
    let tier = tier
        .action(ArgAction::Append)
        .help("A model for a quality tier, such as smart=sonnet; repeat for each tier");
    let model = Arg::new("model").long("model").value_name("NAME=PATH");
    let model = model
        .action(ArgAction::Append)
        .help("A local GGUF model file, such as small=~/models/q4.gguf (local_text)");
    [tier, model]
}

/// The TOML a definition file would hold, built from `--kind` and its options.
fn inline_definition(args: &ArgMatches) -> String {
    let kind = args.get_one::<String>("kind").map(|kind| quoted(kind));
    let parallel = args.get_one::<u32>("max-parallel").map(u32::to_string);
    let timeout = args.get_one::<u64>("timeout").map(u64::to_string);
    let scalars = [
        ("kind", kind),
        ("max_parallel", parallel),
        ("timeout_secs", timeout),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|value| format!("{key} = {value}")));
    let texts = TEXT_OPTIONS.iter().filter_map(|(flag, key, _)| {
        args.get_one::<String>(flag)
            .map(|value| format!("{key} = {}", quoted(value)))
    });
    let tables = [("tiers", "tier"), ("models", "model")]
        .into_iter()
        .filter_map(|(table, flag)| pairs_table(args, table, flag));
    scalars
        .chain(texts)
        .chain(tables)
        .collect::<Vec<_>>()
        .join("\n")
}

/// `[tiers]` from repeated `--tier smart=sonnet`; malformed pairs are left for validation.
fn pairs_table(args: &ArgMatches, table: &str, flag: &str) -> Option<String> {
    let rows: Vec<String> = args
        .get_many::<String>(flag)?
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair.as_str(), ""));
            format!("{} = {}", quoted(name.trim()), quoted(value.trim()))
        })
        .collect();
    Some(format!("[{table}]\n{}", rows.join("\n")))
}

/// A TOML basic string; JSON's escapes are valid TOML escapes.
fn quoted(text: &str) -> String {
    serde_json::Value::from(text).to_string()
}

fn path_arg(name: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .required(name != "root")
        .value_parser(clap::value_parser!(PathBuf))
}

pub fn query(args: &ArgMatches, workspace: Option<&Path>) -> Payload {
    if args.subcommand_name() == Some("scan") {
        return retold_app::providers::scan();
    }
    let Some((_, sub)) = args.subcommand() else {
        return invalid_inputs();
    };
    let root = crate::config_root_for(sub, workspace);
    root.map_or_else(Diagnostic::into_payload, |root| dispatch(&root, args))
}

fn dispatch(root: &Path, args: &ArgMatches) -> Payload {
    match args.subcommand() {
        Some(("list", _)) => retold_app::providers::list(root),
        Some(("health", _)) => retold_app::providers::health(root),
        Some(("add", args)) => import_definition(root, args, retold_app::providers::add),
        Some(("edit", args)) => import_definition(root, args, retold_app::providers::edit),
        Some(("route", args)) => route(root, args),
        Some(("remove", args)) => remove(root, args),
        Some(("test", args)) => test(root, args),
        Some(("login", args)) => login(root, args),
        _ => invalid_inputs(),
    }
}

fn import_definition(
    root: &Path,
    args: &ArgMatches,
    action: fn(&retold_app::providers::AddRequest<'_>) -> Payload,
) -> Payload {
    use retold_app::providers::AddRequest;
    let Some(id) = args.get_one::<String>("id") else {
        return invalid_inputs();
    };
    let inline = inline_definition(args);
    let mut request = args.get_one::<PathBuf>("file").map_or_else(
        || AddRequest::inline(root, id, &inline),
        |file| AddRequest::new(root, id, file),
    );
    request.dry_run = args.get_flag("dry-run");
    action(&request)
}

fn invalid_inputs() -> Payload {
    Diagnostic::new(
        ErrorCode::InputRequired,
        "Supply the provider ID, and --kind or --file.",
    )
    .into_payload()
}

fn route_command() -> Command {
    let command = Command::new("route")
        .about("Show routes, or replace one ordered provider:selector chain")
        .arg(path_arg("root"))
        .arg(Arg::new("tier").requires("order"));
    let suggest = Arg::new("suggest")
        .long("suggest")
        .action(ArgAction::SetTrue)
        .conflicts_with_all(["tier", "order"]);
    let suggest = suggest
        .help("Fill every unrouted tier from the configured providers; existing routes are kept");
    command
        .arg(Arg::new("order").long("order").requires("tier"))
        .arg(suggest)
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue),
        )
}

fn route(root: &Path, args: &ArgMatches) -> Payload {
    let suggesting = args.get_flag("suggest");
    if suggesting {
        let dry_run = args.get_flag("dry-run");
        return retold_app::providers::suggest_routes(retold_app::providers::SuggestRequest {
            root,
            dry_run,
        });
    }
    match (
        args.get_one::<String>("tier"),
        args.get_one::<String>("order"),
    ) {
        (Some(tier), Some(order)) => {
            let request = retold_app::providers::RouteRequest {
                root,
                tier,
                order,
                dry_run: args.get_flag("dry-run"),
            };
            retold_app::providers::set_route(&request)
        }
        (None, None) if !args.get_flag("dry-run") => retold_app::providers::routes(root),
        _ => Diagnostic::new(
            ErrorCode::InputRequired,
            "Supply both the tier and --order to change a route, or neither to show routes.",
        )
        .into_payload(),
    }
}

fn remove_command() -> Command {
    let dry_run = Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue);
    let id_arg = Arg::new("id").required(true);
    let root_arg = path_arg("root");
    Command::new("remove")
        .about("Remove an unreferenced provider definition")
        .args([id_arg, root_arg, dry_run])
}

fn remove(root: &Path, args: &ArgMatches) -> Payload {
    match args.get_one::<String>("id") {
        Some(id) => retold_app::providers::remove(&retold_app::providers::RemoveRequest {
            root,
            id,
            dry_run: args.get_flag("dry-run"),
        }),
        _ => invalid_inputs(),
    }
}

fn test_command() -> Command {
    let command = Command::new("test")
        .about("Plan, then explicitly send one tiny text request per provider")
        .arg(Arg::new("id"))
        .arg(path_arg("root"));
    let confirmations = [
        Arg::new("plan-hash").long("plan-hash"),
        Arg::new("yes").long("yes").action(ArgAction::SetTrue),
    ];
    command
        .arg(Arg::new("spend").long("spend"))
        .args(confirmations)
        .after_help("Without --plan-hash or --yes this only shows the plan. Per-use providers are tested only with a positive --spend.")
}

fn test(root: &Path, args: &ArgMatches) -> Payload {
    let text = |name| args.get_one::<String>(name).map(String::as_str);
    retold_app::provider_tests::respond(&retold_app::provider_tests::TestOptions {
        root,
        provider: text("id"),
        spend: text("spend"),
        plan_hash: text("plan-hash"),
        yes: args.get_flag("yes"),
    })
}

fn login_command() -> Command {
    let profile_arg = Arg::new("profile-dir")
        .long("profile-dir")
        .value_parser(clap::value_parser!(PathBuf));
    let dry_run_arg = Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue);
    let status_arg = Arg::new("status").long("status").action(ArgAction::SetTrue);
    let key_env_arg = Arg::new("key-env").long("key-env");
    let id_arg = Arg::new("id").required(true);
    let root_arg = path_arg("root");
    Command::new("login")
        .about("Inspect login status or configure credentials for a provider")
        .args([
            id_arg,
            root_arg,
            status_arg,
            profile_arg,
            key_env_arg,
            dry_run_arg,
        ])
}

fn login(root: &Path, args: &ArgMatches) -> Payload {
    let id = args.get_one::<String>("id");
    let Some(id) = id else {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "Supply the provider ID and --root.",
        )
        .into_payload();
    };
    let only_status = args.get_flag("status")
        || (!args.contains_id("profile-dir") && !args.contains_id("key-env"));
    if only_status {
        return retold_app::providers::login_status(root, id);
    }
    let mut request = retold_app::providers::LoginRequest::new(root, id);
    let profile_dir = args.get_one::<PathBuf>("profile-dir").map(PathBuf::as_path);
    let key_env = args.get_one::<String>("key-env").map(String::as_str);
    request.profile_dir = profile_dir;
    request.key_env = key_env;
    request.dry_run = args.get_flag("dry-run");
    retold_app::providers::login(&request)
}
