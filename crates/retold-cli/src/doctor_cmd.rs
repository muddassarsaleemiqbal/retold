//! CLI commands for doctor workspace health inspection, repair, and diagnostic bundle generation.

use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::doctor::{self, DoctorMode};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};
use retold_app::workspace::context::Context;

fn bundle_subcommand() -> Command {
    let mut output_arg = Arg::new("output");
    output_arg = output_arg.long("output");
    output_arg = output_arg.value_name("PATH");
    output_arg = output_arg.required(true);
    output_arg = output_arg.value_parser(clap::value_parser!(PathBuf));
    output_arg = output_arg.help("Destination directory for the redacted support bundle");

    let mut dry_run_arg = Arg::new("dry-run");
    dry_run_arg = dry_run_arg.long("dry-run");
    dry_run_arg = dry_run_arg.action(ArgAction::SetTrue);
    dry_run_arg = dry_run_arg.help("Preview bundle contents and redactions without writing files");

    Command::new("bundle")
        .about("Generate an offline, redacted diagnostic support bundle")
        .arg(output_arg)
        .arg(dry_run_arg)
}

pub fn command() -> Command {
    let mut fix_arg = Arg::new("fix");
    fix_arg = fix_arg.long("fix");
    fix_arg = fix_arg.action(ArgAction::SetTrue);
    fix_arg = fix_arg.help("Apply automatic repairs to fixable workspace issues");

    Command::new("doctor")
        .about("Inspect workspace health, detect issues, and optionally repair")
        .arg(fix_arg)
        .subcommand(bundle_subcommand())
}

#[must_use]
pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    if let Some(bundle) = args.subcommand_matches("bundle") {
        let path = root
            .cloned()
            .map_or_else(retold_app::library::default_library, Ok);
        return path.map_or_else(Diagnostic::into_payload, |path| bundle_query(&path, bundle));
    }
    let context = Context::resolve(root.map(PathBuf::as_path));
    context.map_or_else(Diagnostic::into_payload, |context| {
        dispatch_doctor(&context, args)
    })
}

pub fn dispatch_doctor(context: &Context, args: &ArgMatches) -> Payload {
    if let Some(sub_matches) = args.subcommand_matches("bundle") {
        bundle_query(&context.root, sub_matches)
    } else {
        inspect_or_repair(context, args)
    }
}

fn bundle_query(root: &Path, args: &ArgMatches) -> Payload {
    let Some(output) = args.get_one::<PathBuf>("output") else {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "The --output path argument is required for support bundle generation.",
        )
        .into_payload();
    };
    let dry_run = args.get_flag("dry-run");
    let options = doctor::bundle::BundleOptions { output, dry_run };
    doctor::bundle::create_support_bundle(root, &options).unwrap_or_else(Diagnostic::into_payload)
}

fn determine_mode(args: &ArgMatches) -> DoctorMode {
    if args.get_flag("fix") {
        DoctorMode::Repair
    } else {
        DoctorMode::Inspect
    }
}

fn inspect_or_repair(context: &Context, args: &ArgMatches) -> Payload {
    let mode = determine_mode(args);
    doctor::run_doctor(context, mode).unwrap_or_else(Diagnostic::into_payload)
}
