//! Read-only inspection of live supervision and retained physical uncertainty.
use super::{Account, Error, Result};
use crate::protocol::{Outcome, Payload};
use retold_engine::process_supervision::identity::{Identity, Observation};
use serde::Serialize;

pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy)]
pub struct Selection<'a> {
    pub after: Option<&'a str>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    Updating,
    Unconfirmed,
    LegacyUnconfirmed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Process {
    pub id: String,
    pub status: Status,
    pub owner: super::barrier::Owner,
    pub schema_version: Option<u32>,
    pub supervisor: Option<Identity>,
    pub worker: Option<Identity>,
    pub supervisor_observation: Option<Observation>,
    pub worker_observation: Option<Observation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub processes: Vec<Process>,
    pub next_after: Option<String>,
}

impl Account {
    /// A bounded live view; no claim, queue, charge or process is changed.
    pub fn processes(&self, selection: Selection<'_>) -> Result<Page> {
        if selection.limit == 0 || selection.limit > MAX_LIMIT {
            return Err(Error::Invalid(
                "Physical inspection limit must be between 1 and 100.",
            ));
        }
        super::physical::inspect(&self.directory, selection)
    }
}

#[must_use]
pub fn respond(selection: Selection<'_>) -> Payload {
    let result = super::canonical_directory()
        .and_then(|directory| Account::open(&directory))
        .and_then(|account| account.processes(selection))
        .and_then(|page| serde_json::to_value(page).map_err(|_| Error::Corrupt));
    match result {
        Ok(contents) => Payload::Result {
            outcome: Outcome::Completed,
            contents,
        },
        Err(error) => super::commands::diagnostic(&error).into_payload(),
    }
}
