//! The router: picks the first healthy provider in a tier's chain, enforces per-provider
//! concurrency, rests rate-limited providers, and refuses paid fallbacks unless allowed.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use indexmap::IndexMap;
use retold_core::{Capability, Tier};
use schemars::JsonSchema;
use secrecy::SecretString;
use serde::de::DeserializeOwned;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::claude_cli::ClaudeCli;
use crate::codex_cli::CodexCli;
use crate::config::{ProviderConfig, ProvidersConfig, RouteSpec};
use crate::error::ProviderError;
use crate::openrouter::{DEFAULT_BASE_URL, OpenRouter};
use crate::process::Limits;
use crate::text::{Billing, TextProvider, TextRequest, Usage, strict_schema};

const DEFAULT_COOLDOWN: Duration = Duration::from_secs(120);
/// Same-provider retries for transient failures on subscription routes.
pub const TRANSIENT_RETRIES: u32 = 1;

const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_millis(1_500);

/// Exponential backoff plus up to a third of the base as jitter, from the clock's sub-second noise.
fn backoff(first: Duration, attempt: u32) -> Duration {
    let base = first.saturating_mul(2_u32.saturating_pow(attempt.saturating_sub(1)));
    let noise = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    base + base.mul_f64(f64::from(noise % 1_000) / 3_000.0)
}

/// Something worth showing in the progress view.
#[derive(Debug, Clone)]
pub enum RouterEvent {
    /// `from` failed with `reason`; trying `to` next.
    FellBack {
        tier: Tier,
        from: String,
        to: String,
        reason: String,
    },
    /// A provider is resting until it can be used again.
    Cooldown { provider: String, until: Instant },
}

type Observer = Arc<dyn Fn(RouterEvent) + Send + Sync>;

/// Run-level rules the router must follow.
#[derive(Debug, Clone, Copy, Default)]
pub struct Policy {
    /// Whether pay-per-use providers may be used (the engine checks the budget first).
    pub allow_paid: bool,
    /// The request needs live web search; providers without it are skipped.
    pub web: bool,
}

/// One provider tried for a request.
#[derive(Debug, Clone)]
pub struct Attempt {
    pub provider: String,
    pub model: String,
    pub error: ProviderError,
}

/// A typed answer and where it came from.
#[derive(Debug, Clone)]
pub struct Answer<T> {
    pub value: T,
    pub provider: String,
    pub model: String,
    pub billing: Billing,
    pub usage: Usage,
    /// Earlier providers that failed on this request.
    pub failed: Vec<Attempt>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RouteError {
    #[error("no provider is set up for `{0}`; run `retold doctor`")]
    NoRoute(Tier),
    #[error("every provider for `{tier}` failed:\n{}", list(attempts))]
    AllFailed { tier: Tier, attempts: Vec<Attempt> },
    #[error("cancelled")]
    Cancelled,
}

fn list(attempts: &[Attempt]) -> String {
    attempts
        .iter()
        .map(|a| format!("  • {} ({}): {}", a.provider, a.model, a.error))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug)]
struct Slot {
    provider: Arc<dyn TextProvider>,
    permits: Arc<Semaphore>,
    resting_until: Mutex<Option<Instant>>,
}

impl Slot {
    fn resting(&self) -> Option<Instant> {
        let guard = self.resting_until.lock().ok()?;
        guard.filter(|t| *t > Instant::now())
    }

    fn rest(&self, d: Duration) -> Instant {
        let until = Instant::now() + d;
        if let Ok(mut g) = self.resting_until.lock() {
            *g = Some(until);
        }
        until
    }
}

pub struct Router {
    slots: IndexMap<String, Slot>,
    routing: IndexMap<Tier, Vec<RouteSpec>>,
    /// Providers that could not be set up, with the reason.
    unavailable: IndexMap<String, String>,
    observer: Option<Observer>,
    /// First backoff before a same-provider retry; doubles per attempt.
    retry_backoff: Duration,
    health: Option<Arc<dyn crate::health::Health>>,
}

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Router")
            .field("providers", &self.slots.keys().collect::<Vec<_>>())
            .field("unavailable", &self.unavailable)
            .finish_non_exhaustive()
    }
}

