//! Account-backed dispatch gate for engine work. Each per-run attempt slot maps to one
//! deterministic account attempt, so neither a new job nor a lost per-run ledger can
//! redispatch it. Ceilings come from the job's declared per-stage operation nodes.

use std::collections::BTreeMap;

use retold_engine::gate::{GateError, PaidCall, PaidCapability, PaidEvidence, PaidGate, Usd};

use crate::account::types::{
    Attempt, AttemptState, Capability, Job, Receipt, Reservation, Reserved, Resources, Unit,
};
use crate::account::{Account, Error};

#[derive(Debug)]
struct Authority {
    job_id: String,
    plan_hash: String,
}

#[derive(Debug)]
pub struct AccountGate {
    account: Account,
    workspace_id: String,
    video_id: String,
    authority: Option<Authority>,
}

impl AccountGate {
    /// Dispatches only within `job`, which must already be admitted.
    #[must_use]
    pub fn for_job(account: Account, job: &Job) -> Self {
        let Job {
            id,
            workspace,
            video_id,
            plan_hash,
            ..
        } = job;
        Self {
            account,
            workspace_id: workspace.id.clone(),
            video_id: video_id.clone(),
            authority: Some(Authority {
                job_id: id.clone(),
                plan_hash: plan_hash.clone(),
            }),
        }
    }

    /// Resolves billing for a video's earlier attempts; it can never dispatch.
    #[must_use]
    pub fn for_reconciliation(account: Account, workspace_id: &str, video_id: &str) -> Self {
        Self {
            account,
            workspace_id: workspace_id.into(),
            video_id: video_id.into(),
            authority: None,
        }
    }

    /// The account attempt behind a per-run attempt slot.
    pub fn attempt_id(&self, request: &str) -> Result<String, GateError> {
        digest(&("attempt", &self.workspace_id, &self.video_id, request))
    }

    fn authority(&self) -> Result<&Authority, GateError> {
        self.authority
            .as_ref()
            .ok_or_else(|| GateError::Blocked("reconciliation access cannot dispatch".into()))
    }

    fn operation_key(&self, attempt_id: &str, logical: &str) -> Result<String, GateError> {
        let is_empty = logical.is_empty();
        if is_empty {
            Ok(attempt_id.to_string())
        } else {
            digest(&("operation", &self.workspace_id, &self.video_id, logical))
        }
    }

    fn reservation(&self, call: &PaidCall<'_>) -> Result<Reservation, GateError> {
        let PaidCall {
            request,
            stage,
            provider,
            ceiling,
            ..
        } = *call;
        let authority = self.authority()?;
        let capability = capability(call.capability);
        let attempt_id = self.attempt_id(request)?;
        let operation_key = self.operation_key(&attempt_id, call.logical)?;
        Ok(Reservation {
            attempt_id,
            job_id: authority.job_id.clone(),
            operation_key,
            node: Some(operation_node(&self.video_id, stage, capability)?),
            route: provider.into(),
            capability,
            plan_hash: authority.plan_hash.clone(),
            resources: quote(ceiling)?,
        })
    }

    fn known(&self, request: &str) -> Result<Option<(String, Attempt)>, GateError> {
        let attempt_id = self.attempt_id(request)?;
        let found = match self.account.attempt(&attempt_id) {
            Ok(attempt) => Some(attempt),
            Err(Error::Transition) => None,
            Err(error) => return Err(gate_error(&error)),
        };
        Ok(found.map(|attempt| (attempt_id, attempt)))
    }
}

impl PaidGate for AccountGate {
    fn reserve(&self, call: &PaidCall<'_>) -> Result<(), GateError> {
        let reservation = self.reservation(call)?;
        match self.account.reserve(reservation) {
            Ok(Reserved::New(_)) => Ok(()),
            Ok(Reserved::Existing(attempt)) => resume_existing(&attempt),
            Err(error) => Err(gate_error(&error)),
        }
    }

    fn dispatch(&self, request: &str) -> Result<(), GateError> {
        let plan_hash = &self.authority()?.plan_hash;
        let attempt_id = self.attempt_id(request)?;
        self.account
            .mark_dispatched(&attempt_id, plan_hash)
            .map(drop)
            .map_err(|error| gate_error(&error))
    }

