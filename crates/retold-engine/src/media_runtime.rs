//! Provider routing, durable output caching and budget reservations for media work.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use retold_core::{LanguageTag, Tier};
use retold_providers::media::{
    ImageRequest, Media, MediaProvider, MediaUsage, Quote, SpeechRequest, Transcript,
};
use retold_providers::process::Limits;
use retold_providers::{Billing, ProviderConfig, ProviderError, ProvidersConfig};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::gate::{GateError, PaidCapability, PaidEvidence};
use crate::ledger::{Ledger, LedgerError, Purchase};
use crate::store::{Store, hash, stage_of};
use crate::{EngineError, fsx};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("media providers failed: {0}")]
    Unavailable(String),
    #[error("waiting for review: {0}")]
    Review(String),
    #[error("quality check failed: {0}")]
    Quality(String),
}

#[derive(Debug)]
pub struct Slot {
    pub provider: Arc<dyn MediaProvider>,
    pub permits: Arc<Semaphore>,
    pub resting: Mutex<Option<tokio::time::Instant>>,
}

#[derive(Debug)]
pub struct MediaRegistry {
    pub slots: BTreeMap<String, Arc<Slot>>,
    pub config: ProvidersConfig,
    pub health: Option<Arc<dyn retold_providers::health::Health>>,
}

impl MediaRegistry {
    fn assembled(slots: BTreeMap<String, Arc<Slot>>, config: ProvidersConfig) -> Self {
        Self {
            slots,
            config,
            health: None,
        }
    }

    #[must_use]
    pub fn from_config(
        config: &ProvidersConfig,
        key: retold_providers::router::KeyLookup<'_>,
    ) -> Self {
        let mut slots = BTreeMap::new();
        for (id, spec) in &config.providers {
            let provider: Option<Arc<dyn MediaProvider>> = match spec {
                ProviderConfig::LocalVoice {
                    espeak,
                    timeout_secs,
                    ..
                } => retold_voice::local::LocalVoice::new(
                    id,
                    espeak.clone(),
                    Duration::from_secs(*timeout_secs),
                )
                .ok()
                .map(|p| Arc::new(p) as Arc<dyn MediaProvider>),
                ProviderConfig::LocalWhisper {
                    binary,
                    model,
                    ffmpeg,
                    timeout_secs,
                    ..
                } => Some(Arc::new(retold_providers::local_whisper::LocalWhisper {
                    id: id.clone(),
                    binary: binary.clone(),
                    model: model.clone(),
                    ffmpeg: ffmpeg.clone(),
                    limits: retold_providers::process::Limits {
                        timeout: Duration::from_secs(*timeout_secs),
                        ..retold_providers::process::Limits::default()
                    },
                }) as Arc<dyn MediaProvider>),
                ProviderConfig::CodexCli {
                    binary,
                    timeout_secs,
                    config_dir,
                    ..
                } => {
                    let binary = binary.clone().or_else(|| which_binary("codex"));
                    binary
                        .and_then(|binary| {
                            retold_providers::codex_image::CodexImage::new(
                                id,
                                binary,
                                config_dir.as_deref(),
                                Limits {
                                    timeout: Duration::from_secs(*timeout_secs),
                                    env: spec.login_env(),
                                    ..Limits::default()
                                },
                            )
                            .ok()
                        })
                        .map(|p| Arc::new(p) as Arc<dyn MediaProvider>)
                }
                ProviderConfig::OpenRouter {
                    key_env,
                    base_url,
                    timeout_secs,
                    ..
                } => key(key_env)
                    .and_then(|key| {
                        retold_providers::openrouter::OpenRouter::new(
                            id,
                            base_url.clone().unwrap_or_else(|| {
                                retold_providers::openrouter::DEFAULT_BASE_URL.into()
                            }),
                            key,
                            Duration::from_secs(*timeout_secs),
                        )
                        .ok()
                    })
                    .map(|p| Arc::new(p) as Arc<dyn MediaProvider>),
                ProviderConfig::ClaudeCli { .. }
                | ProviderConfig::GeminiCli { .. }
                | ProviderConfig::LocalText { .. } => None,
            };
            if let Some(provider) = provider {
                slots.insert(
                    id.clone(),
                    Arc::new(Slot {
                        provider,
                        permits: Arc::new(Semaphore::new(spec.max_parallel().max(1))),
                        resting: Mutex::new(None),
                    }),
                );
            }
        }
        Self::assembled(slots, config.clone())
    }

