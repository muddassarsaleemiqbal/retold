//! The command surface delegates discovery and planning to the application layer.

use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use retold_app::continuation::ContinueOptions;
use retold_app::library::{self, ListOptions, Query};
use retold_app::packaging::{CleanOptions, PackageOptions, PathsOptions, VerifyOptions};
use retold_app::plan::{ExportOptions, PlanOptions};
use retold_app::protocol::{Diagnostic, ErrorCode, Payload};
use retold_app::review::{
    CastEditSpec, LineEditSpec, NoteSpec, RedoMode, RedoSpec, ReviewFilter, ShotEditSpec,
};
use retold_app::video::{CopySpec, CostsOptions, DeleteMode};
use retold_app::{Request, respond};

pub fn command() -> Command {
    let list = Command::new("list").about("List saved videos without calling providers");
    let list = list.args([
        value("channel"),
        value("search"),
        value("tag"),
        value("sort").value_parser(["created", "updated", "title", "channel"]),
        value("view"),
        value("cursor"),
        value("limit").value_parser(clap::value_parser!(usize)),
        switch("archive"),
    ]);
    let show = Command::new("show")
        .about("Inspect saved evidence without executing or repairing work")
        .arg(reference_arg());
    let mut command = Command::new("video")
        .about("Inspect, plan and continue saved videos")
        .subcommand_required(true);
    command = command.subcommands(production_subcommands(list, show));
    command = command.subcommands(review_subcommands());
    command = command.subcommand(crate::script_cmd::command());
    command = command.subcommands(management_subcommands());
    command.subcommands(delivery_subcommands())
}

fn production_subcommands(list: Command, show: Command) -> Vec<Command> {
    vec![
        crate::new_cmd::command(),
        list,
        show,
        plan_command(),
        continue_command(),
        export_command(),
        costs_command(),
    ]
}

fn review_subcommands() -> Vec<Command> {
    vec![
        review_command(),
        approve_command(),
        unlock_command(),
        note_command(),
        edit_command(),
        undo_command(),
        redo_command(),
        history_command(),
    ]
}

fn delivery_subcommands() -> Vec<Command> {
    vec![
        paths_command(),
        verify_command(),
        clean_command(),
        package_command(),
    ]
}

fn paths_command() -> Command {
    Command::new("paths")
        .about("Inspect filesystem paths for video work, delivery, and state directories")
        .args([
            reference_arg(),
            value("kind").value_parser(["all", "work", "delivery", "state"]),
        ])
}

fn verify_command() -> Command {
    Command::new("verify")
        .about("Verify integrity of video work artifacts and delivery checksums")
        .args([
            reference_arg(),
            value("scope").value_parser(["work", "delivery", "all"]),
        ])
}

fn clean_command() -> Command {
    Command::new("clean")
        .about("Remove temporary or rebuildable files; preserves deliverables and accounting")
        .args([
            reference_arg(),
            value("scope").value_parser(["temporary", "rebuildable"]),
            switch("dry-run"),
            switch("yes"),
        ])
}

fn package_command() -> Command {
    Command::new("package")
        .about("Bundle an exported release into a verified delivery package")
        .args([
            reference_arg(),
            value("edition").required(true),
            value("aspect").required(true),
            value("release").required(true),
            value("output").required(true),
            switch("dry-run"),
        ])
}

fn management_subcommands() -> Vec<Command> {
    vec![
        copy_command(),
        archive_command(),
        unarchive_command(),
        delete_command(),
        tag_command(),
    ]
}

fn tag_command() -> Command {
    Command::new("tag")
        .about("Add or remove organizational tags on a video")
        .args([
            reference_arg(),
            value("add").help("Tag to add to the video"),
            value("remove").help("Tag to remove from the video"),
        ])
}

fn history_command() -> Command {
    Command::new("history")
        .about("Inspect local audit timeline, edits, reviews and export events")
        .args([
            reference_arg(),
            value("kind").value_parser(["plan", "edit", "review", "export", "all"]),
        ])
}

fn copy_command() -> Command {
    Command::new("copy")
        .about("Duplicate a video project with a new title or variant")
        .args([reference_arg(), value("title"), value("lang")])
}

fn archive_command() -> Command {
    Command::new("archive")
        .about("Archive a video from active lists")
        .arg(reference_arg())
}

fn unarchive_command() -> Command {
    Command::new("unarchive")
        .about("Restore an archived video to active lists")
        .arg(reference_arg())
}

fn delete_command() -> Command {
    let force = switch("force")
        .visible_alias("permanent")
        .help("Delete permanently without moving to trash");
    Command::new("delete")
        .about("Delete or move a video to trash")
        .args([reference_arg(), force])
}