    fn withdraw(&self, request: &str) -> Result<(), GateError> {
        let attempt_id = self.attempt_id(request)?;
        self.account
            .cancel_reserved(&attempt_id)
            .map_err(|error| gate_error(&error))
    }

    fn settle(&self, request: &str, evidence: &PaidEvidence<'_>) -> Result<(), GateError> {
        let (attempt_id, attempt) = self
            .known(request)?
            .ok_or_else(|| GateError::Blocked("settlement names no account attempt".into()))?;
        let receipt = receipt(&attempt_id, evidence.artifact_hash, evidence.cost)?;
        let outcome = match attempt.state {
            AttemptState::Dispatched => self.account.settle(&attempt_id, receipt),
            // A later recovery may fill an unknown amount; known amounts never change.
            AttemptState::Settled => self.account.reconcile(&attempt_id, receipt),
            AttemptState::Reserved | AttemptState::NotDispatched => Err(Error::Transition),
        };
        outcome.map_err(|error| gate_error(&error))
    }

    fn reconcile(&self, request: &str, cost: Usd) -> Result<(), GateError> {
        // Attempts recorded only by an ungated run ledger have nothing to resolve here.
        let Some((attempt_id, _attempt)) = self.known(request)? else {
            return Ok(());
        };
        self.account
            .reconcile_attempt(&attempt_id, cost)
            .map_err(|error| gate_error(&error))
    }
}

/// Stable operation-tree node for one stage and capability of a video. Jobs declare their
/// ceilings under these keys before admission.
pub fn operation_node(
    video_id: &str,
    stage: &str,
    capability: Capability,
) -> Result<String, GateError> {
    digest(&("operation-node", video_id, stage, capability))
}

pub(crate) const fn capability(paid: PaidCapability) -> Capability {
    match paid {
        PaidCapability::Text => Capability::Text,
        PaidCapability::Image => Capability::Image,
        PaidCapability::Speech => Capability::Speech,
        PaidCapability::Transcription => Capability::Transcription,
    }
}

/// One request and the provider's worst-case price.
fn quote(ceiling: Usd) -> Result<Resources, GateError> {
    Ok(BTreeMap::from([
        (Unit::Requests, 1),
        (Unit::UsdMicros, micros(ceiling)?),
    ]))
}

fn receipt(attempt_id: &str, artifact_hash: &str, cost: Option<Usd>) -> Result<Receipt, GateError> {
    let usd_micros = cost.map(micros).transpose()?;
    let record = ("receipt", attempt_id, artifact_hash, usd_micros);
    Ok(Receipt {
        artifact_hash: artifact_hash.into(),
        receipt_hash: digest(&record)?,
        observed: BTreeMap::from([(Unit::Requests, Some(1)), (Unit::UsdMicros, usd_micros)]),
    })
}

/// Replaying an attempt is safe only while it has not crossed the dispatch boundary.
fn resume_existing(attempt: &Attempt) -> Result<(), GateError> {
    let attempt_id = &attempt.reservation.attempt_id;
    match attempt.state {
        AttemptState::Reserved => Ok(()),
        AttemptState::NotDispatched => Err(GateError::Blocked(format!(
            "withdrawn attempt {attempt_id} is never reused"
        ))),
        AttemptState::Dispatched | AttemptState::Settled => {
            Err(GateError::Uncertain(attempt_id.clone()))
        }
    }
}

fn micros(amount: Usd) -> Result<u64, GateError> {
    u64::try_from(amount.micros())
        .map_err(|_| GateError::Blocked("negative amounts are not charges".into()))
}

fn digest<T: serde::Serialize>(value: &T) -> Result<String, GateError> {
    retold_engine::store::hash(value).map_err(|error| GateError::Blocked(error.to_string()))
}

fn gate_error(error: &Error) -> GateError {
    let message = error.to_string();
    match error {
        Error::Limit => GateError::Refused(message),
        Error::OperationExists | Error::Transition => GateError::Uncertain(message),
        Error::Missing
        | Error::Exists
        | Error::Corrupt
        | Error::Busy
        | Error::Occupied(_)
        | Error::Invalid(_)
        | Error::Backup(_)
        | Error::BackupInvalid(_) => GateError::Blocked(message),
    }
}
