//! Explicit provider tests. Each selected provider answers one tiny structured text request
//! through the first text route that names it. Free providers answer directly; per-use providers
//! first cross the caller's accounting gate, so a test can never spend outside its authority.
//! Results update the durable health ledger. Planning reads settings only and sends nothing.

use retold_core::{Capability, Tier, Usd};
use retold_providers::health::Health;
use retold_providers::{ProviderConfig, ProviderError, ProvidersConfig, Router, TextProvider};
use retold_providers::{RouteSpec, TextRequest, TextResponse};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::gate::{GateError, PaidCall, PaidCapability, PaidEvidence, PaidGate};
use crate::media_runtime::MediaRegistry;
use crate::provider_health::{HealthLedger, is_login_problem};
use crate::session::{Providers, Secrets, interrupt};
use crate::{EngineError, Paths};

/// The operation stage every provider-test attempt draws on.
pub const STAGE: &str = "provider-test";
const DETAIL_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Billing {
    Subscription,
    Local,
    PerUse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Skip {
    /// Image, speech and transcription probes are not implemented.
    NoTextCapability,
    /// No text route names the provider, so there is no selector to test.
    Unrouted,
    /// Per-use tests need a positive allowance.
    NeedsSpend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Test { tier: String, selector: String },
    Skip { reason: Skip },
}

/// What a test would do for one configured provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Probe {
    pub provider: String,
    pub kind: String,
    pub billing: Billing,
    /// Every tier the adapter supports; only text is tested.
    pub tiers: Vec<String>,
    #[serde(flatten)]
    pub decision: Decision,
}

impl Probe {
    #[must_use]
    pub const fn sends(&self) -> bool {
        matches!(self.decision, Decision::Test { .. })
    }

    #[must_use]
    pub const fn is_paid(&self) -> bool {
        self.sends() && matches!(self.billing, Billing::PerUse)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("no provider named {0} is configured")]
pub struct UnknownProvider(pub String);

/// Whether per-use providers may be tested at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allowance {
    /// The default: only subscription and local providers.
    FreeOnly,
    /// A positive spend; the gate still bounds every attempt.
    PerUse,
}

/// Which providers to plan, and what they may cost.
#[derive(Debug, Clone, Copy)]
pub struct Selection<'a> {
    /// One provider, or every configured provider when absent.
    pub provider: Option<&'a str>,
    pub allowance: Allowance,
}

/// The probes for the selected providers. Reads settings only.
pub fn plan(
    providers: &Providers,
    selection: Selection<'_>,
) -> Result<Vec<Probe>, UnknownProvider> {
    let planner = Planner {
        config: providers.config(),
        allowance: selection.allowance,
    };
    let wanted = selection.provider;
    let probes: Vec<Probe> = planner
        .config
        .providers
        .iter()
        .filter(|(id, _)| wanted.is_none_or(|only| only == id.as_str()))
        .map(|(id, spec)| planner.probe(id, spec))
        .collect();
    match wanted {
        Some(id) if probes.is_empty() => Err(UnknownProvider(id.into())),
        _ => Ok(probes),
    }
}

struct Planner<'a> {
    config: &'a ProvidersConfig,
    allowance: Allowance,
}

impl Planner<'_> {
    fn probe(&self, id: &str, spec: &ProviderConfig) -> Probe {
        let billing = billing(spec);
        Probe {
            provider: id.into(),
            kind: kind(spec),
            billing,
            tiers: tiers(spec),
            decision: self.decision(id, spec, billing),
        }
    }

    fn decision(&self, id: &str, spec: &ProviderConfig, billing: Billing) -> Decision {
        let skip = |reason| Decision::Skip { reason };
        if !spec.supports_tier(Tier::TextFast) {
            return skip(Skip::NoTextCapability);
        }
        let Some((tier, route)) = first_text_route(self.config, id) else {
            return skip(Skip::Unrouted);
        };
        let unaffordable = billing == Billing::PerUse && self.allowance == Allowance::FreeOnly;
        if unaffordable {
            return skip(Skip::NeedsSpend);
        }
        Decision::Test {
            tier: tier.as_str().into(),
            selector: route.selector.clone(),
        }
    }
}