/// Reads a secret from the environment.
pub type KeyLookup<'a> = &'a dyn Fn(&str) -> Option<SecretString>;

impl Router {
    /// Builds every configured provider that can run here; the rest are listed in
    /// [`Router::unavailable`].
    #[must_use]
    pub fn from_config(cfg: &ProvidersConfig, key: KeyLookup<'_>) -> Self {
        let mut slots = IndexMap::new();
        let mut unavailable = IndexMap::new();
        for (id, pc) in &cfg.providers {
            match build(id, pc, key) {
                Ok(p) => {
                    slots.insert(
                        id.clone(),
                        Slot {
                            provider: p,
                            permits: Arc::new(Semaphore::new(pc.max_parallel().max(1))),
                            resting_until: Mutex::new(None),
                        },
                    );
                }
                Err(reason) => {
                    unavailable.insert(id.clone(), reason);
                }
            }
        }
        Self {
            slots,
            routing: cfg.routing.clone(),
            unavailable,
            observer: None,
            retry_backoff: DEFAULT_RETRY_BACKOFF,
            health: None,
        }
    }

    #[must_use]
    pub fn with_health(mut self, health: Arc<dyn crate::health::Health>) -> Self {
        self.health = Some(health);
        self
    }

    /// Share a concurrency limit with another capability adapter for the same provider.
    pub fn share_permits(&mut self, id: &str, permits: Arc<Semaphore>) {
        if let Some(slot) = self.slots.get_mut(id) {
            slot.permits = permits;
        }
    }

    /// Builds a router from ready-made providers (tests and fakes).
    #[must_use]
    pub fn from_providers(
        providers: Vec<(Arc<dyn TextProvider>, usize)>,
        routing: IndexMap<Tier, Vec<RouteSpec>>,
    ) -> Self {
        let slots = providers
            .into_iter()
            .map(|(p, n)| {
                (
                    p.id().to_owned(),
                    Slot {
                        provider: p,
                        permits: Arc::new(Semaphore::new(n.max(1))),
                        resting_until: Mutex::new(None),
                    },
                )
            })
            .collect();
        Self {
            slots,
            routing,
            unavailable: IndexMap::new(),
            observer: None,
            retry_backoff: DEFAULT_RETRY_BACKOFF,
            health: None,
        }
    }

    /// Sets the first retry backoff (tests use zero to stay fast).
    #[must_use]
    pub const fn with_retry_backoff(mut self, backoff: Duration) -> Self {
        self.retry_backoff = backoff;
        self
    }

    #[must_use]
    pub fn with_observer(mut self, f: impl Fn(RouterEvent) + Send + Sync + 'static) -> Self {
        self.observer = Some(Arc::new(f));
        self
    }

    #[must_use]
    pub const fn unavailable(&self) -> &IndexMap<String, String> {
        &self.unavailable
    }

    /// `tier.for_repair(pass)` when that tier has a usable route here, otherwise `tier`.
    #[must_use]
    pub fn repair_tier(&self, tier: Tier, pass: u32) -> Tier {
        let up = tier.for_repair(pass);
        let routed = self
            .routing
            .get(&up)
            .is_some_and(|chain| chain.iter().any(|r| self.slots.contains_key(&r.provider)));
        if routed { up } else { tier }
    }

    #[must_use]
    pub fn text_provider(&self, id: &str) -> Option<Arc<dyn TextProvider>> {
        self.slots.get(id).map(|s| Arc::clone(&s.provider))
    }

    fn emit(&self, e: RouterEvent) {
        if let Some(o) = &self.observer {
            o(e);
        }
    }

