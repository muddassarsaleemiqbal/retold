//! Account-gated paid dispatch: the reservation and dispatch transition precede provider I/O,
//! refusals never reach the provider, and ambiguous outcomes keep their holds. Offline fakes only.
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

use retold_core::{Tier, Usd};
use retold_engine::context::Context;
use retold_engine::gate::{GateError, PaidCall, PaidEvidence, PaidGate};
use retold_engine::ledger::{Charge, Ledger, LedgerError};
use retold_engine::media_runtime::{MediaError, MediaRegistry};
use retold_engine::paid_text::PaidText;
use retold_engine::store::{hash, stage_of};
use retold_providers::config::Found;
use retold_providers::media::{ImageRequest, SpeechRequest};
use retold_providers::{ProvidersConfig, Router, TextRequest};
use serde_json::{Value, json};
use support::{MediaFake, TextFake};
use tokio_util::sync::CancellationToken;

const RUN_BUDGET: Usd = Usd::from_micros(1_000_000);
const FAKE_QUOTE: Usd = Usd::from_micros(10_000);
const FAKE_COST: Usd = Usd::from_micros(5_000);
const NO_DISPATCH_YET: usize = usize::MAX;
/// The shared text fake's provider ID; its quotes come from a media fake with the same ID.
const PAID_TEXT: &str = "fixture";

#[derive(Debug)]
struct RecordingGate {
    media: Arc<MediaFake>,
    events: Mutex<Vec<String>>,
    refuse_reserve: AtomicBool,
    refuse_dispatch: AtomicBool,
    images_at_dispatch: AtomicUsize,
}

impl RecordingGate {
    fn new(media: &Arc<MediaFake>) -> Arc<Self> {
        Arc::new(Self {
            media: Arc::clone(media),
            events: Mutex::new(Vec::new()),
            refuse_reserve: AtomicBool::new(false),
            refuse_dispatch: AtomicBool::new(false),
            images_at_dispatch: AtomicUsize::new(NO_DISPATCH_YET),
        })
    }

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
            "reserve {} {:?} {}",
            call.stage,
            call.capability,
            call.ceiling.micros()
        ));
        let refused = self.refuse_reserve.load(Ordering::SeqCst);
        if refused {
            return Err(GateError::Refused("fixture ceiling".into()));
        }
        Ok(())
    }

    fn dispatch(&self, _: &str) -> Result<(), GateError> {
        self.record("dispatch".into());
        let calls_so_far = self.media.images.load(Ordering::SeqCst);
        self.images_at_dispatch
            .store(calls_so_far, Ordering::SeqCst);
        let blocked = self.refuse_dispatch.load(Ordering::SeqCst);
        if blocked {
            return Err(GateError::Blocked("fixture authority expired".into()));
        }
        Ok(())
    }

    fn withdraw(&self, _: &str) -> Result<(), GateError> {
        self.record("withdraw".into());
        Ok(())
    }

    fn settle(&self, _: &str, evidence: &PaidEvidence<'_>) -> Result<(), GateError> {
        let cost = evidence.cost.map(Usd::micros);
        self.record(format!("settle {} {cost:?}", evidence.artifact_hash));
        Ok(())
    }

    fn reconcile(&self, _: &str, cost: Usd) -> Result<(), GateError> {
        self.record(format!("reconcile {}", cost.micros()));
        Ok(())
    }
}

struct Gated {
    _sandbox: tempfile::TempDir,
    context: Context,
    media: Arc<MediaFake>,
    gate: Arc<RecordingGate>,
}

fn gated() -> Gated {
    let (sandbox, shared, _, media) = support::setup(&["en"], RUN_BUDGET);
    let gate = RecordingGate::new(&media);
    let mut context = (*shared).clone();
    let ledger_path = context.layout.meta().join("ledger.json");
    let ledger = Ledger::open(&ledger_path, RUN_BUDGET).unwrap();
    context.media.ledger = Arc::new(ledger.with_gate(Arc::<RecordingGate>::clone(&gate)));
    Gated {
        _sandbox: sandbox,
        context,
        media,
        gate,
    }
}

fn frame_request() -> ImageRequest {
    ImageRequest {
        prompt: "A quiet harbour at dawn".into(),
        references: vec![],
        aspect: "9:16".into(),
        resolution: "720x1280".into(),
    }
}

async fn draw(fixture: &Gated) -> Result<retold_engine::media_runtime::Asset, MediaError> {
    let path = fixture.context.layout.video_dir().join("frame.png");
    fixture
        .context
        .media
        .image("frames-1", Tier::Image, &frame_request(), &path)
        .await
}

#[tokio::test]
async fn image_dispatch_crosses_the_gate_before_the_provider_call() {
    let fixture = gated();
    let asset = draw(&fixture).await.unwrap();
    let reserve = format!(
        "reserve {} Image {}",
        stage_of("frames-1"),
        FAKE_QUOTE.micros()
    );
    let settle = format!("settle {} Some({})", asset.content_hash, FAKE_COST.micros());
    assert_eq!(fixture.gate.events(), [reserve, "dispatch".into(), settle]);
    assert_eq!(fixture.gate.images_at_dispatch.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.media.images.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.context.media.ledger.total().unwrap(), FAKE_COST);
}

#[tokio::test]
async fn refused_reservation_never_calls_the_provider_or_keeps_a_local_hold() {
    let fixture = gated();
    fixture.gate.refuse_reserve.store(true, Ordering::SeqCst);
    let error = draw(&fixture).await.unwrap_err();
    assert!(matches!(
        error,
        MediaError::Ledger(LedgerError::Gate(GateError::Refused(_)))
    ));
    assert_eq!(fixture.media.images.load(Ordering::SeqCst), 0);
    assert!(fixture.context.media.ledger.entries().unwrap().is_empty());
}

