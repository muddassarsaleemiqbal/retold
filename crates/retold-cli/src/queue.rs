//! Queue command handler: list, show, pause, unpause, stop, move, and attach.

use std::path::{Path, PathBuf};

use clap::{Arg, ArgMatches};
mod follow;
pub use follow::run as follow;
use retold_app::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use retold_app::queue::{JobPage, JobRecord, ListOptions, MAX_PAGE_SIZE, QueueStore};
use retold_app::studio::{
    ProgressEvent, StudioRequest, StudioResponse, query_status, resolve_workspace_state,
    send_request,
};

pub fn command() -> clap::Command {
    let base = clap::Command::new("queue")
        .about("Inspect and control queued video generation jobs")
        .subcommand_required(true);
    base.subcommands(queue_subcommands())
}

fn queue_subcommands() -> Vec<clap::Command> {
    let mut cmds = read_subcommands();
    cmds.extend(control_subcommands());
    cmds
}

fn read_subcommands() -> Vec<clap::Command> {
    vec![
        list_command(),
        clap::Command::new("show")
            .about("Inspect a specific job's status and plan")
            .arg(Arg::new("id").required(true).help("Job ID or prefix")),
        clap::Command::new("attach")
            .about("Attach to a running job and stream progress")
            .args([
                Arg::new("id").required(true).help("Job ID or prefix"),
                Arg::new("once")
                    .long("once")
                    .action(clap::ArgAction::SetTrue)
                    .help("Read one bounded page without following"),
                Arg::new("after-sequence")
                    .long("after-sequence")
                    .value_parser(clap::value_parser!(u64))
                    .help("Replay events after sequence number"),
            ]),
    ]
}

fn list_command() -> clap::Command {
    clap::Command::new("list")
        .about("Inspect a bounded page of pending jobs and history")
        .args([
            Arg::new("limit")
                .long("limit")
                .value_parser(clap::value_parser!(u32).range(1..=i64::from(MAX_PAGE_SIZE))),
            Arg::new("cursor")
                .long("cursor")
                .help("Continue an unchanged queue snapshot"),
            Arg::new("status").long("status").value_parser([
                "queued",
                "running",
                "pausing",
                "paused",
                "stopping",
                "stopped",
                "completed",
                "failed",
                "blocked",
            ]),
        ])
}

fn list_options(args: &ArgMatches) -> ListOptions {
    ListOptions {
        limit: args
            .get_one::<u32>("limit")
            .copied()
            .unwrap_or(retold_app::queue::DEFAULT_PAGE_SIZE),
        cursor: args.get_one::<String>("cursor").cloned(),
        status: args
            .get_one::<String>("status")
            .and_then(|status| retold_app::queue::JobStatus::parse(status)),
    }
}

fn control_subcommands() -> Vec<clap::Command> {
    vec![
        clap::Command::new("pause")
            .about("Pause a running job")
            .arg(Arg::new("id").required(true).help("Job ID")),
        clap::Command::new("unpause")
            .about("Resume a paused job")
            .arg(Arg::new("id").required(true).help("Job ID")),
        clap::Command::new("stop")
            .about("Stop a running or queued job safely")
            .arg(Arg::new("id").required(true).help("Job ID")),
        clap::Command::new("move")
            .about("Move a job before another job in the queue")
            .args([
                Arg::new("id").required(true).help("Job ID to move"),
                Arg::new("before")
                    .long("before")
                    .required(true)
                    .help("Target job ID to place before"),
            ]),
    ]
}

#[must_use]
pub fn query(library: Option<&PathBuf>, matches: &ArgMatches) -> Payload {
    let state_dir = resolve_workspace_state(library.map(PathBuf::as_path));
    let Some((sub, args)) = matches.subcommand() else {
        return input_required();
    };
    if let Some(payload) = query_read(&state_dir, sub, args) {
        return payload;
    }
    if let Some(payload) = query_control(&state_dir, sub, args) {
        return payload;
    }
    input_required()
}

fn query_read(state_dir: &Path, name: &str, args: &ArgMatches) -> Option<Payload> {
    match name {
        "list" => Some(list_payload(state_dir, &list_options(args))),
        "show" => match args.get_one::<String>("id") {
            Some(id) => Some(show_payload(state_dir, id)),
            None => Some(missing_id_error()),
        },
        _ => None,
    }
}