    /// One provider call, decoded into `T`. Subscription calls cost nothing extra, so a
    /// transient failure gets [`TRANSIENT_RETRIES`] quick retries before the chain moves on.
    /// Paid calls never retry here: a lost reply may still have been billed.
    async fn generate<T: DeserializeOwned>(
        &self,
        slot: &Slot,
        req: &TextRequest,
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<(T, Usage), ProviderError> {
        let provider = &slot.provider;
        let retries = if provider.billing() == Billing::Subscription {
            TRANSIENT_RETRIES
        } else {
            0
        };
        let mut attempt = 0;
        loop {
            let permit = tokio::select! {
                p = Arc::clone(&slot.permits).acquire_owned() => p.map_err(|_| ProviderError::Cancelled)?,
                () = cancel.cancelled() => return Err(ProviderError::Cancelled),
            };
            if let Some(health) = &self.health {
                health.check(provider.id())?;
            }
            let result = provider.generate(req, model, cancel).await;
            if let (Some(health), Err(error)) = (&self.health, &result) {
                health.failed(provider.id(), error)?;
            }
            drop(permit);
            let error = match result {
                Ok(resp) => match serde_json::from_value::<T>(resp.value) {
                    Ok(value) => return Ok((value, resp.usage)),
                    Err(e) => ProviderError::Malformed(format!(
                        "reply does not match the expected shape: {e}"
                    )),
                },
                Err(e) => e,
            };
            if attempt >= retries || !error.is_transient() {
                return Err(error);
            }
            attempt += 1;
            tokio::select! {
                () = tokio::time::sleep(backoff(self.retry_backoff, attempt)) => {}
                () = cancel.cancelled() => return Err(ProviderError::Cancelled),
            }
        }
    }

    /// Why this slot cannot take the request right now, if anything.
    fn blocked(slot: &Slot, req: &TextRequest, policy: Policy) -> Option<ProviderError> {
        let p = &slot.provider;
        if p.billing() == Billing::PerUse && !policy.allow_paid {
            Some(ProviderError::Unavailable(
                "paid provider, not allowed for this run".into(),
            ))
        } else if !req.images.is_empty() && !p.supports_images() {
            Some(ProviderError::Unsupported("no image input".into()))
        } else if slot.resting().is_some() {
            Some(ProviderError::RateLimited {
                message: "resting after a rate limit".into(),
                retry_after: None,
            })
        } else {
            None
        }
    }

    /// Rests a rate-limited or unavailable provider and announces the fallback, if any.
    fn after_failure(
        &self,
        tier: Tier,
        slot: &Slot,
        provider: &str,
        next: Option<&RouteSpec>,
        error: &ProviderError,
    ) {
        if error.needs_cooldown() {
            let wait = match error {
                ProviderError::RateLimited {
                    retry_after: Some(d),
                    ..
                } => *d,
                _ => DEFAULT_COOLDOWN,
            };
            let until = slot.rest(wait);
            self.emit(RouterEvent::Cooldown {
                provider: provider.to_owned(),
                until,
            });
        }
        if let Some(next) = next {
            self.emit(RouterEvent::FellBack {
                tier,
                from: provider.to_owned(),
                to: next.provider.clone(),
                reason: error.to_string(),
            });
        }
    }

    /// Asks the tier's chain for a `T`, trying providers in order.
    pub async fn text<T: DeserializeOwned + JsonSchema>(
        &self,
        tier: Tier,
        system: Option<String>,
        prompt: String,
        images: Vec<PathBuf>,
        policy: Policy,
        cancel: &CancellationToken,
    ) -> Result<Answer<T>, RouteError> {
        debug_assert!(matches!(
            tier.capability(),
            Capability::Text | Capability::Vision
        ));
        let chain = self
            .routing
            .get(&tier)
            .filter(|c| !c.is_empty())
            .ok_or(RouteError::NoRoute(tier))?;
        let req = TextRequest {
            system,
            prompt,
            schema: strict_schema::<T>(),
            images,
            web: policy.web,
        };
        let mut failed: Vec<Attempt> = Vec::new();

        for (i, route) in chain.iter().enumerate() {
            let skip = |failed: &mut Vec<Attempt>, model: String, error: ProviderError| {
                failed.push(Attempt {
                    provider: route.provider.clone(),
                    model,
                    error,
                });
            };
            let Some(slot) = self.slots.get(&route.provider) else {
                let reason = self
                    .unavailable
                    .get(&route.provider)
                    .cloned()
                    .unwrap_or_else(|| "not configured".into());
                skip(
                    &mut failed,
                    route.selector.clone(),
                    ProviderError::Unavailable(reason),
                );
                continue;
            };
            let p = &slot.provider;
            let model = p.resolve_model(&route.selector);
            if tier == Tier::TextLocal && p.billing() != Billing::Local {
                skip(
                    &mut failed,
                    model,
                    ProviderError::Unsupported(
                        "text.local forbids cloud providers and fallbacks".into(),
                    ),
                );
                continue;
            }
            if let Some(error) = Self::blocked(slot, &req, policy) {
                skip(&mut failed, model, error);
                continue;
            }
            let error = match self.generate::<T>(slot, &req, &model, cancel).await {
                Ok((value, usage)) => {
                    return Ok(Answer {
                        value,
                        provider: route.provider.clone(),
                        model,
                        billing: p.billing(),
                        usage,
                        failed,
                    });
                }
                Err(ProviderError::Cancelled) => return Err(RouteError::Cancelled),
                Err(e) => e,
            };
            self.after_failure(tier, slot, &route.provider, chain.get(i + 1), &error);
            failed.push(Attempt {
                provider: route.provider.clone(),
                model,
                error,
            });
        }
        Err(RouteError::AllFailed {
            tier,
            attempts: failed,
        })
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "adapter construction dispatches each configured provider explicitly"
)]
fn build(
    id: &str,
    pc: &ProviderConfig,
    key: KeyLookup<'_>,
) -> Result<Arc<dyn TextProvider>, String> {
    let find = |binary: &Option<PathBuf>, name: &str| -> Result<PathBuf, String> {
        match binary {
            Some(p) if p.exists() => Ok(p.clone()),
            Some(p) => Err(format!("{} does not exist", p.display())),
            None => {
                which::which(name).map_err(|_| format!("`{name}` is not installed (not on PATH)"))
            }
        }
    };
    Ok(match pc {
        ProviderConfig::LocalText {
            binary,
            models,
            timeout_secs,
            max_tokens,
            context_size,
            ..
        } => Arc::new(crate::local_text::LocalText::new(
            id,
            find(binary, "llama-completion")?,
            models,
            Limits {
                timeout: Duration::from_secs(*timeout_secs),
                ..Limits::default()
            },
            *max_tokens,
            *context_size,
        )?),
        ProviderConfig::LocalWhisper { .. } | ProviderConfig::LocalVoice { .. } => {
            return Err("local media provider has no text capability".into());
        }
        ProviderConfig::ClaudeCli {
            binary,
            tiers,
            timeout_secs,
            ..
        } => Arc::new(ClaudeCli {
            id: id.to_owned(),
            binary: find(binary, "claude")?,
            tiers: tiers.clone(),
            limits: Limits {
                timeout: Duration::from_secs(*timeout_secs),
                env: pc.login_env(),
                ..Limits::default()
            },
        }),
        ProviderConfig::GeminiCli {
            binary,
            tiers,
            timeout_secs,
            ..
        } => Arc::new(crate::gemini_cli::GeminiCli {
            id: id.into(),
            binary: find(binary, "gemini")?,
            tiers: tiers.clone(),
            limits: Limits {
                timeout: Duration::from_secs(*timeout_secs),
                ..Limits::default()
            },
        }),
        ProviderConfig::CodexCli {
            binary,
            tiers,
            timeout_secs,
            ..
        } => Arc::new(CodexCli {
            id: id.to_owned(),
            binary: find(binary, "codex")?,
            tiers: tiers.clone(),
            limits: Limits {
                timeout: Duration::from_secs(*timeout_secs),
                env: pc.login_env(),
                ..Limits::default()
            },
        }),
        ProviderConfig::OpenRouter {
            key_env,
            base_url,
            timeout_secs,
            ..
        } => {
            let k = key(key_env).ok_or_else(|| {
                format!("${key_env} is not set (add it to your environment or .env)")
            })?;
            Arc::new(
                OpenRouter::new(
                    id,
                    base_url
                        .clone()
                        .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned()),
                    k,
                    Duration::from_secs(*timeout_secs),
                )
                .map_err(|e| e.to_string())?,
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use serde::Deserialize;

    use super::*;
    use crate::text::TextResponse;

    #[derive(Debug)]
    struct Fake {
        id: &'static str,
        billing: Billing,
        reply: Result<serde_json::Value, ProviderError>,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl TextProvider for Fake {
        fn id(&self) -> &str {
            self.id
        }
        fn billing(&self) -> Billing {
            self.billing
        }
        fn supports_images(&self) -> bool {
            bool::default()
        }
        fn resolve_model(&self, s: &str) -> String {
            s.to_owned()
        }
        async fn generate(
            &self,
            _: &TextRequest,
            _: &str,
            _: &CancellationToken,
        ) -> Result<TextResponse, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.reply.clone().map(|value| TextResponse {
                value,
                usage: Usage::default(),
            })
        }
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    struct Pong {
        ok: bool,
    }

    fn fake(
        id: &'static str,
        billing: Billing,
        reply: Result<serde_json::Value, ProviderError>,
    ) -> Arc<Fake> {
        Arc::new(Fake {
            id,
            billing,
            reply,
            calls: AtomicUsize::new(0),
        })
    }

    fn router(fakes: &[Arc<Fake>]) -> Router {
        let routes = fakes
            .iter()
            .map(|f| format!("{}:m", f.id).parse().unwrap())
            .collect();
        let providers = fakes
            .iter()
            .map(|f| (Arc::clone(f) as Arc<dyn TextProvider>, 1))
            .collect();
        Router::from_providers(
            providers,
            std::iter::once((Tier::TextFast, routes)).collect(),
        )
    }

    async fn ask(r: &Router, policy: Policy) -> Result<Answer<Pong>, RouteError> {
        r.text::<Pong>(
            Tier::TextFast,
            None,
            "ping".into(),
            vec![],
            policy,
            &CancellationToken::new(),
        )
        .await
    }

    #[tokio::test]
    async fn local_tier_never_dispatches_subscription_or_paid_fallbacks() {
        let cloud = fake(
            "cloud",
            Billing::Subscription,
            Ok(serde_json::json!({"ok":true})),
        );
        let paid = fake("paid", Billing::PerUse, Ok(serde_json::json!({"ok":true})));
        let local = fake("local", Billing::Local, Ok(serde_json::json!({"ok":true})));
        let mut r = router(&[Arc::clone(&cloud), Arc::clone(&paid), Arc::clone(&local)]);
        let chain = r.routing.shift_remove(&Tier::TextFast).unwrap();
        r.routing.insert(Tier::TextLocal, chain);
        let answer = r
            .text::<Pong>(
                Tier::TextLocal,
                None,
                "ping".into(),
                vec![],
                Policy {
                    allow_paid: true,
                    web: false,
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(answer.provider, "local");
        assert!(answer.value.ok);
        assert_eq!(cloud.calls.load(Ordering::SeqCst), 0);
        assert_eq!(paid.calls.load(Ordering::SeqCst), 0);
        assert_eq!(local.calls.load(Ordering::SeqCst), 1);
        r.routing.get_mut(&Tier::TextLocal).unwrap().pop();
        assert!(
            r.text::<Pong>(
                Tier::TextLocal,
                None,
                "ping".into(),
                vec![],
                Policy {
                    allow_paid: true,
                    web: false
                },
                &CancellationToken::new()
            )
            .await
            .is_err()
        );
        assert_eq!(cloud.calls.load(Ordering::SeqCst), 0);
        assert_eq!(paid.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn falls_back_and_rests_rate_limited_providers() {
        let limited = fake(
            "a",
            Billing::Subscription,
            Err(ProviderError::RateLimited {
                message: "x".into(),
                retry_after: None,
            }),
        );
        let good = fake(
            "b",
            Billing::Subscription,
            Ok(serde_json::json!({"ok": true})),
        );
        let events = Arc::new(Mutex::new(Vec::new()));
        let ev = Arc::clone(&events);
        let r = router(&[Arc::clone(&limited), Arc::clone(&good)])
            .with_observer(move |e| ev.lock().unwrap().push(format!("{e:?}")));

        let a = ask(&r, Policy::default()).await.unwrap();
        assert!(a.value.ok);
        assert_eq!(a.provider, "b");
        assert_eq!(a.failed.len(), 1);
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.contains("FellBack"))
        );

        // The rate-limited provider is resting, so it is not called again.
        ask(&r, Policy::default()).await.unwrap();
        assert_eq!(limited.calls.load(Ordering::SeqCst), 1);
        assert_eq!(good.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn transient_subscription_failures_retry_once_on_the_same_provider() {
        let flaky = fake(
            "f",
            Billing::Subscription,
            Err(ProviderError::Timeout("slow".into())),
        );
        let r = router(&[Arc::clone(&flaky)]);
        assert!(ask(&r, Policy::default()).await.is_err());
        assert_eq!(
            flaky.calls.load(Ordering::SeqCst),
            1 + TRANSIENT_RETRIES as usize
        );
    }

    #[tokio::test(start_paused = true)]
    async fn paid_and_non_transient_failures_never_retry() {
        let paid = fake(
            "p",
            Billing::PerUse,
            Err(ProviderError::Failed("500".into())),
        );
        let refused = fake(
            "r",
            Billing::Subscription,
            Err(ProviderError::Refused("no".into())),
        );
        let r = router(&[Arc::clone(&paid), Arc::clone(&refused)]);
        let policy = Policy {
            allow_paid: true,
            web: false,
        };
        assert!(ask(&r, policy).await.is_err());
        assert_eq!(paid.calls.load(Ordering::SeqCst), 1);
        assert_eq!(refused.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn paid_providers_need_permission() {
        let paid = fake("p", Billing::PerUse, Ok(serde_json::json!({"ok": true})));
        let r = router(&[Arc::clone(&paid)]);
        assert!(matches!(
            ask(&r, Policy::default()).await,
            Err(RouteError::AllFailed { .. })
        ));
        assert_eq!(paid.calls.load(Ordering::SeqCst), 0);
        let policy = Policy {
            allow_paid: true,
            web: false,
        };
        assert!(ask(&r, policy).await.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn wrong_shape_falls_back() {
        let wrong = fake(
            "w",
            Billing::Subscription,
            Ok(serde_json::json!({"nope": 1})),
        );
        let good = fake(
            "g",
            Billing::Subscription,
            Ok(serde_json::json!({"ok": false})),
        );
        let a = ask(&router(&[wrong, good]), Policy::default())
            .await
            .unwrap();
        assert_eq!(a.provider, "g");
        assert!(matches!(a.failed[0].error, ProviderError::Malformed(_)));
    }

    #[tokio::test]
    async fn no_route_is_explained() {
        let r = Router::from_providers(vec![], IndexMap::new());
        let policy = Policy {
            allow_paid: true,
            web: false,
        };
        assert!(matches!(
            ask(&r, policy).await,
            Err(RouteError::NoRoute(Tier::TextFast))
        ));
    }
}
