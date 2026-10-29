//! `providers test`: a hashed plan with one text probe per provider, confirmed like any other
//! execution, then a fresh account job. Per-use probes need a positive `--spend` and
//! provider-scoped limits. Free probes still run only under admitted authority. Scan, add and
//! edit never test.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use retold_engine::Paths;
use retold_engine::gate::PaidCapability;
use retold_engine::provider_test::{
    self, Allowance, Kit, Outcome as ProbeOutcome, Probe, Run, Selection, Status,
};
use retold_engine::session::Providers;
use serde::Serialize;
use serde_json::{Value, json};

use crate::account::Account;
use crate::account::types::{Action, Capability, Job, Limit, Scope, Window, Workspace};
use crate::continuation::{hold_account, job_id, open_account};
use crate::execution::{AccountGate, capability, operation_node};
use crate::plan::{Blocker, Review, positive_limit, resources, spend};
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

/// What every provider-test job is recorded under, in place of a video.
pub const SUBJECT: &str = "providers:test";
const SETTINGS_ID_HEX: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct TestOptions<'a> {
    /// The provider settings folder.
    pub root: &'a Path,
    /// One provider, or every configured provider when absent.
    pub provider: Option<&'a str>,
    pub spend: Option<&'a str>,
    /// The reviewed plan; testing under any other plan is refused.
    pub plan_hash: Option<&'a str>,
    /// Accepts the plan computed now. It never raises an allowance.
    pub yes: bool,
}

impl TestOptions<'_> {
    const fn confirmed(&self) -> bool {
        self.plan_hash.is_some() || self.yes
    }
}

/// Everything the plan hash pins.
#[derive(Debug, Clone, Serialize)]
pub struct TestAuthority {
    pub settings_id: String,
    pub probes: Vec<Probe>,
    pub usd_micros: u64,
    /// One request per per-use probe.
    pub requests: u64,
    /// Per-use provider → the provider account whose allowance it draws on.
    pub routes: BTreeMap<String, String>,
    pub input_versions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestPlan {
    pub plan_hash: String,
    pub authority: TestAuthority,
    pub blockers: Vec<Blocker>,
    /// The exact command that runs this plan.
    pub next: Vec<String>,
}

impl TestPlan {
    fn sending(&self) -> BTreeSet<String> {
        let probes = self.authority.probes.iter();
        let sending = probes.filter(|probe| probe.sends());
        sending.map(|probe| probe.provider.clone()).collect()
    }

