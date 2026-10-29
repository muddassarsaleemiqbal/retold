//! Bounded read-only attempt inspection before billing reconciliation.
use super::{
    Account, Error, Result, database,
    types::{Attempt, AttemptState, Resources, Unit},
};
use serde::Serialize;

pub const DEFAULT_PAGE: usize = 50;
pub const MAX_PAGE: usize = 200;

#[derive(Debug)]
pub struct Selection<'a> {
    pub after: Option<&'a str>,
    pub limit: usize,
    pub state: Option<AttemptState>,
}

#[derive(Debug, Serialize)]
pub struct Page {
    pub attempts: Vec<Row>,
    pub next_after: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub id: String,
    pub job_id: String,
    pub workspace_id: String,
    pub video_id: String,
    pub state: AttemptState,
    pub quoted: Resources,
    pub observed: std::collections::BTreeMap<Unit, Option<u64>>,
}

impl Account {
    pub fn attempts(&self, selection: &Selection<'_>) -> Result<Page> {
        let invalid = selection.limit == 0 || selection.limit > MAX_PAGE;
        if invalid {
            return Err(Error::Invalid(
                "Attempt page size must be between 1 and 200.",
            ));
        }
        let state = database::read(&self.directory)?;
        Ok(page(&state, selection))
    }
}

fn page(state: &super::state::State, selection: &Selection<'_>) -> Page {
    let &Selection {
        after,
        limit,
        state: desired,
    } = selection;
    let selected = state.attempts.iter().filter(|(id, attempt)| {
        let after = after.is_none_or(|after| id.as_str() > after);
        let matches = desired.is_none_or(|status| attempt.state == status);
        after && matches
    });
    let attempts: Vec<_> = selected
        .take(limit + 1)
        .map(|(id, attempt)| row(id, attempt))
        .collect();
    finish(attempts, limit)
}

fn finish(mut attempts: Vec<Row>, limit: usize) -> Page {
    let more = attempts.len() > limit;
    attempts.truncate(limit);
    let next_after = more
        .then(|| attempts.last().map(|row| row.id.clone()))
        .flatten();
    Page {
        attempts,
        next_after,
    }
}

fn row(id: &str, attempt: &Attempt) -> Row {
    Row {
        id: id.into(),
        job_id: attempt.reservation.job_id.clone(),
        workspace_id: attempt.workspace_id.clone(),
        video_id: attempt.video_id.clone(),
        state: attempt.state,
        quoted: attempt.reservation.resources.clone(),
        observed: attempt
            .receipt
            .as_ref()
            .map_or_else(Default::default, |receipt| receipt.observed.clone()),
    }
}

#[must_use]
pub fn respond(selection: &Selection<'_>) -> crate::protocol::Payload {
    use crate::protocol::{Outcome, Payload};
    let result = super::canonical_directory()
        .and_then(|directory| Account::open(&directory))
        .and_then(|account| account.attempts(selection));
    match result {
        Ok(page) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"account_attempts": page}),
        },
        Err(error) => super::commands::diagnostic(&error).into_payload(),
    }
}
