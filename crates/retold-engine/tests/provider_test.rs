//! Provider tests plan without building or contacting providers. Free probes answer directly;
//! per-use probes cross the gate before any request, and ambiguous failures keep their holds.
//! Offline fakes only.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared support module"
)]
mod support {
    include!("support/mod.rs");
}

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use retold_core::Usd;
use retold_engine::gate::{GateError, PaidCall, PaidEvidence, PaidGate};
use retold_engine::media_runtime::MediaRegistry;
use retold_engine::provider_health::HealthLedger;
use retold_engine::provider_test::{
    self, Allowance, Decision, Kit, Outcome, Probe, Run, Selection, Skip, Status,
};
use retold_engine::session::Providers;
use retold_engine::{Paths, provider_settings};
use retold_providers::text::{Billing, TextProvider, TextRequest, TextResponse, Usage};
use retold_providers::{ProviderError, Router};
use serde_json::json;
use support::MediaFake;
use tokio_util::sync::CancellationToken;

const PROVIDERS_TOML: &str = r#"
schema_version = 1
[providers.claude]
kind = "claude_cli"
max_parallel = 1
[providers.claude.tiers]
smart = "sonnet"
[providers.codex]
kind = "codex_cli"
max_parallel = 1
[providers.paid]
kind = "openrouter"
key_env = "FIXTURE_KEY"
max_parallel = 1
[providers.voice]
kind = "local_voice"
max_parallel = 1
[routing]
"text.smart" = ["claude:smart", "paid:vendor/model"]
"#;
const QUOTE_MICROS: i64 = 10_000;
const COOLDOWN: std::time::Duration = std::time::Duration::from_secs(3_600);
const EVERYONE: Selection<'static> = Selection {
    provider: None,
    allowance: Allowance::PerUse,
};

fn installation() -> (tempfile::TempDir, Paths, Providers) {
    let sandbox = tempfile::tempdir().unwrap();
    std::fs::write(sandbox.path().join("providers.toml"), PROVIDERS_TOML).unwrap();
    let paths = Paths {
        config: sandbox.path().into(),
        library: sandbox.path().into(),
    };
    let providers = Providers::load(&paths).unwrap();
    (sandbox, paths, providers)
}

const fn only(provider: &str, allowance: Allowance) -> Selection<'_> {
    Selection {
        provider: Some(provider),
        allowance,
    }
}

fn decision(probes: &[Probe], provider: &str) -> Decision {
    let probe = probes.iter().find(|probe| probe.provider == provider);
    probe.unwrap().decision.clone()
}

const fn skip(reason: Skip) -> Decision {
    Decision::Skip { reason }
}

#[test]
fn plans_test_routed_text_providers_and_skip_paid_ones_without_spend() {
    let (sandbox, _, providers) = installation();
    let free_only = Selection {
        allowance: Allowance::FreeOnly,
        ..EVERYONE
    };
    let free = provider_test::plan(&providers, free_only).unwrap();
    let routed = Decision::Test {
        tier: "text.smart".into(),
        selector: "smart".into(),
    };
    assert_eq!(decision(&free, "claude"), routed);
    assert_eq!(decision(&free, "codex"), skip(Skip::Unrouted));
    assert_eq!(decision(&free, "paid"), skip(Skip::NeedsSpend));
    assert_eq!(decision(&free, "voice"), skip(Skip::NoTextCapability));
    let paid = provider_test::plan(&providers, only("paid", Allowance::PerUse)).unwrap();
    assert_eq!(paid.len(), 1);
    assert!(paid[0].is_paid());
    assert!(provider_test::plan(&providers, only("missing", Allowance::PerUse)).is_err());
    let files = std::fs::read_dir(sandbox.path()).unwrap().count();
    assert_eq!(files, 1, "planning writes nothing");
}

/// Answers the probe, or fails with a fixed error.
#[derive(Debug)]
struct Probed {
    id: &'static str,
    billing: Billing,
    failure: Option<ProviderError>,
    calls: AtomicUsize,
}

impl Probed {
    fn answering(id: &'static str, billing: Billing) -> Arc<Self> {
        Arc::new(Self {
            id,
            billing,
            failure: Option::default(),
            calls: AtomicUsize::new(0),
        })
    }

