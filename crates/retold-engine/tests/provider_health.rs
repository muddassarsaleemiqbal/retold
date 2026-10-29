//! Durable health uses offline observations; cooldown admission survives process recreation.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use retold_engine::provider_health::{HealthLedger, inspect};
use retold_providers::health::Health;
use retold_providers::{ProviderError, ProvidersConfig};
use std::time::Duration;

const COOLDOWN_SECONDS: u64 = 300;
const SOURCE_CONFIG: &str = "[providers.a]\nkind = 'codex_cli'\nmax_parallel = 1\n[providers.b]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]";
fn fixture() -> (tempfile::TempDir, ProvidersConfig) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("providers.toml"), SOURCE_CONFIG).unwrap();
    (root, toml::from_str(SOURCE_CONFIG).unwrap())
}

fn limited() -> ProviderError {
    ProviderError::RateLimited {
        message: "PRIVATE-TOKEN".into(),
        retry_after: Some(Duration::from_secs(COOLDOWN_SECONDS)),
    }
}

#[test]
fn cooldown_survives_recreation_and_identical_aliases_share_it() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    ledger.check("a").unwrap();
    ledger.failed("a", &limited()).unwrap();
    drop(ledger);
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    assert!(ledger.check("a").is_err());
    assert!(ledger.check("b").is_err());
    assert_eq!(inspect(root.path()).unwrap()[0].status, "cooldown");
    let text = std::fs::read_to_string(root.path().join("provider-health.json")).unwrap();
    assert!(!text.contains("PRIVATE"));
}

#[test]
fn shorter_observations_never_clear_longer_cooldowns() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    ledger.failed("a", &limited()).unwrap();
    let before = ledger.observation("a").unwrap().unwrap();
    ledger
        .failed("b", &ProviderError::Unavailable("PRIVATE".into()))
        .unwrap();
    ledger.failed("a", &ProviderError::Cancelled).unwrap();
    assert_eq!(
        ledger.observation("a").unwrap().unwrap().retry_at,
        before.retry_at
    );
}

#[test]
fn corrupt_future_or_busy_health_refuses_dispatch_without_repair() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    let target = root.path().join("provider-health.json");
    for bytes in ["PRIVATE", "{\"version\":99,\"records\":{}}"] {
        std::fs::write(&target, bytes).unwrap();
        assert!(ledger.check("a").is_err());
        assert!(ledger.failed("a", &limited()).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), bytes);
    }
    std::fs::remove_file(&target).unwrap();
    let lock = std::fs::File::create(root.path().join(".provider-health.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(ledger.failed("a", &limited()).is_err());
    assert!(!target.exists());
}

#[test]
fn elapsed_cooldown_is_eligible_without_claiming_health_or_writing() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    ledger.failed("a", &limited()).unwrap();
    let path = root.path().join("provider-health.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let records = state["records"].as_object_mut().unwrap();
    for record in records.values_mut() {
        record["observed_at"] = 0.into();
        record["retry_at"] = 1.into();
    }
    let bytes = serde_json::to_vec(&state).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    ledger.check("a").unwrap();
    assert_eq!(inspect(root.path()).unwrap()[0].status, "cooldown_elapsed");
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[derive(Debug, Default)]
struct LimitedText(std::sync::atomic::AtomicUsize);
#[async_trait::async_trait]
impl retold_providers::TextProvider for LimitedText {
    fn id(&self) -> &'static str {
        "a"
    }
    fn billing(&self) -> retold_providers::Billing {
        retold_providers::Billing::Subscription
    }
    fn supports_images(&self) -> bool {
        true
    }
    fn resolve_model(&self, selector: &str) -> String {
        selector.into()
    }
    async fn generate(
        &self,
        _: &retold_providers::TextRequest,
        _: &str,
        _: &tokio_util::sync::CancellationToken,
    ) -> Result<retold_providers::text::TextResponse, ProviderError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(limited())
    }
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct Reply {
    _ok: bool,
}

#[tokio::test]
async fn a_new_router_does_not_dispatch_a_recorded_limited_provider() {
    use std::sync::Arc;
    let (root, config) = fixture();
    let fake = Arc::new(LimitedText::default());
    for _ in 0..2 {
        let health = Arc::new(HealthLedger::new(root.path(), &config).unwrap());
        assert_router_blocked(Arc::clone(&fake), health).await;
    }
    assert_eq!(fake.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

async fn assert_router_blocked(
    fake: std::sync::Arc<LimitedText>,
    health: std::sync::Arc<HealthLedger>,
) {
    use std::sync::Arc;
    let routing = [(retold_core::Tier::TextFast, vec!["a:fast".parse().unwrap()])].into();
    let router = retold_providers::Router::from_providers(
        vec![(
            Arc::clone(&fake) as Arc<dyn retold_providers::TextProvider>,
            1,
        )],
        routing,
    )
    .with_health(health);
    let result = router
        .text::<Reply>(
            retold_core::Tier::TextFast,
            None,
            "fixture".into(),
            vec![],
            retold_providers::router::Policy::default(),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert!(result.is_err());
}

const FIRST_FAILURE: u32 = 1;
const SECOND_FAILURE: u32 = 2;

#[test]
fn logged_out_blocks_dispatch_and_recovers_on_success() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    ledger
        .failed(
            "a",
            &ProviderError::Unavailable("please log in with /login".into()),
        )
        .unwrap();
    assert!(matches!(
        ledger.check("a"),
        Err(ProviderError::Unavailable(_))
    ));
    assert_eq!(inspect(root.path()).unwrap()[0].status, "logged_out");

    ledger.succeeded("a").unwrap();
    assert!(ledger.check("a").is_ok());
    assert_eq!(inspect(root.path()).unwrap()[0].status, "ok");
}

#[test]
fn failing_error_increments_consecutive_failure_count() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    ledger
        .failed("a", &ProviderError::Failed("connection reset".into()))
        .unwrap();
    assert_eq!(inspect(root.path()).unwrap()[0].status, "failing");
    let first = ledger.observation("a").unwrap().unwrap();
    assert_eq!(first.consecutive_failures, FIRST_FAILURE);

    ledger
        .failed("a", &ProviderError::Timeout("timed out".into()))
        .unwrap();
    let second = ledger.observation("a").unwrap().unwrap();
    assert_eq!(second.consecutive_failures, SECOND_FAILURE);
}

fn expire_cooldown(root: &std::path::Path) {
    let path = root.join("provider-health.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for record in state["records"].as_object_mut().unwrap().values_mut() {
        record["observed_at"] = 0.into();
        record["retry_at"] = 1.into();
    }
    std::fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
}

#[test]
fn a_success_after_cooldown_expiry_replaces_the_old_limit() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    ledger.failed("a", &limited()).unwrap();
    expire_cooldown(root.path());
    ledger.succeeded("a").unwrap();
    let observation = ledger.observation("b").unwrap().unwrap();
    assert_eq!(observation.kind, "ok");
    assert_eq!(observation.retry_at, 0);
    assert_eq!(inspect(root.path()).unwrap()[0].status, "ok");
}

#[test]
fn a_login_failure_after_cooldown_expiry_blocks_alias_admission() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    ledger.failed("a", &limited()).unwrap();
    expire_cooldown(root.path());
    ledger
        .failed("a", &ProviderError::Unavailable("please log in".into()))
        .unwrap();
    assert_eq!(inspect(root.path()).unwrap()[0].status, "logged_out");
    assert!(matches!(
        ledger.check("b"),
        Err(ProviderError::Unavailable(_))
    ));
}