fn review_command() -> Command {
    Command::new("review")
        .about("Inspect items waiting for review, locked approvals, and takes")
        .arg(reference_arg())
        .args([
            switch("pending"),
            switch("stale"),
            switch("failed"),
            value("lang"),
            value("take"),
        ])
}

fn approve_command() -> Command {
    let mut cmd = Command::new("approve");
    cmd = cmd.alias("lock");
    cmd = cmd.about("Approve and lock reviewed items against automatic regeneration");
    cmd.args([
        reference_arg(),
        Arg::new("target").help("Optional stage or item key to approve"),
    ])
}

fn unlock_command() -> Command {
    let mut cmd = Command::new("unlock");
    cmd = cmd.about("Remove approval lock on items or stages");
    cmd.args([
        reference_arg(),
        Arg::new("target").help("Optional stage or item key to unlock"),
    ])
}

fn note_command() -> Command {
    Command::new("note")
        .about("Attach a feedback note to an item")
        .args([
            reference_arg(),
            Arg::new("item").required(true).help("Item identifier"),
            Arg::new("text").required(true).help("Feedback note text"),
        ])
}

fn edit_command() -> Command {
    Command::new("edit")
        .about("Edit video components (shot, line, cast, budget)")
        .subcommand_required(true)
        .subcommands([
            edit_shot_command(),
            edit_line_command(),
            edit_cast_command(),
            edit_budget_command(),
        ])
}

fn edit_shot_command() -> Command {
    Command::new("shot")
        .about("Edit storyboard shot frame parameters")
        .args([
            reference_arg(),
            Arg::new("frame")
                .required(true)
                .value_parser(clap::value_parser!(usize))
                .help("1-based frame number"),
            value("action"),
            value("staging"),
            value("characters"),
            value("type"),
        ])
}

fn edit_line_command() -> Command {
    Command::new("line")
        .about("Edit dialogue or narration line")
        .args([
            reference_arg(),
            Arg::new("line")
                .required(true)
                .value_parser(clap::value_parser!(usize))
                .help("1-based line number"),
            value("lang"),
            value("text"),
            switch("spoken-only"),
        ])
}

fn edit_cast_command() -> Command {
    Command::new("cast")
        .about("Edit character look description")
        .args([
            reference_arg(),
            Arg::new("name").required(true).help("Character name"),
            value("look"),
        ])
}

fn edit_budget_command() -> Command {
    Command::new("budget")
        .about("Edit video budget ceiling")
        .args([
            reference_arg(),
            Arg::new("amount").required(true).help("New USD ceiling"),
        ])
}

fn undo_command() -> Command {
    Command::new("undo")
        .about("Restore prior content version as a new revision")
        .arg(reference_arg())
}

fn redo_command() -> Command {
    Command::new("redo")
        .about("Request new versions of items and rebuild true descendants")
        .args([
            reference_arg(),
            Arg::new("stage").required(true).help("Stage to redo"),
            Arg::new("items").help("1-based item numbers or ranges"),
            switch("dry-run"),
        ])
}

fn plan_command() -> Command {
    let plan = Command::new("plan")
        .about("Show what continue would admit: stages, ceilings, routes and the plan hash");
    plan.arg(reference_arg()).args(plan_args())
}

fn continue_command() -> Command {
    let resume = Command::new("continue")
        .about("Continue a saved video up to ready within fresh, explicit authority");
    let resume = resume.arg(reference_arg()).args(plan_args());
    resume.args(confirmations()).args(background_args())
}

fn export_command() -> Command {
    let export = Command::new("export")
        .about("Open, save or render named editions and formats saved at ready");
    let scope = [
        repeated("edition"),
        repeated("format"),
        value("preset"),
        value("aspect"),
        switch("project-only"),
        switch("dry-run"),
    ];
    let export = export.arg(reference_arg()).args(scope);
    let confirmed = export.args(confirmations()).args(spend_args());
    confirmed.args(background_args())
}

fn background_args() -> Vec<Arg> {
    vec![
        switch("background"),
        value("idempotency-key").requires("background"),
    ]
}

fn costs_command() -> Command {
    let flags = [
        reference_arg(),
        value("request").requires("cost"),
        value("cost").requires("request"),
        switch("reconcile"),
    ];
    Command::new("costs")
        .about("See what this video has cost, and settle charges that are still pending")
        .args(flags)
}

fn repeated(name: &'static str) -> Arg {
    value(name).action(ArgAction::Append)
}

