//! Journal commit precedes enabling the target; the original is retired first and never revived.
use super::super::{creation, invalid, publication};
use super::{Evidence, RECEIPT_FILE, RECOVERY, Receipt, Review, read_json};
use crate::protocol::Diagnostic;
use retold_engine::recovery::INACTIVE_MARKER;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Retired {
    schema_version: u32,
    transfer_id: String,
    target: std::path::PathBuf,
}

pub(super) fn prepare(restored: &Evidence, review: &Review) -> Result<(), Diagnostic> {
    let evidence = restored.root.join(RECOVERY);
    if !review.financial.committed {
        let receipt = Receipt {
            plan_hash: review.plan_hash.clone(),
            financial: review.financial.clone(),
            inactive: restored.inactive.clone(),
        };
        retold_engine::fsx::write_json_atomic(&evidence.join(RECEIPT_FILE), &receipt)
            .map_err(|_| invalid())?;
        publication::sync_directory(&evidence)?;
    }
    let retirement = Retired {
        schema_version: 1,
        transfer_id: review.financial.transfer_id.clone(),
        target: restored.root.clone(),
    };
    retire(&review.financial.transfer.original.root, &retirement)?;
    for video in &review.original.videos {
        retire(&video.directory, &retirement)?;
    }
    Ok(())
}

fn retire(root: &Path, retirement: &Retired) -> Result<(), Diagnostic> {
    let path = root.join(INACTIVE_MARKER);
    let exists = path.try_exists().map_err(|_| invalid())?;
    if exists {
        let prior: Retired = read_json(&path)?;
        if prior != *retirement {
            return Err(invalid());
        }
    } else {
        creation::write_json(&path, retirement)?;
    }
    publication::sync_directory(root)
}

pub(super) fn enable(restored: &Evidence) -> Result<(), Diagnostic> {
    for video in &restored.catalog.videos {
        enable_root(&video.directory)?;
    }
    enable_root(&restored.root)
}

fn enable_root(root: &Path) -> Result<(), Diagnostic> {
    let path = root.join(INACTIVE_MARKER);
    match std::fs::remove_file(&path) {
        Ok(()) => publication::sync_directory(root),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(invalid()),
    }
}
