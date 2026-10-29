//! Paid text fallback behind a durable worst-case reservation.

use retold_core::Tier;
use retold_providers::{Billing, ProviderError, Router, TextRequest};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::EngineError;
use crate::gate::{PaidCapability, PaidEvidence};
use crate::ledger::{Ledger, LedgerError, Purchase};
use crate::media_runtime::{MediaError, MediaRegistry};
use crate::store::{Receipt, hash, stage_of};

#[derive(Debug, Clone)]
pub struct PaidText {
    pub registry: Arc<MediaRegistry>,
    pub ledger: Arc<Ledger>,
    pub allow_paid: bool,
}

#[derive(Debug)]
pub struct Settlement {
    pub id: String,
    pub cost: Option<retold_core::Usd>,
    pub artifact_hash: String,
}

impl PaidText {
    pub async fn generate<T: DeserializeOwned>(
        &self,
        router: &Router,
        tier: Tier,
        identity: (&str, &str),
        req: &TextRequest,
        cancel: &CancellationToken,
    ) -> Result<(T, Receipt, Settlement), MediaError> {
        if tier == Tier::TextLocal {
            return Err(MediaError::Unavailable(
                "text.local forbids paid fallback".into(),
            ));
        }
        if !self.allow_paid {
            return Err(MediaError::Unavailable(
                "paid text needs --allow-paid".into(),
            ));
        }
        let chain = self
            .registry
            .config
            .routing
            .get(&tier)
            .ok_or_else(|| MediaError::Unavailable(format!("no route for {tier}")))?;
        for route in chain {
            let Some(provider) = router.text_provider(&route.provider) else {
                continue;
            };
            if provider.billing() != Billing::PerUse {
                continue;
            }
            let Some(slot) = self.registry.slots.get(&route.provider) else {
                continue;
            };
            let permit = tokio::select! {
                ()=cancel.cancelled()=>return Err(ProviderError::Cancelled.into()),
                permit=Arc::clone(&slot.permits).acquire_owned()=>permit.map_err(|_|ProviderError::Cancelled)?,
            };
            let model = provider.resolve_model(&route.selector);
            let quote = slot.provider.text_quote(&model, req, cancel).await?;
            let base_id = hash(&(identity, &route.provider, &model))?;
            let id = self.ledger.begin_paid(&Purchase {
                logical: &base_id,
                stage: stage_of(identity.0),
                provider: &route.provider,
                capability: PaidCapability::Text,
                ceiling: quote.ceiling,
            })?;
            let response = match provider.generate(req, &model, cancel).await {
                Ok(response) => response,
                Err(error) => {
                    self.ledger.record_failure(&id, &error.to_string())?;
                    if matches!(
                        error,
                        ProviderError::Unbilled(_)
                            | ProviderError::Unavailable(_)
                            | ProviderError::Unsupported(_)
                    ) {
                        self.ledger.reconcile(&id, retold_core::Usd::ZERO)?;
                        return Err(error.into());
                    }
                    return Err(LedgerError::Uncertain(id).into());
                }
            };
            drop(permit);
            let artifact_hash = hash(&response.value)?;
            let value = serde_json::from_value(response.value)
                .map_err(|err| EngineError::Setup(err.to_string()))?;
            let receipt = Receipt {
                provider: route.provider.clone(),
                model,
                input_tokens: response.usage.input_tokens,
                output_tokens: response.usage.output_tokens,
                cost: response.usage.cost,
            };
            let settlement = Settlement {
                id,
                cost: response.usage.cost_known.then_some(response.usage.cost),
                artifact_hash,
            };
            return Ok((value, receipt, settlement));
        }
        Err(MediaError::Unavailable(format!(
            "no paid text route available for {tier}"
        )))
    }
    pub fn settle(&self, settlement: &Settlement) -> Result<(), MediaError> {
        let evidence = PaidEvidence {
            artifact_hash: &settlement.artifact_hash,
            cost: settlement.cost,
        };
        self.ledger
            .settle_with_evidence(&settlement.id, None, &evidence)?;
        Ok(())
    }
}