fn switch(name: &'static str) -> Arg {
    Arg::new(name).long(name).action(ArgAction::SetTrue)
}

fn confirmations() -> [Arg; 2] {
    [value("plan-hash"), switch("yes")]
}

fn reference_arg() -> Arg {
    Arg::new("reference").required(true)
}

fn plan_args() -> Vec<Arg> {
    let mut args = vec![value("until")];
    args.extend(spend_args());
    args
}

fn spend_args() -> Vec<Arg> {
    vec![
        value("spend"),
        value("max-requests").value_parser(clap::value_parser!(u64)),
    ]
}

fn value(name: &'static str) -> Arg {
    Arg::new(name).long(name)
}

#[must_use]
pub fn query(root: Option<&PathBuf>, args: &ArgMatches) -> Payload {
    if let Some(script) = args.subcommand_matches("script") {
        return with_root(root, |root| crate::script_cmd::query(root, script));
    }
    if let Some(new) = args.subcommand_matches("new") {
        return crate::new_cmd::query(root, new);
    }
    if let Some(payload) = dispatch_continuation(root, args) {
        return payload;
    }
    let query_res = match query_from_args(args) {
        Ok(query_spec) => query_spec,
        Err(diagnostic) => return diagnostic.into_payload(),
    };
    with_root(root, |root| library::query(root, query_res))
}

fn dispatch_continuation(root: Option<&PathBuf>, args: &ArgMatches) -> Option<Payload> {
    match args.subcommand() {
        Some(("continue", args)) => Some(with_root(root, |root| {
            run_or_submit(root, args, continue_options(args))
        })),
        Some(("export", args)) if !args.get_flag("dry-run") => {
            let options = ContinueOptions {
                plan: export_plan(args),
                ..continue_options(args)
            };
            Some(with_root(root, |root| run_or_submit(root, args, options)))
        }
        _ => None,
    }
}

fn run_or_submit(root: &std::path::Path, args: &ArgMatches, options: ContinueOptions) -> Payload {
    if args.get_flag("background") {
        let submission = retold_app::studio::SubmitOptions {
            execution: options,
            idempotency_key: args.get_one::<String>("idempotency-key").cloned(),
        };
        retold_app::studio::submit(root, reference(args), &submission)
    } else {
        crate::foreground::run(root, reference(args), &options)
    }
}

fn query_from_args(args: &ArgMatches) -> Result<Query<'_>, Diagnostic> {
    match args.subcommand() {
        Some(("list", args)) => Ok(Query::List(options(args))),
        Some(("show", args)) => Ok(Query::Show(reference(args))),
        Some(("plan", args)) => Ok(Query::Plan(reference(args), plan_options(args))),
        Some(("export", args)) => Ok(Query::Plan(reference(args), export_plan(args))),
        Some(("costs", args)) => costs_query(args),
        _ => parse_review_query(args),
    }
}

fn parse_review_query(args: &ArgMatches) -> Result<Query<'_>, Diagnostic> {
    match args.subcommand() {
        Some(("review", sub)) => Ok(Query::Review(reference(sub), review_filter(sub))),
        Some(("approve", sub)) => Ok(Query::Approve(
            reference(sub),
            sub.get_one::<String>("target").map(String::as_str),
        )),
        Some(("unlock", sub)) => Ok(Query::Unlock(
            reference(sub),
            sub.get_one::<String>("target").map(String::as_str),
        )),
        Some(("note", sub)) => Ok(Query::Note(reference(sub), note_spec(sub))),
        Some(("edit", sub)) => edit_query(sub),
        Some(("undo", sub)) => Ok(Query::Undo(reference(sub))),
        Some(("redo", sub)) => Ok(parse_redo_query(sub)),
        Some(("history", sub)) => Ok(Query::History(
            reference(sub),
            sub.get_one::<String>("kind").map(String::as_str),
        )),
        _ => parse_management_query(args),
    }
}

fn parse_tag_query(sub: &ArgMatches) -> Result<Query<'_>, Diagnostic> {
    let add_tag = sub.get_one::<String>("add");
    let remove_tag = sub.get_one::<String>("remove");
    match (add_tag, remove_tag) {
        (Some(tag), None) => Ok(Query::Tag(
            reference(sub),
            retold_app::library::TagOperation::Add(tag),
        )),
        (None, Some(tag)) => Ok(Query::Tag(
            reference(sub),
            retold_app::library::TagOperation::Remove(tag),
        )),
        (Some(_), Some(_)) => Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Cannot specify both --add and --remove at the same time.",
        )),
        (None, None) => Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Specify either --add TAG or --remove TAG.",
        )),
    }
}