    /// Fresh job authority for exactly this plan.
    fn job(&self, job_id: &str, root: &Path) -> Result<Job, Diagnostic> {
        let authority = &self.authority;
        let ceiling = resources(authority.requests, authority.usd_micros);
        let node = operation_node(SUBJECT, provider_test::STAGE, text())
            .map_err(|_| invalid("Could not derive the operation tree."))?;
        Ok(Job {
            id: job_id.into(),
            workspace: Workspace {
                id: authority.settings_id.clone(),
                root: root.into(),
            },
            video_id: SUBJECT.into(),
            series_id: None,
            plan_hash: self.plan_hash.clone(),
            input_versions: authority.input_versions.clone(),
            action: Action::ProviderTest {
                providers: self.sending(),
            },
            routes: authority.routes.keys().cloned().collect(),
            capabilities: vec![text()],
            operations: BTreeMap::from([(node, ceiling.clone())]),
            resources: ceiling,
            deadline: None,
        })
    }
}

const fn text() -> Capability {
    capability(PaidCapability::Text)
}

/// The provider settings a plan reads.
struct Settings {
    paths: Paths,
    providers: Providers,
    root: PathBuf,
}

impl Settings {
    fn load(root: &Path) -> Result<Self, Diagnostic> {
        let root = root.canonicalize().map_err(|_| {
            Diagnostic::new(
                ErrorCode::InputRequired,
                "The provider settings folder does not exist.",
            )
        })?;
        let paths = Paths {
            config: root.clone(),
            library: root.clone(),
        };
        let providers = Providers::load(&paths).map_err(|_| {
            invalid("Provider settings are invalid. Repair providers.toml before testing.")
        })?;
        Ok(Self {
            paths,
            providers,
            root,
        })
    }
}

#[must_use]
pub fn respond(options: &TestOptions<'_>) -> Payload {
    test(options).unwrap_or_else(Diagnostic::into_payload)
}

fn test(options: &TestOptions<'_>) -> Result<Payload, Diagnostic> {
    let settings = Settings::load(options.root)?;
    let plan = compile(&settings, options)?;
    if !options.confirmed() {
        return Ok(completed(json!({ "test_plan": plan })));
    }
    confirm(&plan, options)?;
    let nothing_sends = plan.sending().is_empty();
    if nothing_sends {
        let skipped = skipped(&plan);
        return Ok(completed(
            json!({ "tested": tested(&plan, None, &skipped) }),
        ));
    }
    run_probes(&settings, &plan)
}

fn compile(settings: &Settings, options: &TestOptions<'_>) -> Result<TestPlan, Diagnostic> {
    let usd_micros = spend(options.spend)?;
    let allowance = if usd_micros > 0 {
        Allowance::PerUse
    } else {
        Allowance::FreeOnly
    };
    let selection = Selection {
        provider: options.provider,
        allowance,
    };
    let probes = provider_test::plan(&settings.providers, selection)
        .map_err(|error| Diagnostic::new(ErrorCode::InputRequired, &error.to_string()))?;
    let authority = authority(settings, probes, usd_micros)?;
    let plan_hash = digest(&authority)?;
    Ok(TestPlan {
        next: next_command(options, &plan_hash),
        blockers: blockers(&authority),
        plan_hash,
        authority,
    })
}

fn authority(
    settings: &Settings,
    probes: Vec<Probe>,
    usd_micros: u64,
) -> Result<TestAuthority, Diagnostic> {
    let accounts = settings.providers.per_use_routes();
    let paid: Vec<&Probe> = probes.iter().filter(|probe| probe.is_paid()).collect();
    let routes = paid
        .iter()
        .filter_map(|probe| {
            let account = accounts.get(&probe.provider)?;
            Some((probe.provider.clone(), account.clone()))
        })
        .collect();
    Ok(TestAuthority {
        settings_id: settings_id(&settings.root)?,
        requests: u64::try_from(paid.len()).unwrap_or(u64::MAX),
        usd_micros,
        routes,
        input_versions: BTreeMap::from([
            ("providers".into(), settings.providers.digest().into()),
            ("settings_root".into(), digest(&settings.root)?),
        ]),
        probes,
    })
}

/// One stable workspace per canonical provider settings folder.
fn settings_id(root: &Path) -> Result<String, Diagnostic> {
    let hex = digest(&root)?;
    Ok(format!("settings-{}", &hex[..SETTINGS_ID_HEX]))
}

fn next_command(options: &TestOptions<'_>, plan_hash: &str) -> Vec<String> {
    let mut words: Vec<String> = ["retold", "providers", "test"].map(String::from).to_vec();
    words.extend(options.provider.map(String::from));
    words.extend(["--root".into(), options.root.display().to_string()]);
    if let Some(amount) = options.spend {
        words.extend(["--spend".into(), amount.into()]);
    }
    words.extend(["--plan-hash".into(), plan_hash.into()]);
    words
}

fn blockers(authority: &TestAuthority) -> Vec<Blocker> {
    let sends = authority.probes.iter().any(Probe::sends);
    if !sends {
        return Vec::new();
    }
    let account = open_account();
    match account {
        Ok(account) => limit_blockers(authority, &account),
        Err(diagnostic) => vec![Blocker {
            code: ErrorCode::AccountRequired,
            message: format!(
                "{} Set an account limit, then plan again.",
                diagnostic.message
            ),
            context: Value::Null,
        }],
    }
}

/// Account day/month and per provider-account lifetime ceilings that would refuse a paid probe.
fn limit_blockers(authority: &TestAuthority, account: &Account) -> Vec<Blocker> {
    let limits = account
        .summary()
        .map(|summary| summary.limits)
        .unwrap_or_default();
    let missing = missing_limits(authority, &limits);
    if missing.is_empty() {
        return Vec::new();
    }
    vec![Blocker {
        code: ErrorCode::AccountRequired,
        message: "Paid provider tests need positive usd_micros and requests limits for every listed scope. Add them with account set-limits.".into(),
        context: json!({ "missing_limits": missing }),
    }]
}

fn missing_limits(authority: &TestAuthority, limits: &[Limit]) -> Vec<Value> {
    let free_only = authority.routes.is_empty();
    if free_only {
        return Vec::new();
    }
    let periods = [Window::Day, Window::Month].map(|window| (Scope::Account, window));
    let accounts: BTreeSet<&String> = authority.routes.values().collect();
    let lifetimes = accounts
        .into_iter()
        .map(|account| (Scope::Provider(account.clone()), Window::Lifetime));
    let required = periods.into_iter().chain(lifetimes);
    required
        .filter(|(scope, window)| !positive_limit(limits, scope, *window))
        .map(|(scope, window)| json!({ "scope": scope, "window": window }))
        .collect()
}

fn confirm(plan: &TestPlan, options: &TestOptions<'_>) -> Result<(), Diagnostic> {
    let review = Review {
        plan_hash: &plan.plan_hash,
        blockers: &plan.blockers,
        reviewed: options.plan_hash,
        yes: options.yes,
    };
    review.confirm("Review the test plan first, then pass --plan-hash HASH, or --yes to accept the plan computed now.")
}

/// Binds the settings folder and per-use routes, admits the job, then runs every probe.
fn run_probes(settings: &Settings, plan: &TestPlan) -> Result<Payload, Diagnostic> {
    let job_id = job_id(&plan.plan_hash);
    let account = hold_account(&format!("providers test {job_id}"))?;
    let job = plan.job(&job_id, &settings.root)?;
    admit(&account, plan, &job)?;
    let kit = Kit::load(&settings.paths, &settings.providers)
        .map_err(|error| Diagnostic::new(ErrorCode::ExecutionFailed, &error.to_string()))?;
    let gate = AccountGate::for_job(account, &job);
    let run = Run {
        kit: &kit,
        gate: &gate,
        namespace: &job_id,
    };
    let outcomes = provider_test::run(&run, &plan.authority.probes)
        .map_err(|error| Diagnostic::new(ErrorCode::ExecutionFailed, &error.to_string()))?;
    let statuses: Vec<Status> = outcomes.iter().map(|outcome| outcome.status).collect();
    let results = attempt_ids(&gate, outcomes);
    let tested = tested(plan, Some(&job_id), &results);
    Ok(report(tested, verdict(&statuses)))
}

fn admit(account: &Account, plan: &TestPlan, job: &Job) -> Result<(), Diagnostic> {
    let account_error = |error| crate::account::commands::diagnostic(&error);
    account
        .register_workspace(job.workspace.clone())
        .map_err(account_error)?;
    for (route, provider_account) in &plan.authority.routes {
        account
            .register_route(route, provider_account)
            .map_err(account_error)?;
    }
    account.admit(job.clone()).map_err(account_error)
}

/// Each result, with the account attempt behind a per-use probe.
fn attempt_ids(gate: &AccountGate, outcomes: Vec<ProbeOutcome>) -> Vec<Value> {
    outcomes
        .into_iter()
        .map(|outcome| {
            let attempt = outcome
                .request
                .as_deref()
                .and_then(|request| gate.attempt_id(request).ok());
            let mut result = json!(outcome);
            result["attempt_id"] = json!(attempt);
            result
        })
        .collect()
}

fn skipped(plan: &TestPlan) -> Vec<Value> {
    let probes = plan.authority.probes.iter();
    probes
        .map(|probe| json!({ "provider": probe.provider, "status": Status::Skipped, "health_recorded": false }))
        .collect()
}

fn tested(plan: &TestPlan, job_id: Option<&str>, results: &[Value]) -> Value {
    let usage = job_id
        .and_then(|job| open_account().ok()?.job_usage(job).ok())
        .map_or(Value::Null, |usage| json!(usage));
    json!({
        "job_id": job_id,
        "plan_hash": plan.plan_hash,
        "results": results,
        "usage": usage,
    })
}

/// Account refusals and unresolved charges block; any other failed probe fails the command.
fn report(tested: Value, verdict: Option<(ErrorCode, &str)>) -> Payload {
    let Some((code, message)) = verdict else {
        return completed(json!({ "tested": tested }));
    };
    let mut diagnostic = Diagnostic::new(code, message);
    diagnostic.context = tested;
    diagnostic.into_payload()
}

fn verdict(statuses: &[Status]) -> Option<(ErrorCode, &'static str)> {
    let accounting = statuses
        .iter()
        .any(|status| matches!(status, Status::Refused | Status::Uncertain));
    if accounting {
        return Some((
            ErrorCode::AccountRequired,
            "Accounting refused a provider test or a paid test charge is unresolved. Inspect account attempts.",
        ));
    }
    let failed = statuses
        .iter()
        .any(|status| !matches!(status, Status::Ok | Status::Skipped | Status::Cooldown));
    failed.then_some((
        ErrorCode::ExecutionFailed,
        "At least one provider failed its test. See each result; health was recorded where possible.",
    ))
}

const fn completed(contents: Value) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents,
    }
}

fn digest<T: Serialize>(value: &T) -> Result<String, Diagnostic> {
    retold_engine::store::hash(value).map_err(|_| invalid("Could not hash the test plan."))
}

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, message)
}