#[tokio::test]
async fn refused_dispatch_withdraws_and_never_reuses_the_slot() {
    let fixture = gated();
    fixture.gate.refuse_dispatch.store(true, Ordering::SeqCst);
    let error = draw(&fixture).await.unwrap_err();
    assert!(matches!(
        error,
        MediaError::Ledger(LedgerError::Gate(GateError::Blocked(_)))
    ));
    assert_eq!(fixture.media.images.load(Ordering::SeqCst), 0);
    assert!(
        fixture
            .gate
            .events()
            .ends_with(&["dispatch".into(), "withdraw".into()])
    );
    let ledger = &fixture.context.media.ledger;
    let entries = ledger.entries().unwrap();
    assert!(matches!(
        entries.values().collect::<Vec<_>>()[..],
        [Charge::Exact { cost: Usd::ZERO }]
    ));
    fixture.gate.refuse_dispatch.store(false, Ordering::SeqCst);
    let asset = draw(&fixture).await.unwrap();
    assert!(asset.request_id.ends_with("-retry-1"));
}

#[tokio::test]
async fn ambiguous_speech_failure_keeps_its_hold_and_never_settles() {
    let fixture = gated();
    fixture.media.fail_speech.store(true, Ordering::SeqCst);
    let request = SpeechRequest {
        text: "The harbour was quiet.".into(),
        voice: "narrator".into(),
        instructions: None,
        language: "en".parse().unwrap(),
    };
    let path = fixture.context.layout.video_dir().join("line.wav");
    let error = fixture
        .context
        .media
        .speech("voices-en-1", "openrouter", "model", &request, &path)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        MediaError::Ledger(LedgerError::Uncertain(_))
    ));
    let events = fixture.gate.events();
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(events[1], "dispatch");
    assert_eq!(fixture.context.media.ledger.total().unwrap(), FAKE_QUOTE);
}

#[test]
fn gated_ledgers_refuse_calls_that_would_bypass_the_account() {
    let fixture = gated();
    let ledger = &fixture.context.media.ledger;
    assert!(ledger.reserve("direct", FAKE_QUOTE).is_err());
    assert!(ledger.settle("direct", Some(FAKE_COST), None).is_err());
    assert!(ledger.entries().unwrap().is_empty());
}

/// The shared text fake billed per use, quoted by a paid media fake under the same route ID.
fn paid_text_routes() -> (Router, MediaRegistry) {
    let text = Arc::new(TextFake {
        per_use: true,
        ..TextFake::default()
    });
    let quotes = Arc::new(MediaFake {
        paid: true,
        id_override: Some(PAID_TEXT),
        ..MediaFake::default()
    });
    let mut config = ProvidersConfig::suggested(Found {
        claude: false,
        codex: false,
    });
    let route = format!("{PAID_TEXT}:model").parse().unwrap();
    config.routing.insert(Tier::TextSmart, vec![route]);
    let router = Router::from_providers(vec![(text, 1)], config.routing.clone());
    let registry = MediaRegistry::from_providers(config, vec![(quotes, 1)]);
    (router, registry)
}

fn faces_request() -> TextRequest {
    TextRequest {
        system: None,
        prompt: "Count the faces".into(),
        schema: json!({"properties": {"faces": {}}}),
        images: vec![],
        web: false,
    }
}

fn per_use_text(fixture: &Gated) -> (Router, PaidText) {
    let (router, registry) = paid_text_routes();
    let paid = PaidText {
        registry: Arc::new(registry),
        ledger: Arc::clone(&fixture.context.media.ledger),
        allow_paid: true,
    };
    (router, paid)
}

#[tokio::test]
async fn paid_text_settles_through_the_gate_with_its_response_hash() {
    let fixture = gated();
    let (router, paid) = per_use_text(&fixture);
    let cancel = CancellationToken::new();
    let (_, _, settlement) = paid
        .generate::<Value>(
            &router,
            Tier::TextSmart,
            ("script-1", "key"),
            &faces_request(),
            &cancel,
        )
        .await
        .unwrap();
    paid.settle(&settlement).unwrap();
    let reserve = format!(
        "reserve {} Text {}",
        stage_of("script-1"),
        FAKE_QUOTE.micros()
    );
    let answer = hash(&json!({"faces": []})).unwrap();
    // The fake reports no exact cost, so the full quote stays held.
    let settle = format!("settle {answer} None");
    assert_eq!(fixture.gate.events(), [reserve, "dispatch".into(), settle]);
    assert_eq!(fixture.context.media.ledger.total().unwrap(), FAKE_QUOTE);
}

#[derive(Debug)]
struct CoolingDown;
impl retold_providers::health::Health for CoolingDown {
    fn check(&self, _: &str) -> Result<(), retold_providers::ProviderError> {
        Err(retold_providers::ProviderError::RateLimited {
            message: "recorded cooldown".into(),
            retry_after: None,
        })
    }
    fn failed(
        &self,
        _: &str,
        _: &retold_providers::ProviderError,
    ) -> Result<(), retold_providers::ProviderError> {
        Ok(())
    }
}

#[tokio::test]
async fn media_cooldown_refuses_before_provider_calls_or_budget_holds() {
    let mut fixture = gated();
    let registry = Arc::get_mut(&mut fixture.context.media.registry).unwrap();
    registry.health = Some(Arc::new(CoolingDown));
    assert!(draw(&fixture).await.is_err());
    assert_eq!(fixture.media.images.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.gate.events(), Vec::<String>::new());
}