fn first_text_route<'a>(config: &'a ProvidersConfig, id: &str) -> Option<(Tier, &'a RouteSpec)> {
    let text_routes = config
        .routing
        .iter()
        .filter(|(tier, _)| tier.capability() == Capability::Text);
    text_routes.into_iter().find_map(|(tier, chain)| {
        let route = chain.iter().find(|route| route.provider == id);
        route.map(|route| (*tier, route))
    })
}

const fn billing(spec: &ProviderConfig) -> Billing {
    match spec {
        ProviderConfig::OpenRouter { .. } => Billing::PerUse,
        ProviderConfig::LocalText { .. }
        | ProviderConfig::LocalVoice { .. }
        | ProviderConfig::LocalWhisper { .. } => Billing::Local,
        ProviderConfig::ClaudeCli { .. }
        | ProviderConfig::CodexCli { .. }
        | ProviderConfig::GeminiCli { .. } => Billing::Subscription,
    }
}

fn kind(spec: &ProviderConfig) -> String {
    let value = serde_json::to_value(spec).unwrap_or_default();
    value["kind"].as_str().unwrap_or("unknown").into()
}

fn tiers(spec: &ProviderConfig) -> Vec<String> {
    let supported = Tier::ALL
        .into_iter()
        .filter(|&tier| spec.supports_tier(tier));
    supported.map(|tier| tier.as_str().into()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The provider answered the probe.
    Ok,
    /// Nothing was sent; see the plan's reason.
    Skipped,
    /// A recorded cooldown has not ended; nothing was sent.
    Cooldown,
    Limited,
    LoggedOut,
    /// The provider could not be built or reached.
    Unavailable,
    /// The provider failed, timed out or did not answer the probe.
    Failing,
    /// Accounting refused the attempt before anything was sent.
    Refused,
    /// A per-use attempt may have been charged; its hold stays until reconciled.
    Uncertain,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub provider: String,
    pub status: Status,
    /// The per-use attempt slot the gate saw, when there was one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    /// One line of at most 200 characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub health_recorded: bool,
}

impl Outcome {
    fn new(probe: &Probe, status: Status) -> Self {
        Self {
            provider: probe.provider.clone(),
            status,
            request: None,
            detail: None,
            health_recorded: false,
        }
    }

    fn detail(mut self, detail: &str) -> Self {
        self.detail = Some(one_line(detail));
        self
    }
}

/// Built providers, their quotes and the health ledger tests record into.
#[derive(Debug)]
pub struct Kit {
    pub router: Router,
    pub registry: MediaRegistry,
    pub health: HealthLedger,
}

impl Kit {
    /// Builds every configured provider without contacting any of them.
    pub fn load(paths: &Paths, providers: &Providers) -> Result<Self, EngineError> {
        let secrets = Secrets::load(paths);
        let lookup = |name: &str| secrets.get(name);
        let config = providers.config();
        let health = HealthLedger::new(&paths.config, config)
            .map_err(|_| EngineError::Setup("Could not initialize provider health.".into()))?;
        Ok(Self {
            router: Router::from_config(config, &lookup),
            registry: MediaRegistry::from_config(config, &lookup),
            health,
        })
    }
}

/// One test invocation: per-use attempts are named `namespace:provider` at the gate.
#[derive(Debug, Clone, Copy)]
pub struct Run<'a> {
    pub kit: &'a Kit,
    pub gate: &'a dyn PaidGate,
    pub namespace: &'a str,
}

impl Run<'_> {
    #[must_use]
    pub fn request(&self, provider: &str) -> String {
        format!("{}:{provider}", self.namespace)
    }
}

/// Tests each probe in order on a fresh runtime. The first Ctrl-C cancels gracefully; a
/// second also ends provider processes.
pub fn run(run: &Run<'_>, probes: &[Probe]) -> Result<Vec<Outcome>, EngineError> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| EngineError::Setup(format!("starting the runtime: {error}")))?;
    let cancel = CancellationToken::new();
    Ok(runtime.block_on(async {
        let watcher = tokio::spawn(interrupt(cancel.clone()));
        let outcomes = test(run, probes, &cancel).await;
        watcher.abort();
        outcomes
    }))
}

pub async fn test(run: &Run<'_>, probes: &[Probe], cancel: &CancellationToken) -> Vec<Outcome> {
    let mut outcomes = Vec::with_capacity(probes.len());
    for probe in probes {
        outcomes.push(one(run, probe, cancel).await);
    }
    outcomes
}

