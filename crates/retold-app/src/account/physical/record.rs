//! Versioned supervision evidence retains the legacy owner fields for conservative old readers.
use super::super::barrier::Owner;
use retold_engine::process_supervision::identity::Identity;
use serde::{Deserialize, Serialize};

pub(super) const RECORD_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Record {
    #[serde(flatten)]
    pub owner: Owner,
    pub schema_version: u32,
    pub supervisor: Identity,
    pub worker: Option<Identity>,
}

impl Record {
    #[cfg_attr(
        not(unix),
        expect(dead_code, reason = "only the Unix supervisor reaches this")
    )]
    pub fn current() -> super::super::Result<Self> {
        let owner = Owner {
            pid: std::process::id(),
            purpose: "Unconfirmed supervised process; physical recovery required.".into(),
            since: jiff::Timestamp::now(),
        };
        let supervisor =
            retold_engine::process_supervision::identity::read(owner.pid).map_err(|_| {
                super::super::Error::Invalid("Native supervisor identity is unavailable.")
            })?;
        Ok(Self {
            owner,
            schema_version: RECORD_VERSION,
            supervisor,
            worker: None,
        })
    }
}