#[test]
fn a_failure_after_cooldown_expiry_starts_a_failure_streak() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    ledger.failed("a", &limited()).unwrap();
    expire_cooldown(root.path());
    ledger
        .failed("a", &ProviderError::Failed("synthetic failure".into()))
        .unwrap();
    assert_eq!(inspect(root.path()).unwrap()[0].status, "failing");
    assert_eq!(
        ledger
            .observation("a")
            .unwrap()
            .unwrap()
            .consecutive_failures,
        FIRST_FAILURE
    );
}

#[test]
fn an_active_cooldown_survives_success_but_does_not_hide_a_login_failure() {
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    ledger.failed("a", &limited()).unwrap();
    let deadline = ledger.cooldown_until("a").unwrap();
    ledger.succeeded("a").unwrap();
    assert_eq!(ledger.cooldown_until("b").unwrap(), deadline);
    assert_eq!(inspect(root.path()).unwrap()[0].status, "cooldown");
    ledger
        .failed("a", &ProviderError::Unavailable("please log in".into()))
        .unwrap();
    assert_eq!(ledger.cooldown_until("b").unwrap(), deadline);
    assert_eq!(inspect(root.path()).unwrap()[0].status, "logged_out");
    assert!(matches!(
        ledger.check("b"),
        Err(ProviderError::Unavailable(_))
    ));
}

#[test]
fn inspecting_an_empty_inventory_still_rejects_corrupt_health_without_repair() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("providers.toml"),
        "[providers]\n[routing]\n",
    )
    .unwrap();
    let path = root.path().join("provider-health.json");
    let bytes = b"synthetic corrupt state";
    std::fs::write(&path, bytes).unwrap();
    assert!(inspect(root.path()).is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

fn record_repeated_failures(ledger: &HealthLedger, barrier: &std::sync::Barrier) -> usize {
    const ATTEMPTS: usize = 8;
    barrier.wait();
    (0..ATTEMPTS)
        .filter(|_| {
            ledger
                .failed("a", &ProviderError::Failed("fixture".into()))
                .is_ok()
        })
        .count()
}

#[test]
fn concurrent_successful_writes_preserve_failure_counts() {
    const WORKERS: usize = 8;
    let (root, config) = fixture();
    let ledger = HealthLedger::new(root.path(), &config).unwrap();
    let barrier = std::sync::Barrier::new(WORKERS);
    let accepted: usize = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..WORKERS)
            .map(|_| scope.spawn(|| record_repeated_failures(&ledger, &barrier)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .sum()
    });
    assert!(accepted > 0);
    let count = ledger
        .observation("b")
        .unwrap()
        .unwrap()
        .consecutive_failures;
    assert_eq!(usize::try_from(count).unwrap(), accepted);
}