fn query_control(state_dir: &Path, name: &str, args: &ArgMatches) -> Option<Payload> {
    let Some(id) = args.get_one::<String>("id") else {
        return Some(missing_id_error());
    };
    match name {
        "move" => {
            let Some(before) = args.get_one::<String>("before") else {
                return Some(missing_before_error());
            };
            Some(move_payload(state_dir, id, before))
        }
        "attach" => {
            let after_seq = args.get_one::<u64>("after-sequence").copied();
            Some(attach_payload(state_dir, id, after_seq))
        }
        "pause" | "unpause" | "stop" => Some(control_payload(state_dir, name, id)),
        _ => None,
    }
}

fn missing_id_error() -> Payload {
    Payload::Error {
        outcome: Outcome::Failed,
        diagnostic: Diagnostic::new(ErrorCode::InputRequired, "Job ID is required. See --help."),
    }
}

fn missing_before_error() -> Payload {
    Payload::Error {
        outcome: Outcome::Failed,
        diagnostic: Diagnostic::new(
            ErrorCode::InputRequired,
            "--before <job-id> is required for queue move.",
        ),
    }
}

fn input_required() -> Payload {
    Payload::Error {
        outcome: Outcome::Failed,
        diagnostic: Diagnostic::new(
            ErrorCode::InputRequired,
            "A queue subcommand is required. See --help.",
        ),
    }
}

fn list_payload(state_dir: &Path, options: &ListOptions) -> Payload {
    let db_path = state_dir.join("queue.sqlite");
    if !db_path.exists() {
        return empty_queue_payload(options);
    }
    QueueStore::open_read_only(&db_path)
        .and_then(|store| store.list_page(options))
        .map_or_else(
            |error| error.diagnostic().into_payload(),
            |page| page_payload(&page),
        )
}

fn empty_queue_payload(options: &ListOptions) -> Payload {
    let has_cursor = options.cursor.is_some();
    if has_cursor {
        return retold_app::queue::QueueError::CursorInvalid
            .diagnostic()
            .into_payload();
    }
    page_payload(&JobPage {
        jobs: Vec::new(),
        total: 0,
        next_cursor: Option::default(),
    })
}

fn page_payload(page: &JobPage) -> Payload {
    let empty = page.jobs.is_empty();
    let text = if empty {
        "No jobs match this queue view.".into()
    } else {
        render_job_table(&page.jobs)
    };
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text, "jobs": page.jobs, "total": page.total,
            "next_cursor": page.next_cursor,
        }),
    }
}

fn render_job_table(jobs: &[JobRecord]) -> String {
    let mut lines = vec![format!(
        "  {:<16} {:<10} {:<24} CREATED",
        "ID", "STATUS", "VIDEO"
    )];
    for job in jobs {
        lines.push(format!(
            "  {:<16} {:<10} {:<24} {}",
            job.id,
            job.status.as_str(),
            job.video_ref,
            job.created_at
        ));
    }
    lines.join("\n")
}

fn show_payload(state_dir: &Path, id: &str) -> Payload {
    let db_path = state_dir.join("queue.sqlite");
    if !db_path.exists() {
        return not_found_payload(id);
    }
    let store = match QueueStore::open_read_only(&db_path) {
        Ok(opened_store) => opened_store,
        Err(error) => return error.diagnostic().into_payload(),
    };
    find_job_in_store(&store, id)
}

fn find_job_in_store(store: &QueueStore, id: &str) -> Payload {
    match store.details(id) {
        Ok(Some(details)) => {
            let text = format_job_details(&details.job);
            Payload::Result {
                outcome: Outcome::Completed,
                contents: serde_json::json!({
                    "text": text, "job": details.job, "result": details.result,
                    "execution": details.execution,
                }),
            }
        }
        Ok(None) => not_found_payload(id),
        Err(error) => error.diagnostic().into_payload(),
    }
}

fn not_found_payload(id: &str) -> Payload {
    let msg = format!("Job '{id}' does not exist in the queue.");
    Payload::Error {
        outcome: Outcome::Failed,
        diagnostic: Diagnostic::new(ErrorCode::RefNotFound, &msg),
    }
}