fn parse_lifecycle_query(args: &ArgMatches) -> Result<Query<'_>, Diagnostic> {
    match args.subcommand() {
        Some(("archive", sub)) => Ok(Query::Archive(reference(sub))),
        Some(("unarchive", sub)) => Ok(Query::Unarchive(reference(sub))),
        Some(("delete", sub)) => Ok(Query::Delete(reference(sub), delete_mode(sub))),
        _ => parse_delivery_query(args),
    }
}

fn parse_delivery_query(args: &ArgMatches) -> Result<Query<'_>, Diagnostic> {
    match args.subcommand() {
        Some(("paths", sub)) => Ok(Query::Paths(reference(sub), paths_options(sub))),
        Some(("verify", sub)) => Ok(Query::Verify(reference(sub), verify_options(sub))),
        Some(("clean", sub)) => Ok(Query::Clean(reference(sub), clean_options(sub))),
        Some(("package", sub)) => Ok(parse_package_query(sub)),
        _ => Err(extract_unavailable_diagnostic()),
    }
}

fn paths_options(sub: &ArgMatches) -> PathsOptions<'_> {
    PathsOptions {
        kind: sub.get_one::<String>("kind").map(String::as_str),
    }
}

fn verify_options(sub: &ArgMatches) -> VerifyOptions<'_> {
    VerifyOptions {
        scope: sub.get_one::<String>("scope").map(String::as_str),
    }
}

fn clean_options(sub: &ArgMatches) -> CleanOptions<'_> {
    CleanOptions {
        scope: sub.get_one::<String>("scope").map(String::as_str),
        dry_run: sub.get_flag("dry-run"),
        yes: sub.get_flag("yes"),
    }
}

fn parse_package_query(sub: &ArgMatches) -> Query<'_> {
    let edition = sub.get_one::<String>("edition").map_or("", String::as_str);
    let aspect = sub.get_one::<String>("aspect").map_or("", String::as_str);
    let release = sub.get_one::<String>("release").map_or("", String::as_str);
    let output_str = sub.get_one::<String>("output").map_or("", String::as_str);
    let output = Path::new(output_str);
    let dry_run = sub.get_flag("dry-run");

    Query::Package(
        reference(sub),
        PackageOptions {
            edition,
            aspect,
            release,
            output,
            dry_run,
        },
    )
}

fn parse_management_query(args: &ArgMatches) -> Result<Query<'_>, Diagnostic> {
    match args.subcommand() {
        Some(("copy", sub)) => Ok(Query::Copy(reference(sub), copy_spec(sub))),
        Some(("tag", sub)) => parse_tag_query(sub),
        _ => parse_lifecycle_query(args),
    }
}

fn copy_spec(sub: &ArgMatches) -> CopySpec<'_> {
    CopySpec {
        title: sub.get_one::<String>("title").map(String::as_str),
        lang: sub.get_one::<String>("lang").map(String::as_str),
    }
}

fn delete_mode(sub: &ArgMatches) -> DeleteMode {
    if sub.get_flag("force") {
        DeleteMode::Permanent
    } else {
        DeleteMode::Trash
    }
}

fn note_spec(sub: &ArgMatches) -> NoteSpec<'_> {
    NoteSpec {
        item: sub.get_one::<String>("item").map_or("", String::as_str),
        text: sub.get_one::<String>("text").map_or("", String::as_str),
    }
}

fn extract_unavailable_diagnostic() -> Diagnostic {
    match respond(Request::Unavailable) {
        Payload::Error { diagnostic, .. } => diagnostic,
        _ => Diagnostic::new(ErrorCode::NotAvailable, "Action unavailable"),
    }
}

fn parse_redo_query(sub: &ArgMatches) -> Query<'_> {
    let is_dry_run = sub.get_flag("dry-run");
    let mode = if is_dry_run {
        RedoMode::DryRun
    } else {
        RedoMode::Commit
    };
    let spec = RedoSpec {
        stage: sub.get_one::<String>("stage").map_or("", String::as_str),
        items: sub.get_one::<String>("items").map(String::as_str),
        mode,
    };
    Query::Redo(reference(sub), spec)
}

fn review_filter(args: &ArgMatches) -> ReviewFilter {
    ReviewFilter {
        pending: args.get_flag("pending"),
        stale: args.get_flag("stale"),
        failed: args.get_flag("failed"),
        lang: args.get_one::<String>("lang").cloned(),
        take: args.get_one::<String>("take").cloned(),
    }
}