    #[must_use]
    pub fn from_providers(
        config: ProvidersConfig,
        providers: Vec<(Arc<dyn MediaProvider>, usize)>,
    ) -> Self {
        let slots = providers
            .into_iter()
            .map(|(provider, n)| {
                (
                    provider.id().to_owned(),
                    Arc::new(Slot {
                        provider,
                        permits: Arc::new(Semaphore::new(n.max(1))),
                        resting: Mutex::new(None),
                    }),
                )
            })
            .collect();
        Self {
            slots,
            config,
            health: None,
        }
    }
}

fn which_binary(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join(name))
        .find(|p| p.is_file())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub path: PathBuf,
    pub content_hash: String,
    pub provider: String,
    pub model: String,
    pub usage: MediaUsage,
    pub quote: Quote,
    pub request_id: String,
}

impl Asset {
    pub fn verify(&self) -> Result<(), EngineError> {
        let bytes = std::fs::read(&self.path).map_err(|e| EngineError::io(&self.path, e))?;
        if blake3::hash(&bytes).to_hex().as_str() != self.content_hash {
            return Err(EngineError::Setup(format!(
                "saved asset {} changed; restore it before resuming",
                self.path.display()
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MediaRuntime {
    pub registry: Arc<MediaRegistry>,
    pub store: Arc<Mutex<Store>>,
    pub ledger: Arc<Ledger>,
    pub cancel: CancellationToken,
    pub allow_paid: bool,
}

impl MediaRuntime {
    pub async fn image(
        &self,
        step: &str,
        tier: Tier,
        req: &ImageRequest,
        path: &Path,
    ) -> Result<Asset, MediaError> {
        self.image_routed(step, tier, req, path, None).await
    }
    pub async fn repair_image(
        &self,
        step: &str,
        tier: Tier,
        req: &ImageRequest,
        path: &Path,
        previous: &str,
    ) -> Result<Asset, MediaError> {
        self.image_routed(step, tier, req, path, Some(previous))
            .await
    }
    /// Picks the reference-free tier when one is routed and hashes the cache identity.
    async fn image_identity(
        &self,
        step: &str,
        tier: Tier,
        req: &ImageRequest,
    ) -> Result<(Tier, String), MediaError> {
        let refs = reference_hashes(&req.references)?;
        let effective_tier = if req.references.is_empty()
            && self
                .registry
                .config
                .routing
                .contains_key(&Tier::ImageNoRefs)
        {
            Tier::ImageNoRefs
        } else {
            tier
        };
        let identity = serde_json::json!({"version":1,"revision":self.store.lock().await.revision(step),"tier":effective_tier,"request":req,"refs":refs});
        Ok((effective_tier, hash(&identity)?))
    }
    async fn image_routed(
        &self,
        step: &str,
        tier: Tier,
        req: &ImageRequest,
        path: &Path,
        previous: Option<&str>,
    ) -> Result<Asset, MediaError> {
        let (effective_tier, key) = self.image_identity(step, tier, req).await?;
        if let Some(asset) = self.cached(step, &key).await? {
            return Ok(asset);
        }
        let chain = self
            .registry
            .config
            .routing
            .get(&effective_tier)
            .ok_or_else(|| MediaError::Unavailable(format!("no route for {effective_tier}")))?;
        let mut failures = Vec::new();
        let mut budget_error = None;
        let mut routes: Vec<_> = chain.iter().collect();
        if let Some(previous) = previous {
            routes.sort_by_key(|route| route.provider == previous);
        }
        for route in routes {
            let Some(slot) = self.registry.slots.get(&route.provider) else {
                continue;
            };
            if slot.provider.billing() == Billing::PerUse && !self.allow_paid {
                continue;
            }
            let permit = self.permit(slot).await?;
            let quote = match slot
                .provider
                .image_quote(&route.selector, req, &self.cancel)
                .await
            {
                Ok(q) => q,
                Err(e) => {
                    failures.push(e.to_string());
                    continue;
                }
            };
            let base_req = hash(&(step, &key, &route.provider, &route.selector))?;
            let attempt = format!(
                "{} · {} · up to {}",
                route.provider, route.selector, quote.ceiling
            );
            self.begin_attempt(step, attempt).await?;
            let purchase = Purchase {
                logical: &base_req,
                stage: stage_of(step),
                provider: slot.provider.id(),
                capability: PaidCapability::Image,
                ceiling: quote.ceiling,
            };
            let request = match self.reserve(slot, &purchase) {
                Ok(id) => id,
                Err(error) if over_limit(&error) => {
                    budget_error = Some(error);
                    continue;
                }
                Err(error) => return Err(error),
            };
            let result = slot
                .provider
                .image(&route.selector, req, &quote, &self.cancel)
                .await;
            drop(permit);
            match result {
                Ok(media) => {
                    return self
                        .persist(
                            step,
                            &key,
                            path,
                            media,
                            slot,
                            &route.selector,
                            quote,
                            &request,
                        )
                        .await;
                }
                Err(e) => {
                    self.failed(step, slot, &request, &e).await?;
                    failures.push(format!("{}: {e}", route.provider));
                }
            }
        }
        if let Some(error) = budget_error {
            return Err(error);
        }
        Err(MediaError::Unavailable(failures.join("; ")))
    }

    pub async fn speech(
        &self,
        step: &str,
        provider: &str,
        model: &str,
        req: &SpeechRequest,
        path: &Path,
    ) -> Result<Asset, MediaError> {
        let key = hash(
            &serde_json::json!({"version":if model == "kokoro" {5} else if model == "chatterbox-hi" {3} else {1},"revision":self.store.lock().await.revision(step),"provider":provider,"model":model,"request":req}),
        )?;
        if let Some(asset) = self.cached(step, &key).await? {
            return Ok(asset);
        }
        let slot = self.registry.slots.get(provider).ok_or_else(|| {
            MediaError::Unavailable(format!("voice provider `{provider}` is unavailable"))
        })?;
        self.require_paid(slot)?;
        let _permit = self.permit(slot).await?;
        let quote = slot.provider.speech_quote(model, req, &self.cancel).await?;
        let base_req = hash(&(step, &key, provider, model))?;
        self.store.lock().await.begin(step)?;
        let request = self.reserve(
            slot,
            &Purchase {
                logical: &base_req,
                stage: stage_of(step),
                provider: slot.provider.id(),
                capability: PaidCapability::Speech,
                ceiling: quote.ceiling,
            },
        )?;
        let media = match slot.provider.speech(model, req, &self.cancel).await {
            Ok(m) => m,
            Err(e) => {
                self.failed(step, slot, &request, &e).await?;
                return Err(e.into());
            }
        };
        let path = if media.value.starts_with(b"RIFF") {
            path.with_extension("wav")
        } else {
            path.into()
        };
        self.persist(step, &key, &path, media, slot, model, quote, &request)
            .await
    }

    /// Inspect a committed transcript without dispatching, quoting, or reserving a request.
    pub async fn cached_transcript(
        &self,
        step: &str,
        model: &str,
        audio: &Asset,
        lang: &LanguageTag,
    ) -> Result<Option<Transcript>, MediaError> {
        let key = hash(&(
            1,
            self.store.lock().await.revision(step),
            model,
            &audio.content_hash,
            lang,
        ))?;
        let Some(asset) = self.cached(step, &key).await? else {
            return Ok(None);
        };
        let transcript: Transcript = serde_json::from_slice(
            &std::fs::read(&asset.path).map_err(|e| EngineError::io(&asset.path, e))?,
        )
        .map_err(|e| EngineError::Setup(e.to_string()))?;
        transcript.validate()?;
        Ok(Some(transcript))
    }

    /// Read-only identity for local ASR weights; cloud cache/ledger identities
    /// remain unchanged so existing paid receipts are never replayed.
    #[must_use]
    pub fn local_transcription_identity(&self, provider: &str) -> Option<serde_json::Value> {
        let ProviderConfig::LocalWhisper { model, binary, .. } =
            self.registry.config.providers.get(provider)?
        else {
            return None;
        };
        let metadata = std::fs::metadata(model).ok();
        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos().to_string());
        Some(
            serde_json::json!({"provider":provider,"model_file":model,"binary":binary,"bytes":metadata.as_ref().map(std::fs::Metadata::len),"modified":modified}),
        )
    }
    pub async fn transcribe(
        &self,
        step: &str,
        provider: &str,
        model: &str,
        audio: &Asset,
        lang: &LanguageTag,
        path: &Path,
    ) -> Result<Transcript, MediaError> {
        let key = hash(&(
            1,
            self.store.lock().await.revision(step),
            model,
            &audio.content_hash,
            lang,
        ))?;
        let key = if let Some(backend) = self.local_transcription_identity(provider) {
            hash(&(key, backend))?
        } else {
            key
        };
        if let Some(asset) = self.cached(step, &key).await? {
            let transcript: Transcript = serde_json::from_slice(
                &std::fs::read(&asset.path).map_err(|e| EngineError::io(&asset.path, e))?,
            )
            .map_err(|e| EngineError::Setup(e.to_string()))?;
            transcript.validate()?;
            return Ok(transcript);
        }
        let slot = self.registry.slots.get(provider).ok_or_else(|| {
            MediaError::Unavailable(format!(
                "transcription provider `{provider}` is unavailable"
            ))
        })?;
        self.require_paid(slot)?;
        let _permit = self.permit(slot).await?;
        let bytes = std::fs::read(&audio.path).map_err(|e| EngineError::io(&audio.path, e))?;
        let duration = retold_providers::media::audio_duration(&bytes)?;
        let format = if bytes.starts_with(b"RIFF") {
            "wav"
        } else {
            "mp3"
        };
        let quote = slot
            .provider
            .transcript_quote(model, duration, &self.cancel)
            .await?;
        let base_req = hash(&(step, &key, provider, model))?;
        self.store.lock().await.begin(step)?;
        let request = self.reserve(
            slot,
            &Purchase {
                logical: &base_req,
                stage: stage_of(step),
                provider: slot.provider.id(),
                capability: PaidCapability::Transcription,
                ceiling: quote.ceiling,
            },
        )?;
        let media = match slot
            .provider
            .transcribe(model, &bytes, format, lang, &self.cancel)
            .await
        {
            Ok(m) => m,
            Err(e) => {
                self.failed(step, slot, &request, &e).await?;
                return Err(e.into());
            }
        };
        let transcript = media.value;
        let value = serde_json::to_vec_pretty(&transcript)
            .map_err(|e| EngineError::Setup(e.to_string()))?;
        self.persist(
            step,
            &key,
            path,
            Media {
                value,
                usage: media.usage,
            },
            slot,
            model,
            quote,
            &request,
        )
        .await?;
        // A charged, parseable response must remain recoverable even if its word
        // timestamps fail validation. Never discard it and invite a paid replay.
        if let Err(error) = transcript.validate() {
            self.store
                .lock()
                .await
                .task_stopped(step, &error.to_string())?;
            return Err(error.into());
        }
        Ok(transcript)
    }

    fn require_paid(&self, slot: &Slot) -> Result<(), MediaError> {
        if slot.provider.billing() == Billing::PerUse && !self.allow_paid {
            return Err(MediaError::Unavailable(
                "paid media needs --allow-paid and sufficient --budget".into(),
            ));
        }
        Ok(())
    }
    async fn begin_attempt(&self, step: &str, attempt: String) -> Result<(), MediaError> {
        let mut store = self.store.lock().await;
        store.begin(step)?;
        store.emit(step, crate::events::Status::Running, None, Some(attempt))?;
        Ok(())
    }
    /// The last local step before a provider call: per-use work crosses the dispatch boundary.
    fn reserve(&self, slot: &Slot, purchase: &Purchase<'_>) -> Result<String, MediaError> {
        if slot.provider.billing() == Billing::PerUse {
            Ok(self.ledger.begin_paid(purchase)?)
        } else {
            Ok(purchase.logical.to_owned())
        }
    }
    async fn cached(&self, step: &str, key: &str) -> Result<Option<Asset>, MediaError> {
        let mut asset: Option<Asset> = self.store.lock().await.cached(step, key)?;
        if let Some(asset) = &mut asset {
            asset.verify()?;
            if let Some(charge) = self.ledger.entries()?.get(&asset.request_id) {
                match charge {
                    crate::ledger::Charge::Exact { cost } => asset.usage.cost = Some(*cost),
                    crate::ledger::Charge::Released => {
                        asset.usage.cost = Some(retold_core::Usd::ZERO);
                    }
                    _ => asset.usage.cost = self.recover_settlement(asset).await?,
                }
            }
        }
        Ok(asset)
    }
    /// A committed result whose settlement was interrupted settles now, with its saved bytes.
    async fn recover_settlement(
        &self,
        asset: &Asset,
    ) -> Result<Option<retold_core::Usd>, MediaError> {
        let Asset {
            provider,
            usage,
            content_hash,
            request_id,
            ..
        } = asset;
        let mut cost = usage.cost;
        if cost.is_none()
            && let Some(id) = &usage.generation
            && let Some(slot) = self.registry.slots.get(provider)
        {
            cost = slot.provider.recorded_cost(id, &self.cancel).await?;
        }
        let evidence = PaidEvidence {
            artifact_hash: content_hash,
            cost,
        };
        self.ledger
            .settle_with_evidence(request_id, usage.generation.clone(), &evidence)?;
        Ok(cost)
    }
    async fn permit(&self, slot: &Slot) -> Result<tokio::sync::OwnedSemaphorePermit, MediaError> {
        let rest = *slot.resting.lock().await;
        if let Some(until) = rest {
            tokio::select! { () = self.cancel.cancelled() => return Err(ProviderError::Cancelled.into()), () = tokio::time::sleep_until(until) => {} }
        }
        let permit = tokio::select! {
            () = self.cancel.cancelled() => return Err(ProviderError::Cancelled.into()),
            result = Arc::clone(&slot.permits).acquire_owned() => result.map_err(|_| ProviderError::Cancelled)?,
        };
        if let Some(health) = &self.registry.health {
            health.check(slot.provider.id())?;
        }
        Ok(permit)
    }
    async fn failed(
        &self,
        step: &str,
        slot: &Slot,
        request: &str,
        error: &ProviderError,
    ) -> Result<(), MediaError> {
        if let Some(health) = &self.registry.health {
            health.failed(slot.provider.id(), error)?;
        }
        self.store
            .lock()
            .await
            .task_stopped(step, &error.to_string())?;
        if let ProviderError::RateLimited { retry_after, .. } = error {
            *slot.resting.lock().await =
                Some(tokio::time::Instant::now() + retry_after.unwrap_or(Duration::from_secs(120)));
        }
        if slot.provider.billing() == Billing::PerUse && matches!(error, ProviderError::Unbilled(_))
        {
            self.ledger.record_failure(request, &error.to_string())?;
            self.ledger.reconcile(request, retold_core::Usd::ZERO)?;
            return Ok(());
        }
        if slot.provider.billing() == Billing::PerUse {
            self.ledger.record_failure(request, &error.to_string())?;
            // Even a failed response may have been billed upstream. Never silently
            // release a hold or try another paid provider after an ambiguous failure.
            return Err(LedgerError::Uncertain(request.into()).into());
        }
        if matches!(error, ProviderError::Cancelled) {
            return Err(error.clone().into());
        }
        Ok(())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "split pending (Phase 0 step 3c); counted by the clean-code baseline"
    )]
    async fn persist(
        &self,
        step: &str,
        key: &str,
        path: &Path,
        media: Media<Vec<u8>>,
        slot: &Slot,
        model: &str,
        quote: Quote,
        request: &str,
    ) -> Result<Asset, MediaError> {
        let asset = Asset {
            path: immutable_path(path, key),
            content_hash: blake3::hash(&media.value).to_hex().to_string(),
            provider: slot.provider.id().into(),
            model: model.into(),
            usage: media.usage,
            quote,
            request_id: request.into(),
        };
        fsx::write_atomic(&asset.path, &media.value)?;
        // A receipt per image puts media spend in the run's cost report next to text calls.
        let receipt = crate::store::Receipt {
            provider: asset.provider.clone(),
            model: asset.model.clone(),
            input_tokens: 0,
            output_tokens: 0,
            cost: match (asset.usage.cost, slot.provider.billing()) {
                (Some(cost), _) => cost,
                (None, Billing::PerUse) => asset.quote.ceiling,
                (None, _) => retold_core::Usd::ZERO,
            },
        };
        self.store
            .lock()
            .await
            .commit(step, key, &asset, Some(receipt))?;
        if slot.provider.billing() == Billing::PerUse {
            let evidence = PaidEvidence {
                artifact_hash: &asset.content_hash,
                cost: asset.usage.cost,
            };
            self.ledger
                .settle_with_evidence(request, asset.usage.generation.clone(), &evidence)?;
        }
        Ok(asset)
    }
}

/// Another allowed route may still fit the per-run or account ceilings.
const fn over_limit(error: &MediaError) -> bool {
    matches!(
        error,
        MediaError::Ledger(LedgerError::Budget { .. } | LedgerError::Gate(GateError::Refused(_)))
    )
}

fn immutable_path(path: &Path, key: &str) -> PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("asset");
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("bin");
    path.with_file_name(format!("{stem}-{key}.{extension}"))
}

fn reference_hashes(references: &[std::path::PathBuf]) -> Result<Vec<String>, MediaError> {
    references
        .iter()
        .map(|p| {
            std::fs::read(p)
                .map(|b| blake3::hash(&b).to_hex().to_string())
                .map_err(|e| EngineError::io(p, e))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}