async fn one(run: &Run<'_>, probe: &Probe, cancel: &CancellationToken) -> Outcome {
    let Decision::Test { selector, .. } = &probe.decision else {
        return Outcome::new(probe, Status::Skipped);
    };
    if cancel.is_cancelled() {
        return Outcome::new(probe, Status::Cancelled);
    }
    if let Err(outcome) = admitted(run.kit, probe) {
        return outcome;
    }
    let router = &run.kit.router;
    let Some(provider) = router.text_provider(&probe.provider) else {
        let reason = router.unavailable().get(&probe.provider);
        let reason = reason.map_or("the provider could not be built", String::as_str);
        return Outcome::new(probe, Status::Unavailable).detail(reason);
    };
    let attempt = Attempt {
        run,
        probe,
        model: provider.resolve_model(selector),
        provider: provider.as_ref(),
    };
    attempt.conclude(cancel).await
}

/// A recorded cooldown, or unreadable health storage, stops the probe before any request.
fn admitted(kit: &Kit, probe: &Probe) -> Result<(), Outcome> {
    let cooldown = kit.health.cooldown_until(&probe.provider).map_err(|_| {
        Outcome::new(probe, Status::Unavailable)
            .detail("Provider health storage is unavailable or invalid; nothing was sent.")
    })?;
    match cooldown {
        Some(until) => Err(Outcome::new(probe, Status::Cooldown)
            .detail(&format!("recorded cooldown until {until} (Unix seconds)"))),
        None => Ok(()),
    }
}

/// What came back from one probe.
enum Answer {
    Replied(TextResponse),
    /// The provider failed and nothing stays charged.
    Failed(ProviderError),
    /// The provider failed after a per-use dispatch; the charge stays held until reconciled.
    Held(ProviderError),
    /// Accounting stopped or could not record the attempt.
    Stopped(Status, String),
}

/// One probe of one built provider.
struct Attempt<'a> {
    run: &'a Run<'a>,
    probe: &'a Probe,
    provider: &'a dyn TextProvider,
    model: String,
}

impl Attempt<'_> {
    async fn conclude(&self, cancel: &CancellationToken) -> Outcome {
        let per_use = self.probe.billing == Billing::PerUse;
        let answer = if per_use {
            self.paid(cancel).await
        } else {
            self.free(cancel).await
        };
        let mut outcome = conclude(&self.run.kit.health, self.probe, answer);
        if per_use {
            outcome.request = Some(self.slot());
        }
        outcome
    }

    fn slot(&self) -> String {
        self.run.request(&self.probe.provider)
    }

    async fn free(&self, cancel: &CancellationToken) -> Answer {
        let reply = self
            .provider
            .generate(&request(), &self.model, cancel)
            .await;
        reply.map_or_else(Answer::Failed, Answer::Replied)
    }

    async fn paid(&self, cancel: &CancellationToken) -> Answer {
        let ceiling = match self.quote(cancel).await {
            Ok(ceiling) => ceiling,
            Err(answer) => return answer,
        };
        let slot = self.slot();
        let call = PaidCall {
            request: &slot,
            logical: "",
            stage: STAGE,
            provider: &self.probe.provider,
            capability: PaidCapability::Text,
            ceiling,
        };
        let gate = self.run.gate;
        if let Err(error) = cross(gate, &call) {
            return Answer::Stopped(gate_status(&error), error.to_string());
        }
        let reply = self
            .provider
            .generate(&request(), &self.model, cancel)
            .await;
        settle(gate, &slot, reply)
    }

    async fn quote(&self, cancel: &CancellationToken) -> Result<Usd, Answer> {
        let slots = &self.run.kit.registry.slots;
        let Some(slot) = slots.get(&self.probe.provider) else {
            let reason = "the provider cannot quote a per-use request".into();
            return Err(Answer::Stopped(Status::Unavailable, reason));
        };
        let quoted = slot
            .provider
            .text_quote(&self.model, &request(), cancel)
            .await;
        quoted.map(|quote| quote.ceiling).map_err(Answer::Failed)
    }
}

