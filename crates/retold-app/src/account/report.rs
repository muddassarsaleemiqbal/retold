//! Read-only spend reports. A charge belongs to the period its reservation started in. Holds stay
//! visible until resolved and count against current ceilings exactly as admission counts them.
//! Nothing here changes balances or grants authority.
use super::types::{Attempt, Balance, Scope, Unit, Window};
use super::usage::{self, UNITS, Usage};
use super::{Account, Result, database, state::State};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Grouping {
    Provider,
    Video,
    Series,
    Day,
}

#[derive(Debug, Clone, Copy)]
pub struct Query {
    pub window: Window,
    pub by: Grouping,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub timezone: String,
    pub window: Window,
    /// The current day or month in the account timezone; absent for lifetime reports.
    pub period: Option<String>,
    pub by: Grouping,
    pub rows: Vec<Row>,
    pub total: Totals,
    /// Unresolved holds from earlier periods, which still reduce this period's headroom.
    pub carried: Option<Totals>,
    /// The account ceiling for this window, when one is set.
    pub ceiling: Option<Ceiling>,
}

#[derive(Debug, Serialize)]
pub struct Row {
    /// Provider account, video, series or day; absent for attempts outside any series.
    pub key: Option<String>,
    #[serde(flatten)]
    pub totals: Totals,
    /// The finite limit on this provider account, video or series for the window, when set.
    pub ceiling: Option<Ceiling>,
}

#[derive(Debug, Serialize)]
pub struct Totals {
    pub attempts: usize,
    /// Attempts still holding their quote until recovery or reconciliation.
    pub unresolved: usize,
    pub resources: BTreeMap<Unit, Balance>,
}

/// A finite limit and the usage admission would count against it now.
#[derive(Debug, Serialize)]
pub struct Headroom {
    pub limit: u64,
    pub used: u64,
    pub available: u64,
}

pub type Ceiling = BTreeMap<Unit, Headroom>;

impl Account {
    pub fn spend(&self, query: Query) -> Result<Report> {
        let mut state = database::read(&self.directory)?;
        // Reports name the current period, not the one of the last recorded change.
        state.last_seen = state.last_seen.max(self.clock.now());
        let reporter = Reporter {
            periods: usage::periods(&state)?,
            state: &state,
            query,
        };
        reporter.report()
    }
}

struct Reporter<'a> {
    state: &'a State,
    query: Query,
    periods: (String, String),
}

