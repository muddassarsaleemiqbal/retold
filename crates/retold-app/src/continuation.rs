//! `video continue` and `video export`: recompile the plan, confirm it, admit fresh job
//! authority and run the engine through the account gate. Continue stops at `ready`; only an
//! export plan names deliveries the editor may open or render. Saved run flags grant nothing.

use std::path::Path;
use std::sync::Arc;

pub use retold_engine::session::Control;
use retold_engine::session::{self, StopKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::account::{Account, canonical_directory, commands};
use crate::execution::AccountGate;
use crate::plan::{ExecutablePlan, PlanOptions, Review};
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

const MESSAGE_LIMIT: usize = 500;
const JOB_HASH_PREFIX: usize = 12;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinueOptions {
    pub plan: PlanOptions,
    /// The reviewed plan; continuing under any other plan is refused.
    pub plan_hash: Option<String>,
    /// Accepts the plan computed now instead of a reviewed hash. It never raises allowance.
    pub yes: bool,
}

#[must_use]
pub fn respond(root: &Path, reference: &str, options: &ContinueOptions) -> Payload {
    ContinueRequest {
        root,
        reference,
        options,
        control: &Control::default(),
    }
    .respond()
}

#[derive(Debug)]
pub struct ContinueRequest<'a> {
    pub root: &'a Path,
    pub reference: &'a str,
    pub options: &'a ContinueOptions,
    pub control: &'a Control,
}

impl ContinueRequest<'_> {
    #[must_use]
    pub fn respond(&self) -> Payload {
        continue_video(self).unwrap_or_else(Diagnostic::into_payload)
    }
}

fn continue_video(request: &ContinueRequest<'_>) -> Result<Payload, Diagnostic> {
    let catalog = crate::discovery::scan(request.root)?;
    let (plan, _) = crate::plan::compile(&catalog, request.reference, &request.options.plan)?;
    confirm(&plan, request.options)?;
    let job_id = job_id(&plan.plan_hash);
    run_plan(&plan, &job_id, Some(request.control))
}

pub(crate) fn run_plan(
    plan: &ExecutablePlan,
    job_id: &str,
    control: Option<&session::Control>,
) -> Result<Payload, Diagnostic> {
    let exporting = plan.authority.export.is_some();
    let action = if exporting { "export" } else { "continue" };
    let account = hold_account(&format!("video {action} {job_id}"))?;
    admit(&account, plan, job_id)?;
    let outcome = plan.run_with_account(account, job_id, control);
    Ok(report(plan, job_id, outcome))
}

pub(crate) fn confirm(plan: &ExecutablePlan, options: &ContinueOptions) -> Result<(), Diagnostic> {
    let review = Review {
        plan_hash: &plan.plan_hash,
        blockers: &plan.blockers,
        reviewed: options.plan_hash.as_deref(),
        yes: options.yes,
    };
    review.confirm(
        "Review video plan first, then pass --plan-hash HASH, or --yes to accept the plan computed now.",
    )
}

/// Binds the library and routes, then admits a job pinned to this exact plan.
fn admit(account: &Account, plan: &ExecutablePlan, job_id: &str) -> Result<(), Diagnostic> {
    let job = plan.job(job_id)?;
    account
        .register_workspace(job.workspace.clone())
        .map_err(|error| commands::diagnostic(&error))?;
    for (route, provider_account) in &plan.authority.routes {
        account
            .register_route(route, provider_account)
            .map_err(|error| commands::diagnostic(&error))?;
    }
    account
        .admit(job)
        .map_err(|error| commands::diagnostic(&error))
}

impl ExecutablePlan {
    fn run_with_account(
        &self,
        account: Account,
        job_id: &str,
        control: Option<&session::Control>,
    ) -> Result<(), session::SessionError> {
        let job = self
            .job(job_id)
            .map_err(|_| session::SessionError::Changed)?;
        let runtime = &self.runtime;
        let request = session::Request {
            paths: &runtime.paths,
            layout: &runtime.layout,
            providers: &runtime.providers,
            gate: Arc::new(AccountGate::for_job(account, &job)),
            allow_paid: self.authority.usd_micros > 0,
            until: runtime.until,
            expected: &runtime.expected,
            editor: None,
            control,
        };
        match &runtime.selection {
            Some(selection) => session::export(&request, selection),
            None => session::run_until(&request),
        }
    }
}

fn usage(job_id: &str) -> Value {
    open_account()
        .ok()
        .and_then(|account| account.job_usage(job_id).ok())
        .map_or(Value::Null, |usage| json!(usage))
}

fn report(
    plan: &ExecutablePlan,
    job_id: &str,
    outcome: Result<(), session::SessionError>,
) -> Payload {
    let usage = usage(job_id);
    let export = &plan.authority.export;
    let context = json!({
        "job_id": job_id,
        "plan_hash": plan.plan_hash,
        "until": plan.authority.until,
        "usage": usage,
        "export_authorized": export.is_some(),
        "export": export,
    });
    let kind = if export.is_some() {
        "exported"
    } else {
        "continued"
    };
    match outcome {
        Ok(()) => Payload::Result {
            outcome: Outcome::Completed,
            contents: json!({ kind: context }),
        },
        Err(error) => {
            let mut diagnostic = Diagnostic::new(code(error.kind()), &message(&error));
            diagnostic.context = context;
            diagnostic.context["cancelled"] = json!(error.kind() == StopKind::Cancelled);
            diagnostic.into_payload()
        }
    }
}

/// Blocked stops keep their own codes; cancellations and failures are execution failures.
const STOP_CODES: &[(StopKind, ErrorCode)] = &[
    (StopKind::Review, ErrorCode::ReviewRequired),
    (StopKind::Authority, ErrorCode::AccountRequired),
    (StopKind::Changed, ErrorCode::PlanChanged),
    (StopKind::NotReady, ErrorCode::NotReady),
];

fn code(kind: StopKind) -> ErrorCode {
    STOP_CODES
        .iter()
        .find(|(stop, _)| *stop == kind)
        .map_or(ErrorCode::ExecutionFailed, |(_, code)| *code)
}

/// Engine messages can quote provider output; keep them short and on one line.
fn message(error: &session::SessionError) -> String {
    one_line(&error.to_string())
}

fn one_line(text: &str) -> String {
    let words: Vec<_> = text.split_whitespace().collect();
    truncated(&words.join(" "))
}

fn truncated(text: &str) -> String {
    text.chars().take(MESSAGE_LIMIT).collect()
}

pub(crate) fn open_account() -> Result<Account, Diagnostic> {
    canonical_directory()
        .and_then(|directory| Account::open(&directory))
        .map_err(|error| commands::diagnostic(&error))
}

/// The account, holding the maintenance barrier for a whole job.
pub(crate) fn hold_account(purpose: &str) -> Result<Account, Diagnostic> {
    open_account()?
        .hold(purpose)
        .map_err(|error| commands::diagnostic(&error))
}

pub(crate) fn job_id(plan_hash: &str) -> String {
    let started = jiff::Timestamp::now();
    format!(
        "job-{}-{}",
        started.strftime("%Y%m%dT%H%M%S%.f"),
        &plan_hash[..JOB_HASH_PREFIX]
    )
}