/// Reserves, then crosses the one-time dispatch boundary; a reservation that cannot dispatch
/// is withdrawn.
fn cross(gate: &dyn PaidGate, call: &PaidCall<'_>) -> Result<(), GateError> {
    gate.reserve(call)?;
    gate.dispatch(call.request).inspect_err(|_| {
        // A failed withdrawal leaves a reservation the account already refuses to reuse.
        let _ = gate.withdraw(call.request);
    })
}

fn settle(gate: &dyn PaidGate, slot: &str, reply: Result<TextResponse, ProviderError>) -> Answer {
    match reply {
        Ok(response) => settle_reply(gate, slot, response),
        Err(error) if unbilled(&error) => match gate.reconcile(slot, Usd::ZERO) {
            Ok(()) => Answer::Failed(error),
            Err(gate_error) => Answer::Stopped(Status::Uncertain, gate_error.to_string()),
        },
        Err(error) => Answer::Held(error),
    }
}

fn settle_reply(gate: &dyn PaidGate, slot: &str, response: TextResponse) -> Answer {
    let Ok(artifact_hash) = crate::store::hash(&response.value) else {
        let reason = "the reply could not be recorded; its charge stays held".into();
        return Answer::Stopped(Status::Uncertain, reason);
    };
    let usage = &response.usage;
    let evidence = PaidEvidence {
        artifact_hash: &artifact_hash,
        cost: usage.cost_known.then_some(usage.cost),
    };
    match gate.settle(slot, &evidence) {
        Ok(()) => Answer::Replied(response),
        Err(error) => Answer::Stopped(Status::Uncertain, error.to_string()),
    }
}

/// Failures the provider confirms were never billed; the same set paid text releases.
const fn unbilled(error: &ProviderError) -> bool {
    matches!(
        error,
        ProviderError::Unbilled(_) | ProviderError::Unavailable(_) | ProviderError::Unsupported(_)
    )
}

const fn gate_status(error: &GateError) -> Status {
    match error {
        GateError::Refused(_) | GateError::Blocked(_) => Status::Refused,
        GateError::Uncertain(_) => Status::Uncertain,
    }
}

/// Records the result in the health ledger and classifies it.
fn conclude(health: &HealthLedger, probe: &Probe, answer: Answer) -> Outcome {
    let provider = probe.provider.as_str();
    match answer {
        Answer::Replied(response) if answers_probe(&response.value) => Outcome {
            health_recorded: health.succeeded(provider).is_ok(),
            ..Outcome::new(probe, Status::Ok)
        },
        Answer::Replied(_) => {
            let error = ProviderError::Malformed("the reply did not answer the probe".into());
            failed(health, probe, &error)
        }
        Answer::Failed(error) => failed(health, probe, &error),
        Answer::Held(error) => Outcome {
            status: Status::Uncertain,
            ..failed(health, probe, &error)
        },
        Answer::Stopped(status, detail) => Outcome::new(probe, status).detail(&detail),
    }
}

fn failed(health: &HealthLedger, probe: &Probe, error: &ProviderError) -> Outcome {
    Outcome {
        health_recorded: health.failed(&probe.provider, error).is_ok(),
        ..Outcome::new(probe, classify(error)).detail(&error.to_string())
    }
}

fn classify(error: &ProviderError) -> Status {
    match error {
        ProviderError::RateLimited { .. } => Status::Limited,
        ProviderError::Unavailable(message) if is_login_problem(message) => Status::LoggedOut,
        ProviderError::Unavailable(_) => Status::Unavailable,
        ProviderError::Cancelled => Status::Cancelled,
        _ => Status::Failing,
    }
}

fn answers_probe(value: &serde_json::Value) -> bool {
    value.get("ok") == Some(&serde_json::Value::Bool(true))
}

/// The smallest structured request every text adapter supports.
fn request() -> TextRequest {
    TextRequest {
        system: Some("You are a connectivity check. Reply with JSON only.".into()),
        prompt: "Return exactly {\"ok\": true}.".into(),
        schema: serde_json::json!({
            "type": "object",
            "properties": { "ok": { "type": "boolean" } },
            "required": ["ok"],
            "additionalProperties": false,
        }),
        images: Vec::new(),
        web: false,
    }
}

/// Provider messages can quote output; keep them short and on one line.
fn one_line(text: &str) -> String {
    let words: Vec<_> = text.split_whitespace().collect();
    truncated(&words.join(" "))
}

fn truncated(text: &str) -> String {
    text.chars().take(DETAIL_LIMIT).collect()
}