impl Reporter<'_> {
    fn report(&self) -> Result<Report> {
        let attempts = &self.state.attempts;
        let (current, earlier): (Vec<&Attempt>, Vec<&Attempt>) =
            attempts.values().partition(|attempt| self.within(attempt));
        Ok(Report {
            timezone: self.state.timezone.clone(),
            window: self.query.window,
            period: self.period(),
            by: self.query.by,
            rows: self.rows(&current)?,
            total: self.totals(&current)?,
            carried: self.carried(earlier)?,
            ceiling: self.ceiling(&Scope::Account)?,
        })
    }

    fn period(&self) -> Option<String> {
        match self.query.window {
            Window::Lifetime => None,
            Window::Day => Some(self.periods.0.clone()),
            Window::Month => Some(self.periods.1.clone()),
        }
    }

    fn within(&self, attempt: &Attempt) -> bool {
        match self.query.window {
            Window::Lifetime => true,
            Window::Day => attempt.day == self.periods.0,
            Window::Month => attempt.month == self.periods.1,
        }
    }

    fn rows(&self, attempts: &[&Attempt]) -> Result<Vec<Row>> {
        let mut groups: BTreeMap<Option<String>, Vec<&Attempt>> = BTreeMap::new();
        for &attempt in attempts {
            let key = group_key(attempt, self.query.by);
            groups.entry(key).or_default().push(attempt);
        }
        groups
            .into_iter()
            .map(|(key, attempts)| self.row(key, &attempts))
            .collect()
    }

    fn row(&self, key: Option<String>, attempts: &[&Attempt]) -> Result<Row> {
        let ceiling = match self.scope(key.as_deref()) {
            Some(scope) => self.ceiling(&scope)?,
            None => None,
        };
        Ok(Row {
            totals: self.totals(attempts)?,
            key,
            ceiling,
        })
    }

    /// The limit scope a row's key names; days have no scope of their own.
    fn scope(&self, key: Option<&str>) -> Option<Scope> {
        let key = String::from(key?);
        match self.query.by {
            Grouping::Provider => Some(Scope::Provider(key)),
            Grouping::Video => Some(Scope::Video(key)),
            Grouping::Series => Some(Scope::Series(key)),
            Grouping::Day => None,
        }
    }

    fn totals(&self, attempts: &[&Attempt]) -> Result<Totals> {
        let resources = UNITS
            .into_iter()
            .map(|unit| Ok((unit, self.usage(unit).sum(attempts.iter().copied())?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let unresolved = attempts
            .iter()
            .filter(|attempt| usage::unresolved(attempt))
            .count();
        Ok(Totals {
            attempts: attempts.len(),
            unresolved,
            resources: reported(resources),
        })
    }

    fn carried(&self, earlier: Vec<&Attempt>) -> Result<Option<Totals>> {
        let held: Vec<_> = earlier
            .into_iter()
            .filter(|attempt| usage::unresolved(attempt))
            .collect();
        let periodic = self.query.window != Window::Lifetime;
        periodic.then(|| self.totals(&held)).transpose()
    }

    /// Headroom under the limit set for `scope` in this window, counted as admission counts it.
    fn ceiling(&self, scope: &Scope) -> Result<Option<Ceiling>> {
        let window = self.query.window;
        let limit = self
            .state
            .limits
            .iter()
            .find(|limit| limit.scope == *scope && limit.window == window);
        limit
            .map(|limit| {
                let ceilings = limit.resources.iter();
                ceilings
                    .map(|(&unit, &ceiling)| Ok((unit, self.headroom(scope, unit, ceiling)?)))
                    .collect::<Result<Ceiling>>()
            })
            .transpose()
    }

    fn headroom(&self, scope: &Scope, unit: Unit, limit: u64) -> Result<Headroom> {
        let applicable = self
            .state
            .attempts
            .values()
            .filter(|attempt| usage::applies(scope, attempt));
        let total = self.usage(unit).sum(applicable)?;
        let used = total.charged.saturating_add(total.held);
        Ok(Headroom {
            limit,
            used,
            available: limit.saturating_sub(used),
        })
    }

    const fn usage(&self, unit: Unit) -> Usage<'_> {
        Usage {
            unit,
            window: self.query.window,
            periods: &self.periods,
        }
    }
}

fn group_key(attempt: &Attempt, by: Grouping) -> Option<String> {
    let key = match by {
        Grouping::Provider => &attempt.provider_account,
        Grouping::Video => &attempt.video_id,
        Grouping::Series => attempt.series_id.as_ref()?,
        Grouping::Day => &attempt.day,
    };
    Some(key.clone())
}

/// Headroom under the limit set for `scope` and `window`, counted at the state's current period.
pub(super) fn headroom(state: &State, scope: &Scope, window: Window) -> Result<Option<Ceiling>> {
    let reporter = Reporter {
        periods: usage::periods(state)?,
        state,
        query: Query {
            window,
            by: Grouping::Provider,
        },
    };
    reporter.ceiling(scope)
}

/// Money and requests always; other dimensions only when something used them.
fn reported(resources: BTreeMap<Unit, Balance>) -> BTreeMap<Unit, Balance> {
    resources
        .into_iter()
        .filter(|(unit, balance)| {
            let required = matches!(unit, Unit::UsdMicros | Unit::Requests);
            let used = balance.charged > 0 || balance.held > 0 || balance.unknown_observations > 0;
            required || used
        })
        .collect()
}

#[must_use]
pub fn respond(query: Query) -> crate::protocol::Payload {
    use crate::protocol::{Outcome, Payload};
    let result = super::canonical_directory()
        .and_then(|directory| Account::open(&directory))
        .and_then(|account| account.spend(query));
    match result {
        Ok(report) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"spend": report}),
        },
        Err(error) => super::commands::diagnostic(&error).into_payload(),
    }
}
