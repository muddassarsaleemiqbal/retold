//! Single-flight operation identity and a durable dispatch boundary; no retry on replay.
use super::state::{MAX_ATTEMPTS, State};
use super::types::{
    Attempt, AttemptState, Capability, DispatchPermit, Receipt, Reservation, Reserved, Unit,
};
use super::{Error, Result, budget, policy, usage};
use retold_engine::gate::Usd;

pub fn reserve(state: State, reservation: Reservation) -> Result<(State, Reserved)> {
    policy::identifier(&reservation.attempt_id)?;
    let prior = state.attempts.get(&reservation.attempt_id);
    if let Some(prior) = prior {
        if prior.reservation != reservation {
            return Err(Error::Invalid(
                "Attempt IDs cannot replace historical requests.",
            ));
        }
        let existing = prior.clone();
        return Ok((state, Reserved::Existing(existing)));
    }
    let attempt = prepare(&state, reservation)?;
    check_single_flight(&state, &attempt)?;
    budget::check(&state, &attempt)?;
    let full = state.attempts.len() >= MAX_ATTEMPTS;
    if full {
        return Err(Error::Limit);
    }
    Ok(record_reservation(state, attempt))
}

fn record_reservation(mut state: State, attempt: Attempt) -> (State, Reserved) {
    state.note("reserved", &attempt.reservation.attempt_id);
    state
        .attempts
        .insert(attempt.reservation.attempt_id.clone(), attempt.clone());
    (state, Reserved::New(attempt))
}

fn prepare(state: &State, reservation: Reservation) -> Result<Attempt> {
    let job = state
        .jobs
        .get(&reservation.job_id)
        .ok_or(Error::Invalid("Unknown job authority."))?;
    policy::validate_job(state, job)?;
    validate_request(job, &reservation)?;
    let provider = state
        .routes
        .get(&reservation.route)
        .ok_or(Error::Invalid("Unknown provider account."))?;
    let (day, month) = usage::periods(state)?;
    Ok(Attempt {
        reservation,
        provider_account: provider.clone(),
        workspace_id: job.workspace.id.clone(),
        video_id: job.video_id.clone(),
        series_id: job.series_id.clone(),
        day,
        month,
        timezone: state.timezone.clone(),
        state: AttemptState::Reserved,
        receipt: None,
        receipt_history: Vec::new(),
    })
}

fn validate_request(job: &super::types::Job, reservation: &Reservation) -> Result<()> {
    let pinned = reservation.plan_hash == job.plan_hash;
    let routed = job.routes.contains(&reservation.route);
    let capable = job.capabilities.contains(&reservation.capability);
    if !pinned || !routed || !capable {
        return Err(Error::Invalid(
            "Request is outside the pinned plan, route or capabilities.",
        ));
    }
    validate_quote(reservation)?;
    if let Some(node) = &reservation.node {
        policy::digest(node)?;
    }
    policy::digest(&reservation.operation_key)
}

fn validate_quote(reservation: &Reservation) -> Result<()> {
    use super::types::Unit;
    let requests = reservation
        .resources
        .get(&Unit::Requests)
        .copied()
        .unwrap_or(0);
    let usd_declared = reservation.resources.contains_key(&Unit::UsdMicros);
    if requests == 0 || !usd_declared {
        return Err(Error::Invalid(
            "Every quote must bound requests and explicitly declare USD micros, including zero.",
        ));
    }
    let timed = matches!(reservation.capability, Capability::Local);
    let local_millis = reservation
        .resources
        .get(&Unit::LocalMillis)
        .copied()
        .unwrap_or(0);
    if timed && local_millis == 0 {
        return Err(Error::Invalid(
            "Local execution needs a finite nonzero time bound.",
        ));
    }
    Ok(())
}

fn check_single_flight(state: &State, attempt: &Attempt) -> Result<()> {
    for prior in state.attempts.values() {
        let same_op = prior.workspace_id == attempt.workspace_id
            && prior.reservation.operation_key == attempt.reservation.operation_key
            && prior.reservation.attempt_id != attempt.reservation.attempt_id;
        if same_op {
            check_prior_attempt(prior)?;
        }
    }
    Ok(())
}