fn format_job_details(job: &JobRecord) -> String {
    let mut lines = vec![
        format!("Job ID:      {}", job.id),
        format!("Status:      {}", job.status.as_str()),
        format!("Video Ref:   {}", job.video_ref),
        format!("Plan Hash:   {}", job.plan_hash),
        format!("Priority:    {}", job.priority),
        format!("Created:     {}", job.created_at),
        format!("Updated:     {}", job.updated_at),
    ];
    if let Some(code) = &job.error_code {
        lines.push(format!("Error Code:  {code}"));
    }
    if let Some(msg) = &job.error_message {
        lines.push(format!("Error:       {msg}"));
    }
    lines.join("\n")
}

fn control_payload(state_dir: &Path, action: &str, id: &str) -> Payload {
    if let Some(problem) = studio_problem(state_dir, action, id) {
        return problem;
    }
    let req = control_request(action, id);
    dispatch_control(state_dir, &req)
}

fn move_payload(state_dir: &Path, id: &str, before: &str) -> Payload {
    if let Some(problem) = studio_problem(state_dir, "move", id) {
        return problem;
    }
    let request = StudioRequest::MoveJob {
        id: id.into(),
        before: before.into(),
    };
    dispatch_control(state_dir, &request)
}

fn studio_problem(state_dir: &Path, action: &str, id: &str) -> Option<Payload> {
    let status = query_status(state_dir);
    let issue = status.issues.into_iter().next();
    if let Some(issue) = issue {
        return Some(issue.into_payload());
    }
    (!status.running).then(|| studio_required_payload(action, id))
}

fn attach_payload(state_dir: &Path, id: &str, after_seq: Option<u64>) -> Payload {
    retold_app::studio::Attachment::open(state_dir, id, after_seq.unwrap_or(0))
        .and_then(|attachment| attachment.poll())
        .map_or_else(
            |error| error.diagnostic().into_payload(),
            |page| attach_snapshot(&page),
        )
}

fn attach_snapshot(page: &retold_app::queue::ReplayPage) -> Payload {
    let text = match (page.events.is_empty(), page.is_terminal) {
        (true, true) => "Job is terminal (no further events).".into(),
        (true, false) => "No new events received.".into(),
        (false, _) => format_event_log(&page.events),
    };
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({ "text": text, "events": page.events,
            "is_terminal": page.is_terminal, "job": page.job,
            "result": page.result, "after_sequence": page.next_sequence }),
    }
}

fn format_event_log(events: &[ProgressEvent]) -> String {
    let mut lines = Vec::new();
    for event in events {
        lines.push(format!(
            "[seq {}] {}: {}",
            event.sequence, event.stage, event.message
        ));
    }
    lines.join("\n")
}

fn control_request(action: &str, id: &str) -> StudioRequest {
    match action {
        "pause" => StudioRequest::PauseJob { id: id.to_owned() },
        "unpause" => StudioRequest::UnpauseJob { id: id.to_owned() },
        _ => StudioRequest::StopJob { id: id.to_owned() },
    }
}

fn studio_required_payload(action: &str, id: &str) -> Payload {
    Payload::Error {
        outcome: Outcome::Blocked,
        diagnostic: Diagnostic {
            code: ErrorCode::StudioRequired,
            message: "Queue commands require a running studio. Start it with retold studio start."
                .into(),
            context: serde_json::json!({"action": action, "job_id": id}),
            suggested_actions: vec![vec!["retold".into(), "studio".into(), "start".into()]],
            redacted: false,
        },
    }
}

fn dispatch_control(state_dir: &Path, request: &StudioRequest) -> Payload {
    match send_request(state_dir, request) {
        Ok(StudioResponse::JobTransitioned { job }) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "text": format!("Job {} is now {}. ", job.id, job.status.as_str()),
                "job": job,
            }),
        },
        Ok(StudioResponse::Error { code, message }) => {
            Diagnostic::new(code, &message).into_payload()
        }
        Ok(_) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"text": "Action completed.", "status": "ok"}),
        },
        Err(err) => {
            let msg = err.to_string();
            Payload::Error {
                outcome: Outcome::Failed,
                diagnostic: Diagnostic::new(ErrorCode::ExecutionFailed, &msg),
            }
        }
    }
}
