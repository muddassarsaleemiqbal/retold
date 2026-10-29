//! Durable uncertainty survives loss of a supervisor's OS activity lock.
#![cfg_attr(
    not(unix),
    expect(dead_code, reason = "only the Unix supervisor reaches this")
)]
use std::fs::{File, OpenOptions};
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

use super::{Error, Result, barrier::Owner};
mod inspection;
mod record;
pub(super) use inspection::inspect;
use record::Record;

const CLAIMS_FOLDER: &str = "physical-processes";
const MAX_RECORDS: usize = 4096;
const MAX_BYTES: u64 = 4096;
const MAX_FAILURE_CHARS: usize = 512;

/// Deliberately has no cleanup destructor: only confirmed process completion clears evidence.
pub struct Claim {
    path: PathBuf,
    lock: File,
    record: Record,
}

impl Claim {
    pub fn begin(directory: &Path) -> Result<Self> {
        let record = Record::current()?;
        let root = directory.join(CLAIMS_FOLDER);
        std::fs::create_dir_all(&root).map_err(|_| Error::Corrupt)?;
        retold_engine::fsx::sync_directory(directory).map_err(|_| Error::Corrupt)?;
        let file = tempfile::Builder::new()
            .prefix("process-")
            .tempfile_in(&root)
            .map_err(|_| Error::Corrupt)?;
        let (mut lock, path) = file.keep().map_err(|_| Error::Corrupt)?;
        lock.lock().map_err(|_| Error::Corrupt)?;
        let bytes = serde_json::to_vec(&record).map_err(|_| Error::Corrupt)?;
        lock.write_all(&bytes).map_err(|_| Error::Corrupt)?;
        lock.sync_all().map_err(|_| Error::Corrupt)?;
        retold_engine::fsx::sync_directory(&root).map_err(|_| Error::Corrupt)?;
        Ok(Self { path, lock, record })
    }

    pub fn retain_failure(mut self, error: &std::io::Error) -> Result<()> {
        let detail: String = error.to_string().chars().take(MAX_FAILURE_CHARS).collect();
        self.record.owner.purpose = format!("Physical recovery required: {detail}");
        self.persist().map(drop)
    }

    pub fn register_worker(
        mut self,
        identity: retold_engine::process_supervision::identity::Identity,
    ) -> Result<Self> {
        let registered = self.record.worker.is_some();
        if registered {
            return Err(Error::Transition);
        }
        self.record.worker = Some(identity);
        self.persist()
    }

    fn persist(mut self) -> Result<Self> {
        let bytes = serde_json::to_vec(&self.record).map_err(|_| Error::Corrupt)?;
        self.lock.rewind().map_err(|_| Error::Corrupt)?;
        self.lock.set_len(0).map_err(|_| Error::Corrupt)?;
        self.lock.write_all(&bytes).map_err(|_| Error::Corrupt)?;
        self.lock.sync_all().map_err(|_| Error::Corrupt)?;
        Ok(self)
    }

    pub fn complete(self) -> Result<()> {
        std::fs::remove_file(&self.path).map_err(|_| Error::Corrupt)?;
        let parent = self.path.parent().ok_or(Error::Corrupt)?;
        retold_engine::fsx::sync_directory(parent).map_err(|_| Error::Corrupt)
    }
}

/// Live claims are covered by activity locks. Unlocked records never expire or trust PID reuse.
pub(super) fn unresolved(directory: &Path) -> Result<Vec<Owner>> {
    let entries = match std::fs::read_dir(directory.join(CLAIMS_FOLDER)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(Error::Corrupt),
    };
    let mut owners = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= MAX_RECORDS {
            return Err(Error::Corrupt);
        }
        let path = entry.map_err(|_| Error::Corrupt)?.path();
        let bytes =
            retold_engine::fsx::read_bounded(&path, MAX_BYTES).map_err(|_| Error::Corrupt)?;
        let Some(bytes) = bytes else {
            continue;
        };
        let file = match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(Error::Corrupt),
        };
        match file.try_lock() {
            Ok(()) => owners.push(serde_json::from_slice(&bytes).map_err(|_| Error::Corrupt)?),
            Err(std::fs::TryLockError::WouldBlock) => (),
            Err(_) => return Err(Error::Corrupt),
        }
    }
    Ok(owners)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abandoned_claim_blocks_admission_and_maintenance_without_pid_checks() {
        let root = tempfile::tempdir().unwrap();
        let activity = super::super::barrier::join(root.path(), "test supervisor").unwrap();
        let claim = Claim::begin(root.path()).unwrap();
        assert_eq!(unresolved(root.path()).unwrap(), Vec::<Owner>::new());
        drop(claim);
        drop(activity);
        assert!(matches!(
            super::super::barrier::join(root.path(), "replacement"),
            Err(Error::Occupied(_))
        ));
        assert!(matches!(
            super::super::barrier::exclusive(root.path()),
            Err(Error::Occupied(_))
        ));
        assert_eq!(unresolved(root.path()).unwrap().len(), 1);
    }

    #[test]
    fn confirmed_completion_reopens_maintenance() {
        let root = tempfile::tempdir().unwrap();
        let activity = super::super::barrier::join(root.path(), "test supervisor").unwrap();
        Claim::begin(root.path()).unwrap().complete().unwrap();
        drop(activity);
        assert!(super::super::barrier::exclusive(root.path()).is_ok());
    }

    #[test]
    fn malformed_claim_fails_closed_without_removing_evidence() {
        let root = tempfile::tempdir().unwrap();
        let claim = Claim::begin(root.path()).unwrap();
        let path = claim.path.clone();
        drop(claim);
        std::fs::write(&path, b"invalid").unwrap();
        assert!(matches!(
            super::super::barrier::exclusive(root.path()),
            Err(Error::Corrupt)
        ));
        assert_eq!(std::fs::read(path).unwrap(), b"invalid");
    }
}
