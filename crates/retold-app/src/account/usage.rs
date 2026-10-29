//! Unknown observations remain held; known charges belong to the reservation's period.
use super::types::{Attempt, AttemptState, Balance, Scope, Summary, Unit, Window};
use super::{Error, Result, state::State};

pub fn periods(state: &State) -> Result<(String, String)> {
    let zone = jiff::tz::TimeZone::get(&state.timezone).map_err(|_| Error::Corrupt)?;
    let date = state.last_seen.to_zoned(zone).date();
    Ok((
        date.to_string(),
        format!("{:04}-{:02}", date.year(), date.month()),
    ))
}

/// Every accounted resource dimension, in report order.
pub const UNITS: [Unit; 7] = [
    Unit::UsdMicros,
    Unit::Requests,
    Unit::InputTokens,
    Unit::OutputTokens,
    Unit::AudioMillis,
    Unit::LocalMillis,
    Unit::VisionPixels,
];

pub fn applies(scope: &Scope, attempt: &Attempt) -> bool {
    matches!(scope, Scope::Account)
        || matches!(scope, Scope::Workspace(id) if *id == attempt.workspace_id)
        || matches!(scope, Scope::Video(id) if *id == attempt.video_id)
        || matches!(scope, Scope::Series(id) if attempt.series_id.as_ref() == Some(id))
        || matches!(scope, Scope::Provider(id) if *id == attempt.provider_account)
}

pub struct Usage<'a> {
    pub unit: Unit,
    pub window: Window,
    pub periods: &'a (String, String),
}
impl Usage<'_> {
    pub fn sum<'a>(&self, mut attempts: impl Iterator<Item = &'a Attempt>) -> Result<Balance> {
        attempts.try_fold(Balance::default(), |total, attempt| {
            let next = self.amount(attempt);
            Ok(Balance {
                charged: total
                    .charged
                    .checked_add(next.charged)
                    .ok_or(Error::Limit)?,
                held: total.held.checked_add(next.held).ok_or(Error::Limit)?,
                unknown_observations: total
                    .unknown_observations
                    .checked_add(next.unknown_observations)
                    .ok_or(Error::Limit)?,
            })
        })
    }
    fn amount(&self, attempt: &Attempt) -> Balance {
        if attempt.state == AttemptState::NotDispatched {
            return Balance::default();
        }
        let observed = observed(attempt, self.unit);
        let quoted = quoted(attempt, self.unit);
        observed.map_or_else(
            || Balance {
                held: quoted,
                unknown_observations: u64::from(unknown_observation(attempt, self.unit)),
                ..Balance::default()
            },
            |charged| Balance {
                charged: self.period_charge(attempt, charged),
                ..Balance::default()
            },
        )
    }
    fn period_charge(&self, attempt: &Attempt, charged: u64) -> u64 {
        let same_window = match self.window {
            Window::Lifetime => true,
            Window::Day => attempt.day == self.periods.0,
            Window::Month => attempt.month == self.periods.1,
        };
        if same_window { charged } else { 0 }
    }
}

pub fn summary(state: &State) -> Result<Summary> {
    let lifetime = lifetime(state)?;
    let (workspaces, jobs, attempts) = counts(state);
    Ok(Summary {
        account_id: state.account_id.clone(),
        timezone: state.timezone.clone(),
        workspaces,
        jobs,
        attempts,
        limits: state.limits.clone(),
        lifetime,
    })
}

fn observed(attempt: &Attempt, unit: Unit) -> Option<u64> {
    attempt
        .receipt
        .as_ref()
        .and_then(|receipt| receipt.observed.get(&unit))
        .copied()
        .flatten()
}
fn quoted(attempt: &Attempt, unit: Unit) -> u64 {
    attempt
        .reservation
        .resources
        .get(&unit)
        .copied()
        .unwrap_or(0)
}
fn lifetime(state: &State) -> Result<std::collections::BTreeMap<Unit, Balance>> {
    totals(state, |_| true)
}

/// Lifetime totals of one job's attempts.
pub fn job(state: &State, job_id: &str) -> Result<std::collections::BTreeMap<Unit, Balance>> {
    totals(state, |attempt| attempt.reservation.job_id == job_id)
}

fn totals(
    state: &State,
    include: impl Fn(&Attempt) -> bool,
) -> Result<std::collections::BTreeMap<Unit, Balance>> {
    let periods = periods(state)?;
    UNITS
        .into_iter()
        .map(|unit| {
            let usage = Usage {
                unit,
                window: Window::Lifetime,
                periods: &periods,
            };
            let attempts = state.attempts.values().filter(|attempt| include(attempt));
            Ok((unit, usage.sum(attempts)?))
        })
        .collect()
}

fn counts(state: &State) -> (usize, usize, usize) {
    (
        state.workspaces.len(),
        state.jobs.len(),
        state.attempts.len(),
    )
}

/// Still holding its quote: reserved, dispatched, or settled with an unknown observation.
pub fn unresolved(attempt: &Attempt) -> bool {
    let in_flight = matches!(
        attempt.state,
        AttemptState::Reserved | AttemptState::Dispatched
    );
    let settled = attempt.state == AttemptState::Settled;
    let unknown = UNITS
        .into_iter()
        .any(|unit| unknown_observation(attempt, unit) && observed(attempt, unit).is_none());
    in_flight || (settled && unknown)
}

fn unknown_observation(attempt: &Attempt, unit: Unit) -> bool {
    attempt.receipt.as_ref().is_some_and(|receipt| {
        attempt.reservation.resources.contains_key(&unit) || receipt.observed.contains_key(&unit)
    })
}
