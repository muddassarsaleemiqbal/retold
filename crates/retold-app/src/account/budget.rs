//! Every resource dimension must fit job, operation and cumulative scope ceilings.
use super::types::{Action, Attempt, Balance, Job, Scope, Unit, Window};
use super::{
    Error, Result,
    state::State,
    usage::{self, Usage},
};

pub fn check(state: &State, attempt: &Attempt) -> Result<()> {
    let job = state
        .jobs
        .get(&attempt.reservation.job_id)
        .ok_or(Error::Corrupt)?;
    for (&unit, &quote) in &attempt.reservation.resources {
        let check = ResourceCheck {
            state,
            attempt,
            unit,
            quote,
            periods: usage::periods(state)?,
        };
        check.job(job)?;
        check.required_scopes(&job.action)?;
        check.configured_scopes()?;
    }
    Ok(())
}

struct ResourceCheck<'a> {
    state: &'a State,
    attempt: &'a Attempt,
    unit: Unit,
    quote: u64,
    periods: (String, String),
}
impl ResourceCheck<'_> {
    fn job(&self, job: &Job) -> Result<()> {
        let usage = Usage {
            unit: self.unit,
            window: Window::Lifetime,
            periods: &self.periods,
        };
        let total = usage.sum(
            self.state
                .attempts
                .values()
                .filter(|prior| prior.reservation.job_id == job.id),
        )?;
        headroom(
            &total,
            self.quote,
            job.resources.get(&self.unit).copied().unwrap_or(0),
        )?;
        self.operation(job, &usage)
    }
    fn operation(&self, job: &Job, usage: &Usage<'_>) -> Result<()> {
        let node = self.attempt.reservation.budget_node();
        let operation = job.operations.get(node).ok_or(Error::Limit)?;
        let total = usage.sum(self.state.attempts.values().filter(|prior| {
            prior.reservation.job_id == job.id && prior.reservation.budget_node() == node
        }))?;
        headroom(
            &total,
            self.quote,
            operation.get(&self.unit).copied().unwrap_or(0),
        )
    }
    fn required_scopes(&self, action: &Action) -> Result<()> {
        for (scope, window) in required(action, self.attempt) {
            self.required(&scope, window)?;
        }
        if let Some(series) = &self.attempt.series_id {
            self.required(&Scope::Series(series.clone()), Window::Lifetime)?;
        }
        Ok(())
    }
    fn required(&self, scope: &Scope, window: Window) -> Result<()> {
        let ceiling = self
            .state
            .limits
            .iter()
            .find(|limit| limit.scope == *scope && limit.window == window)
            .and_then(|limit| limit.resources.get(&self.unit))
            .copied()
            .unwrap_or(0);
        let total = self.total(scope, window)?;
        headroom(&total, self.quote, ceiling)
    }
    fn configured_scopes(&self) -> Result<()> {
        for limit in &self.state.limits {
            let applies = usage::applies(&limit.scope, self.attempt);
            if !applies {
                continue;
            }
            let total = self.total(&limit.scope, limit.window)?;
            headroom(
                &total,
                self.quote,
                limit.resources.get(&self.unit).copied().unwrap_or(0),
            )?;
        }
        Ok(())
    }
    fn total(&self, scope: &Scope, window: Window) -> Result<Balance> {
        let usage = Usage {
            unit: self.unit,
            window,
            periods: &self.periods,
        };
        usage.sum(
            self.state
                .attempts
                .values()
                .filter(|prior| usage::applies(scope, prior)),
        )
    }
}
/// Scopes that must carry a finite limit before any attempt. Provider tests belong to no
/// workspace or video, so their provider account carries the lifetime ceiling instead.
fn required(action: &Action, attempt: &Attempt) -> Vec<(Scope, Window)> {
    let lifetime = match action {
        Action::ProviderTest { .. } => vec![Scope::Provider(attempt.provider_account.clone())],
        Action::Continue | Action::Export { .. } => vec![
            Scope::Workspace(attempt.workspace_id.clone()),
            Scope::Video(attempt.video_id.clone()),
        ],
    };
    let periods = [Window::Day, Window::Month].map(|window| (Scope::Account, window));
    let lifetimes = lifetime.into_iter().map(|scope| (scope, Window::Lifetime));
    periods.into_iter().chain(lifetimes).collect()
}

fn headroom(total: &Balance, quote: u64, ceiling: u64) -> Result<()> {
    let used = total
        .charged
        .checked_add(total.held)
        .and_then(|amount| amount.checked_add(quote))
        .ok_or(Error::Limit)?;
    if used > ceiling {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