    fn failing(id: &'static str, billing: Billing, error: ProviderError) -> Arc<Self> {
        Arc::new(Self {
            id,
            billing,
            failure: Some(error),
            calls: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl TextProvider for Probed {
    fn id(&self) -> &str {
        self.id
    }
    fn billing(&self) -> Billing {
        self.billing
    }
    fn supports_images(&self) -> bool {
        bool::default()
    }
    fn resolve_model(&self, selector: &str) -> String {
        selector.into()
    }
    async fn generate(
        &self,
        _: &TextRequest,
        _: &str,
        _: &CancellationToken,
    ) -> Result<TextResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let answer = TextResponse {
            value: json!({ "ok": true }),
            usage: Usage::default(),
        };
        self.failure.clone().map_or(Ok(answer), Err)
    }
}

#[derive(Debug, Default)]
struct RecordingGate {
    events: Mutex<Vec<String>>,
    refuse: AtomicBool,
}

impl RecordingGate {
    fn record(&self, event: String) {
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }

    fn events(&self) -> Vec<String> {
        let events = self.events.lock().unwrap();
        events.clone()
    }
}

impl PaidGate for RecordingGate {
    fn reserve(&self, call: &PaidCall<'_>) -> Result<(), GateError> {
        self.record(format!(
            "reserve {} {}",
            call.request,
            call.ceiling.micros()
        ));
        let refused = self.refuse.load(Ordering::SeqCst);
        if refused {
            return Err(GateError::Refused("fixture ceiling".into()));
        }
        Ok(())
    }
    fn dispatch(&self, request: &str) -> Result<(), GateError> {
        self.record(format!("dispatch {request}"));
        Ok(())
    }
    fn withdraw(&self, request: &str) -> Result<(), GateError> {
        self.record(format!("withdraw {request}"));
        Ok(())
    }
    fn settle(&self, request: &str, _: &PaidEvidence<'_>) -> Result<(), GateError> {
        self.record(format!("settle {request}"));
        Ok(())
    }
    fn reconcile(&self, request: &str, cost: Usd) -> Result<(), GateError> {
        self.record(format!("reconcile {request} {}", cost.micros()));
        Ok(())
    }
}

/// The given text providers; quotes for `paid` come from a per-use media fake.
fn kit(paths: &Paths, providers: Vec<Arc<Probed>>) -> Kit {
    let config = provider_settings::load(&paths.config).unwrap();
    let text = providers
        .into_iter()
        .map(|provider| (provider as Arc<dyn TextProvider>, 1))
        .collect();
    let quotes = Arc::new(MediaFake {
        paid: true,
        id_override: Some("paid"),
        ..MediaFake::default()
    });
    Kit {
        router: Router::from_providers(text, config.routing.clone()),
        registry: MediaRegistry::from_providers(config.clone(), vec![(quotes, 1)]),
        health: HealthLedger::new(&paths.config, &config).unwrap(),
    }
}

/// Plans from the same fixture settings the kit was built from, then tests.
fn test(kit: &Kit, gate: &RecordingGate, selection: Selection<'_>) -> Vec<Outcome> {
    let (_sandbox, _, providers) = installation();
    let probes = provider_test::plan(&providers, selection).unwrap();
    let run = Run {
        kit,
        gate,
        namespace: "job",
    };
    provider_test::run(&run, &probes).unwrap()
}

fn status_of(outcomes: &[Outcome], provider: &str) -> Status {
    let outcome = outcomes.iter().find(|outcome| outcome.provider == provider);
    outcome.unwrap().status
}

fn recorded(kit: &Kit, provider: &str) -> Option<String> {
    let health = &kit.health;
    let observation = health.observation(provider).unwrap();
    observation.map(|observation| observation.kind)
}

#[test]
fn free_and_paid_probes_record_health_and_cross_the_gate_first() {
    let (_sandbox, paths, _) = installation();
    let claude = Probed::answering("claude", Billing::Subscription);
    let paid = Probed::answering("paid", Billing::PerUse);
    let kit = kit(&paths, vec![claude, paid]);
    let gate = RecordingGate::default();
    let outcomes = test(&kit, &gate, EVERYONE);
    let statuses = ["claude", "paid", "codex"].map(|id| status_of(&outcomes, id));
    assert_eq!(statuses, [Status::Ok, Status::Ok, Status::Skipped]);
    let expected = [
        format!("reserve job:paid {QUOTE_MICROS}"),
        "dispatch job:paid".into(),
        "settle job:paid".into(),
    ];
    assert_eq!(gate.events(), expected);
    assert_eq!(recorded(&kit, "claude").as_deref(), Some("ok"));
    let sent = outcomes
        .iter()
        .filter(|outcome| outcome.status != Status::Skipped);
    assert!(sent.into_iter().all(|outcome| outcome.health_recorded));
}

#[test]
fn refused_paid_probes_never_reach_the_provider() {
    let (_sandbox, paths, _) = installation();
    let paid = Probed::answering("paid", Billing::PerUse);
    let kit = kit(&paths, vec![Arc::clone(&paid)]);
    let gate = RecordingGate::default();
    gate.refuse.store(true, Ordering::SeqCst);
    let outcomes = test(&kit, &gate, only("paid", Allowance::PerUse));
    assert_eq!(status_of(&outcomes, "paid"), Status::Refused);
    assert_eq!(paid.calls(), 0);
    assert_eq!(recorded(&kit, "paid"), None);
}

#[test]
fn failures_are_classified_and_ambiguous_paid_failures_keep_their_hold() {
    let (_sandbox, paths, _) = installation();
    let logged_out = ProviderError::Unavailable("please log in with /login".into());
    let claude = Probed::failing("claude", Billing::Subscription, logged_out);
    let timeout = ProviderError::Timeout("no reply".into());
    let paid = Probed::failing("paid", Billing::PerUse, timeout);
    let kit = kit(&paths, vec![claude, paid]);
    let gate = RecordingGate::default();
    let outcomes = test(&kit, &gate, EVERYONE);
    assert_eq!(status_of(&outcomes, "claude"), Status::LoggedOut);
    assert_eq!(status_of(&outcomes, "paid"), Status::Uncertain);
    let events = gate.events();
    let resolved = events
        .iter()
        .any(|event| event.starts_with("reconcile") || event.starts_with("settle"));
    assert!(!resolved, "{events:?}");
    assert_eq!(recorded(&kit, "claude").as_deref(), Some("logged_out"));
}

#[test]
fn recorded_cooldowns_skip_the_probe_without_sending() {
    use retold_providers::health::Health;
    let (_sandbox, paths, _) = installation();
    let claude = Probed::answering("claude", Billing::Subscription);
    let kit = kit(&paths, vec![Arc::clone(&claude)]);
    let limited = ProviderError::RateLimited {
        message: "slow down".into(),
        retry_after: Some(COOLDOWN),
    };
    kit.health.failed("claude", &limited).unwrap();
    let gate = RecordingGate::default();
    let outcomes = test(&kit, &gate, only("claude", Allowance::FreeOnly));
    assert_eq!(status_of(&outcomes, "claude"), Status::Cooldown);
    assert_eq!(claude.calls(), 0);
}