fn check_prior_attempt(prior: &Attempt) -> Result<()> {
    match prior.state {
        AttemptState::Reserved | AttemptState::Dispatched => Err(Error::OperationExists),
        AttemptState::Settled => ensure_unsettled_or_failed(prior),
        AttemptState::NotDispatched => Ok(()),
    }
}

fn ensure_unsettled_or_failed(attempt: &Attempt) -> Result<()> {
    if let Some(receipt) = &attempt.receipt {
        let no_artifact = hash(&("no-artifact", &attempt.reservation.attempt_id))?;
        if receipt.artifact_hash != no_artifact {
            return Err(Error::OperationExists);
        }
    }
    Ok(())
}

pub fn mark_dispatched(
    mut state: State,
    attempt_id: &str,
    plan_hash: &str,
) -> Result<(State, DispatchPermit)> {
    let attempt = state.attempts.get(attempt_id).ok_or(Error::Transition)?;
    let job = state
        .jobs
        .get(&attempt.reservation.job_id)
        .ok_or(Error::Corrupt)?;
    policy::validate_job(&state, job)?;
    if attempt.state != AttemptState::Reserved || attempt.reservation.plan_hash != plan_hash {
        return Err(Error::Transition);
    }
    let attempt = state
        .attempts
        .get_mut(attempt_id)
        .ok_or(Error::Transition)?;
    attempt.state = AttemptState::Dispatched;
    state.note("dispatched", attempt_id);
    Ok((
        state,
        DispatchPermit {
            attempt_id: attempt_id.into(),
        },
    ))
}

pub fn cancel_reserved(mut state: State, attempt_id: &str) -> Result<(State, ())> {
    let attempt = state
        .attempts
        .get_mut(attempt_id)
        .ok_or(Error::Transition)?;
    if attempt.state == AttemptState::NotDispatched {
        return Ok((state, ()));
    }
    if attempt.state != AttemptState::Reserved {
        return Err(Error::Transition);
    }
    attempt.state = AttemptState::NotDispatched;
    state.note("definitely_not_dispatched", attempt_id);
    Ok((state, ()))
}

pub fn settle(mut state: State, attempt_id: &str, receipt: Receipt) -> Result<(State, ())> {
    validate_receipt(&receipt)?;
    let attempt = state
        .attempts
        .get_mut(attempt_id)
        .ok_or(Error::Transition)?;
    let replay = attempt.receipt.as_ref() == Some(&receipt);
    if replay {
        return Ok((state, ()));
    }
    if attempt.state != AttemptState::Dispatched {
        return Err(Error::Transition);
    }
    attempt.receipt_history.push(receipt.clone());
    attempt.receipt = Some(receipt);
    attempt.state = AttemptState::Settled;
    state.note("settled", attempt_id);
    Ok((state, ()))
}

pub fn reconcile(mut state: State, attempt_id: &str, receipt: Receipt) -> Result<(State, ())> {
    validate_receipt(&receipt)?;
    let attempt = state
        .attempts
        .get_mut(attempt_id)
        .ok_or(Error::Transition)?;
    let original = attempt.receipt.as_ref().ok_or(Error::Transition)?;
    if *original == receipt {
        return Ok((state, ()));
    }
    if receipt.artifact_hash != original.artifact_hash {
        return Err(Error::Transition);
    }
    for (unit, amount) in &original.observed {
        let changed = amount.is_some() && receipt.observed.get(unit) != Some(amount);
        if changed {
            return Err(Error::Transition);
        }
    }
    attempt.receipt_history.push(receipt.clone());
    attempt.receipt = Some(receipt);
    state.note("reconciled_unknown_observations", attempt_id);
    Ok((state, ()))
}

fn validate_receipt(receipt: &Receipt) -> Result<()> {
    policy::digest(&receipt.artifact_hash)?;
    policy::digest(&receipt.receipt_hash)
}

fn reconcile_reserved(mut state: State, attempt_id: &str, cost: Usd) -> Result<State> {
    if cost != Usd::ZERO {
        return Err(Error::Transition);
    }
    let attempt = state
        .attempts
        .get_mut(attempt_id)
        .ok_or(Error::Transition)?;
    if attempt.state == AttemptState::Reserved {
        attempt.state = AttemptState::NotDispatched;
        state.note("definitely_not_dispatched", attempt_id);
    }
    Ok(state)
}