fn edit_query(args: &ArgMatches) -> Result<Query<'_>, Diagnostic> {
    match args.subcommand() {
        Some(("shot", sub)) => Ok(Query::EditShot(reference(sub), shot_edit_spec(sub))),
        Some(("line", sub)) => Ok(Query::EditLine(reference(sub), line_edit_spec(sub))),
        Some(("cast", sub)) => Ok(Query::EditCast(reference(sub), cast_edit_spec(sub))),
        Some(("budget", sub)) => Ok(Query::EditBudget(
            reference(sub),
            sub.get_one::<String>("amount").map_or("", String::as_str),
        )),
        _ => Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "An edit subcommand is required (shot, line, cast, budget).",
        )),
    }
}

fn shot_edit_spec(sub: &ArgMatches) -> ShotEditSpec<'_> {
    let frame = sub.get_one::<usize>("frame").copied().unwrap_or(1);
    ShotEditSpec {
        frame,
        action: sub.get_one::<String>("action").map(String::as_str),
        staging: sub.get_one::<String>("staging").map(String::as_str),
        characters: sub.get_one::<String>("characters").map(String::as_str),
        shot_type: sub.get_one::<String>("type").map(String::as_str),
    }
}

fn line_edit_spec(sub: &ArgMatches) -> LineEditSpec<'_> {
    let line = sub.get_one::<usize>("line").copied().unwrap_or(1);
    LineEditSpec {
        line,
        lang: sub.get_one::<String>("lang").map(String::as_str),
        text: sub.get_one::<String>("text").map(String::as_str),
        spoken_only: sub.get_flag("spoken-only"),
    }
}

fn cast_edit_spec(sub: &ArgMatches) -> CastEditSpec<'_> {
    CastEditSpec {
        character: sub.get_one::<String>("name").map_or("", String::as_str),
        look: sub.get_one::<String>("look").map_or("", String::as_str),
    }
}

fn parse_cost(cost_text: &str) -> Result<retold_app::video::Usd, Diagnostic> {
    cost_text.parse::<retold_app::video::Usd>().map_err(|_| {
        Diagnostic::new(
            ErrorCode::InputRequired,
            "Supply a valid non-negative USD amount for --cost, such as 1.50 or $0.02.",
        )
    })
}

fn costs_query(args: &ArgMatches) -> Result<Query<'_>, Diagnostic> {
    let cost = match args.get_one::<String>("cost") {
        Some(cost_text) => Some(parse_cost(cost_text)?),
        None => None,
    };
    let options = CostsOptions {
        request: args.get_one::<String>("request").map(String::as_str),
        cost,
    };
    Ok(Query::Costs(reference(args), options))
}

fn with_root(root: Option<&PathBuf>, action: impl FnOnce(&std::path::Path) -> Payload) -> Payload {
    let root = root.cloned().map_or_else(library::default_library, Ok);
    root.map_or_else(Diagnostic::into_payload, |root| action(&root))
}

fn reference(args: &ArgMatches) -> &str {
    args.get_one::<String>("reference")
        .map_or("", String::as_str)
}

fn plan_options(args: &ArgMatches) -> PlanOptions {
    PlanOptions {
        until: args.try_get_one::<String>("until").ok().flatten().cloned(),
        spend: args.get_one::<String>("spend").cloned(),
        max_requests: args.get_one::<u64>("max-requests").copied(),
        export: None,
    }
}

fn export_plan(args: &ArgMatches) -> PlanOptions {
    let all = |name| -> Vec<String> {
        args.get_many::<String>(name)
            .map(|values| values.cloned().collect())
            .unwrap_or_default()
    };
    let export = ExportOptions {
        editions: all("edition"),
        formats: all("format"),
        project_only: args.get_flag("project-only"),
        preset: args.get_one::<String>("preset").cloned(),
        aspect: args.get_one::<String>("aspect").cloned(),
    };
    PlanOptions {
        export: Some(export),
        ..plan_options(args)
    }
}

fn continue_options(args: &ArgMatches) -> ContinueOptions {
    ContinueOptions {
        plan: plan_options(args),
        plan_hash: args.get_one::<String>("plan-hash").cloned(),
        yes: args.get_flag("yes"),
    }
}

fn options(args: &ArgMatches) -> ListOptions {
    ListOptions {
        channel: args.get_one::<String>("channel").cloned(),
        search: args.get_one::<String>("search").cloned(),
        tag: args.get_one::<String>("tag").cloned(),
        sort: args.get_one::<String>("sort").cloned(),
        view: args.get_one::<String>("view").cloned(),
        cursor: args.get_one::<String>("cursor").cloned(),
        limit: args.get_one::<usize>("limit").copied(),
        archive: args.get_flag("archive"),
    }
}
