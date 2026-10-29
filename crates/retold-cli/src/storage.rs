//! Portable backup inspection never selects or changes the canonical account.
use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

pub fn command() -> Command {
    let archive = Arg::new("archive")
        .required(true)
        .value_parser(clap::value_parser!(std::path::PathBuf));
    let verify = Command::new("verify")
        .about("Verify a portable directory backup offline")
        .arg(archive);
    Command::new("storage")
        .subcommand_required(true)
        .subcommands([
            verify,
            backup_command(),
            restore_command(),
            activate_command(),
            usage_command(),
        ])
}

pub fn query(library: Option<&std::path::PathBuf>, arguments: &ArgMatches) -> Payload {
    if let Some(usage) = arguments.subcommand_matches("usage") {
        return usage_query(library, usage);
    }
    if let Some(activate) = arguments.subcommand_matches("activate") {
        return activate_query(activate);
    }
    if let Some(restore) = arguments.subcommand_matches("restore") {
        return restore_query(restore);
    }
    if let Some(backup) = arguments.subcommand_matches("backup") {
        return backup_query(library, backup);
    }
    let path = arguments
        .subcommand_matches("verify")
        .and_then(|arguments| arguments.get_one::<std::path::PathBuf>("archive"));
    path.map_or_else(
        || {
            Diagnostic::new(
                ErrorCode::InputRequired,
                "Supply a portable backup directory.",
            )
            .into_payload()
        },
        |path| retold_app::storage::respond(path),
    )
}

fn backup_command() -> Command {
    let output_arg = Arg::new("output")
        .long("output")
        .required(true)
        .value_parser(clap::value_parser!(std::path::PathBuf));
    let dry_run_arg = Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue);
    let video_arg = Arg::new("video")
        .long("video")
        .value_parser(clap::value_parser!(String));
    let all_arg = Arg::new("all")
        .long("all")
        .action(ArgAction::SetTrue)
        .conflicts_with("video");
    Command::new("backup")
        .about("Back up managed files in the selected registered workspace and account history")
        .args([output_arg, dry_run_arg, video_arg, all_arg])
}

fn backup_query(library: Option<&std::path::PathBuf>, arguments: &ArgMatches) -> Payload {
    use retold_app::storage::creation::{self, Mode, Target};
    let root = library
        .cloned()
        .map_or_else(retold_app::library::default_library, Ok);
    let Some(output) = arguments.get_one::<std::path::PathBuf>("output") else {
        return Diagnostic::new(ErrorCode::InputRequired, "Supply --output PATH.").into_payload();
    };
    let mode = if arguments.get_flag("dry-run") {
        Mode::Preview
    } else {
        Mode::Create
    };
    let target = if arguments.get_flag("all") {
        Target::AllWorkspaces
    } else if let Some(video_id) = arguments.get_one::<String>("video") {
        Target::SelectedVideo(video_id)
    } else {
        Target::Workspace
    };
    root.map_or_else(Diagnostic::into_payload, |root| {
        creation::respond_request(creation::Request {
            root: &root,
            output,
            mode,
            target,
        })
    })
}

fn restore_command() -> Command {
    Command::new("restore")
        .about("Restore a verified backup as an inactive workspace into a new directory")
        .args([
            Arg::new("archive")
                .required(true)
                .value_parser(clap::value_parser!(std::path::PathBuf)),
            Arg::new("into")
                .long("into")
                .required(true)
                .value_parser(clap::value_parser!(std::path::PathBuf)),
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue),
        ])
}

fn restore_query(arguments: &ArgMatches) -> Payload {
    use retold_app::storage::restoration::{self, Mode};
    let archive = arguments.get_one::<std::path::PathBuf>("archive");
    let output = arguments.get_one::<std::path::PathBuf>("into");
    let (Some(archive), Some(output)) = (archive, output) else {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "Supply a backup directory and --into PATH.",
        )
        .into_payload();
    };
    let mode = if arguments.get_flag("dry-run") {
        Mode::Preview
    } else {
        Mode::Restore
    };
    restoration::respond(archive, output, mode)
}

fn activate_command() -> Command {
    let root_arg = Arg::new("root")
        .required(true)
        .value_parser(clap::value_parser!(std::path::PathBuf));
    let dry_run_arg = Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue)
        .conflicts_with("plan-hash");
    let plan_hash_arg = Arg::new("plan-hash")
        .long("plan-hash")
        .required_unless_present("dry-run");
    let reconcile_arg = Arg::new("reconcile")
        .long("reconcile")
        .action(ArgAction::SetTrue);
    let attest_arg = Arg::new("attest-prior-stopped")
        .long("attest-prior-stopped")
        .action(ArgAction::SetTrue);
    Command::new("activate")
        .about(
            "Review or apply a workspace ownership transfer or cross-installation reconciliation",
        )
        .args([
            root_arg,
            dry_run_arg,
            plan_hash_arg,
            reconcile_arg,
            attest_arg,
        ])
}

fn activate_query(arguments: &ArgMatches) -> Payload {
    use retold_app::storage::activation::{self, Mode, ReconcileConfirmation};
    let Some(root) = arguments.get_one::<std::path::PathBuf>("root") else {
        return Diagnostic::new(ErrorCode::InputRequired, "Supply the restored root.")
            .into_payload();
    };
    let is_reconcile = arguments.get_flag("reconcile");
    let plan_hash = arguments.get_one::<String>("plan-hash").map(String::as_str);
    let attest = arguments.get_flag("attest-prior-stopped");

    let mode = match (is_reconcile, plan_hash) {
        (true, None) => Mode::ReconcilePreview,
        (true, Some(hash)) => Mode::ReconcileActivate(ReconcileConfirmation {
            plan_hash: hash,
            attest_prior_stopped: attest,
        }),
        (false, None) => Mode::Preview,
        (false, Some(hash)) => Mode::Activate(hash),
    };
    activation::respond(root, mode)
}

fn usage_command() -> Command {
    Command::new("usage")
        .about("Inspect workspace storage consumption, breakdown by video or kind, and reclaimable space")
        .arg(
            Arg::new("by")
                .long("by")
                .value_parser(["video", "kind"])
                .help("Group storage by video or kind"),
        )
}

fn usage_query(library: Option<&std::path::PathBuf>, arguments: &ArgMatches) -> Payload {
    let resolved_root = library
        .cloned()
        .map_or_else(retold_app::library::default_library, Ok);
    resolved_root.map_or_else(Diagnostic::into_payload, |root_path| {
        let grouping = match arguments.get_one::<String>("by").map(String::as_str) {
            Some("video") => retold_app::storage::usage::UsageGrouping::ByVideo,
            Some("kind") => retold_app::storage::usage::UsageGrouping::ByKind,
            _ => retold_app::storage::usage::UsageGrouping::Summary,
        };
        retold_app::storage::usage::inspect_storage_usage(&root_path, grouping)
            .unwrap_or_else(Diagnostic::into_payload)
    })
}