fn reconcile_dispatched(mut state: State, attempt_id: &str, cost: Usd) -> Result<State> {
    let artifact_hash = hash(&("no-artifact", attempt_id))?;
    let target = ReceiptTarget {
        attempt_id,
        artifact_hash: &artifact_hash,
    };
    let receipt = initial_reconcile_receipt(target, cost)?;
    let attempt = state
        .attempts
        .get_mut(attempt_id)
        .ok_or(Error::Transition)?;
    attempt.receipt_history.push(receipt.clone());
    attempt.receipt = Some(receipt);
    attempt.state = AttemptState::Settled;
    state.note("settled", attempt_id);
    Ok(state)
}

fn check_immutable_observations(original: &Receipt, updated: &Receipt) -> Result<()> {
    for (unit, amount) in &original.observed {
        let changed = amount.is_some() && updated.observed.get(unit) != Some(amount);
        if changed {
            return Err(Error::Transition);
        }
    }
    Ok(())
}

fn apply_settled_receipt(mut state: State, attempt_id: &str, receipt: Receipt) -> Result<State> {
    let attempt = state
        .attempts
        .get_mut(attempt_id)
        .ok_or(Error::Transition)?;
    attempt.receipt_history.push(receipt.clone());
    attempt.receipt = Some(receipt);
    state.note("reconciled_unknown_observations", attempt_id);
    Ok(state)
}

fn reconcile_settled(state: State, attempt_id: &str, cost: Usd) -> Result<State> {
    let original = state
        .attempts
        .get(attempt_id)
        .and_then(|saved| saved.receipt.as_ref())
        .ok_or(Error::Transition)?
        .clone();
    let target = ReceiptTarget {
        attempt_id,
        artifact_hash: &original.artifact_hash,
    };
    let receipt = update_reconcile_receipt(target, cost, &original)?;
    if original == receipt {
        return Ok(state);
    }
    check_immutable_observations(&original, &receipt)?;
    apply_settled_receipt(state, attempt_id, receipt)
}

/// Explicit billing reconciliation for an attempt without dispatch authority.
pub fn reconcile_attempt(state: State, attempt_id: &str, cost: Usd) -> Result<(State, ())> {
    let attempt = state.attempts.get(attempt_id).ok_or(Error::Transition)?;
    let new_state = match attempt.state {
        AttemptState::Reserved | AttemptState::NotDispatched => {
            reconcile_reserved(state, attempt_id, cost)?
        }
        AttemptState::Dispatched => reconcile_dispatched(state, attempt_id, cost)?,
        AttemptState::Settled => reconcile_settled(state, attempt_id, cost)?,
    };
    Ok((new_state, ()))
}

#[derive(Clone, Copy)]
struct ReceiptTarget<'a> {
    attempt_id: &'a str,
    artifact_hash: &'a str,
}

fn initial_reconcile_receipt(target: ReceiptTarget<'_>, cost: Usd) -> Result<Receipt> {
    let observed = std::collections::BTreeMap::from([(Unit::Requests, Some(1))]);
    make_reconcile_receipt(target, cost, observed)
}

fn update_reconcile_receipt(
    target: ReceiptTarget<'_>,
    cost: Usd,
    original: &Receipt,
) -> Result<Receipt> {
    make_reconcile_receipt(target, cost, original.observed.clone())
}

fn make_reconcile_receipt(
    target: ReceiptTarget<'_>,
    cost: Usd,
    mut observed: std::collections::BTreeMap<Unit, Option<u64>>,
) -> Result<Receipt> {
    let usd_micros = u64::try_from(cost.micros()).map_err(|_| Error::Invalid("Negative cost."))?;
    observed.insert(Unit::UsdMicros, Some(usd_micros));
    let record = (
        "receipt",
        target.attempt_id,
        target.artifact_hash,
        Some(usd_micros),
    );
    let receipt_hash = hash(&record)?;
    let receipt = Receipt {
        artifact_hash: target.artifact_hash.into(),
        receipt_hash,
        observed,
    };
    validate_receipt(&receipt)?;
    Ok(receipt)
}

fn hash<T: serde::Serialize>(value: &T) -> Result<String> {
    retold_engine::store::hash(value).map_err(|_| Error::Corrupt)
}
