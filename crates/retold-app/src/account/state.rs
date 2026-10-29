//! Authoritative account state persisted in one bounded SQLite transaction.
use super::types::{Attempt, Job, Limit, Workspace};
use super::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_ATTEMPTS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audit {
    pub at: jiff::Timestamp,
    pub kind: String,
    pub entity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema: u32,
    pub account_id: String,
    pub timezone: String,
    pub last_seen: jiff::Timestamp,
    pub workspaces: BTreeMap<String, Workspace>,
    pub routes: BTreeMap<String, String>,
    pub limits: Vec<Limit>,
    pub jobs: BTreeMap<String, Job>,
    pub attempts: BTreeMap<String, Attempt>,
    pub audit: Vec<Audit>,
}

impl State {
    pub fn empty(timezone: &str) -> Result<Self> {
        jiff::tz::TimeZone::get(timezone).map_err(|_| Error::Invalid("Unknown IANA timezone."))?;
        let now = jiff::Timestamp::now();
        let account_id =
            retold_engine::store::hash(&(timezone, now)).map_err(|_| Error::Corrupt)?;
        Ok(Self {
            schema: FORMAT_VERSION,
            account_id,
            timezone: timezone.into(),
            last_seen: now,
            workspaces: BTreeMap::new(),
            routes: BTreeMap::new(),
            limits: Vec::new(),
            jobs: BTreeMap::new(),
            attempts: BTreeMap::new(),
            audit: Vec::new(),
        })
    }

    pub fn note(&mut self, kind: &str, entity: &str) {
        self.audit.push(Audit {
            at: self.last_seen,
            kind: kind.into(),
            entity: entity.into(),
        });
    }

    pub fn validate(&self) -> Result<()> {
        let invalid = self.schema != FORMAT_VERSION || self.attempts.len() > MAX_ATTEMPTS;
        if invalid {
            return Err(Error::Corrupt);
        }
        jiff::tz::TimeZone::get(&self.timezone).map_err(|_| Error::Corrupt)?;
        Ok(())
    }
}
