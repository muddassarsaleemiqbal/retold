//! The account-wide maintenance barrier (product spec §8.2). A mutation takes the admission mutex,
//! takes a shared activity lock, releases admission and keeps activity for its whole operation or
//! job. A backup takes admission and tries exclusive activity without waiting; while owners are
//! active it reports them and changes nothing. OS locks release on crash; durable physical
//! uncertainty remains a separate barrier until confirmed cleanup.
use super::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const LOCKS_NAME: &str = "locks";
const ADMISSION_NAME: &str = "admission";
const ACTIVITY_NAME: &str = "activity";
const OWNERS_NAME: &str = "owners";
/// Admission is only ever held briefly, so joiners poll for it rather than queue.
const WAIT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);
const MAX_OWNER_BYTES: u64 = 4096;

static RECORDS: AtomicU64 = AtomicU64::new(0);

/// Who holds shared activity, as recorded beside the lock for busy reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub pid: u32,
    pub purpose: String,
    pub since: jiff::Timestamp,
}

/// Shared activity for one operation or job. Dropping it releases the barrier.
#[derive(Debug)]
pub struct Token {
    record_path: PathBuf,
    record: File,
    activity: File,
}

impl Drop for Token {
    fn drop(&mut self) {
        // A forked child may briefly retain duplicate descriptors until exec.
        let _ = self.activity.unlock();
        let _ = self.record.unlock();
        let _ = std::fs::remove_file(&self.record_path);
    }
}

/// Exclusive activity with admission closed: no mutation can start until it is dropped.
#[derive(Debug)]
pub struct Exclusive {
    // Fields drop in order, so activity is released before admission reopens.
    activity: File,
    admission: File,
}

impl Drop for Exclusive {
    fn drop(&mut self) {
        // Closing alone cannot release locks retained by inherited descriptors.
        let _ = self.activity.unlock();
        let _ = self.admission.unlock();
    }
}

pub fn join(directory: &Path, purpose: &str) -> Result<Token> {
    let locks = locks(directory)?;
    let deadline = Instant::now() + WAIT;
    loop {
        let admission = open(&locks.join(ADMISSION_NAME))?;
        if acquired(&admission.try_lock())? {
            reject_unresolved(directory)?;
            let activity = open(&locks.join(ACTIVITY_NAME))?;
            if acquired(&activity.try_lock_shared())? {
                return register(&locks, activity, purpose);
            }
        }
        drop(admission);
        if Instant::now() >= deadline {
            return Err(Error::Busy);
        }
        std::thread::sleep(POLL);
    }
}

/// Closes admission and takes exclusive activity, or reports the active owners.
pub fn exclusive(directory: &Path) -> Result<Exclusive> {
    let locks = locks(directory)?;
    let admission = open(&locks.join(ADMISSION_NAME))?;
    let deadline = Instant::now() + WAIT;
    while !acquired(&admission.try_lock())? {
        if Instant::now() >= deadline {
            return Err(Error::Busy);
        }
        std::thread::sleep(POLL);
    }
    let activity = open(&locks.join(ACTIVITY_NAME))?;
    if acquired(&activity.try_lock())? {
        reject_unresolved(directory)?;
        sweep(&locks)?;
        return Ok(Exclusive {
            activity,
            admission,
        });
    }
    drop(admission);
    Err(Error::Occupied(owners(directory)?))
}

/// Live activity owners and unresolved durable physical claims. PIDs are diagnostic only.
pub fn owners(directory: &Path) -> Result<Vec<Owner>> {
    let records = directory.join(LOCKS_NAME).join(OWNERS_NAME);
    let mut owners = match std::fs::read_dir(&records) {
        Ok(entries) => collect_owners(entries)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => return Err(Error::Corrupt),
    };
    owners.extend(super::physical::unresolved(directory)?);
    Ok(owners)
}

fn collect_owners(entries: std::fs::ReadDir) -> Result<Vec<Owner>> {
    let mut owners = Vec::new();
    for entry in entries {
        let path = entry.map_err(|_| Error::Corrupt)?.path();
        if let Some(owner) = live(&path)? {
            owners.push(owner);
        }
    }
    owners.sort_by_key(|owner| owner.since);
    Ok(owners)
}

fn live(path: &Path) -> Result<Option<Owner>> {
    let Ok(file) = File::open(path) else {
        return Ok(Option::default());
    };
    if acquired(&file.try_lock())? {
        return Ok(Option::default());
    }
    let bytes = retold_engine::fsx::read_bounded(path, MAX_OWNER_BYTES)
        .ok()
        .flatten()
        .unwrap_or_default();
    let unknown = Owner {
        pid: 0,
        purpose: "unknown".into(),
        since: jiff::Timestamp::UNIX_EPOCH,
    };
    Ok(Some(serde_json::from_slice(&bytes).unwrap_or(unknown)))
}

/// Removes records whose owners are gone; only called with activity held exclusively.
fn sweep(locks: &Path) -> Result<()> {
    let entries = std::fs::read_dir(locks.join(OWNERS_NAME)).map_err(|_| Error::Corrupt)?;
    for entry in entries {
        let path = entry.map_err(|_| Error::Corrupt)?.path();
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// Records the owner while admission is still held, so a backup always sees it.
fn register(locks: &Path, activity: File, purpose: &str) -> Result<Token> {
    let sequence = RECORDS.fetch_add(1, Ordering::Relaxed);
    let since = jiff::Timestamp::now();
    let name = format!(
        "{}-{}-{sequence}.json",
        std::process::id(),
        since.as_nanosecond()
    );
    let record_path = locks.join(OWNERS_NAME).join(name);
    let mut record = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&record_path)
        .map_err(|_| Error::Corrupt)?;
    // A busy report may hold the record for a moment while checking that its owner is alive.
    record.lock().map_err(|_| Error::Corrupt)?;
    let owner = Owner {
        pid: std::process::id(),
        purpose: purpose.into(),
        since,
    };
    let contents = serde_json::to_vec(&owner).map_err(|_| Error::Corrupt)?;
    record.write_all(&contents).map_err(|_| Error::Corrupt)?;
    Ok(Token {
        record_path,
        record,
        activity,
    })
}

fn locks(directory: &Path) -> Result<PathBuf> {
    let locks = directory.join(LOCKS_NAME);
    std::fs::create_dir_all(locks.join(OWNERS_NAME)).map_err(|_| Error::Corrupt)?;
    Ok(locks)
}

fn open(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    options.create(true).truncate(false);
    options.open(path).map_err(|_| Error::Corrupt)
}

const fn acquired(attempt: &std::result::Result<(), TryLockError>) -> Result<bool> {
    match attempt {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(_)) => Err(Error::Corrupt),
    }
}

fn reject_unresolved(directory: &Path) -> Result<()> {
    let owners = super::physical::unresolved(directory)?;
    if owners.is_empty() {
        Ok(())
    } else {
        Err(Error::Occupied(owners))
    }
}

#[cfg(test)]
mod tests {
    use super::{exclusive, join};

    #[test]
    fn maintenance_drop_releases_locks_even_with_inherited_descriptors() {
        let directory = tempfile::tempdir().unwrap();
        let maintenance = exclusive(directory.path()).unwrap();
        let _inherited_activity = maintenance.activity.try_clone().unwrap();
        let _inherited_admission = maintenance.admission.try_clone().unwrap();
        drop(maintenance);
        assert!(exclusive(directory.path()).is_ok());
    }

    #[test]
    fn activity_drop_releases_inherited_lock_without_releasing_other_owners() {
        let directory = tempfile::tempdir().unwrap();
        let first = join(directory.path(), "finished operation").unwrap();
        let second = join(directory.path(), "independent operation").unwrap();
        let _inherited = first.activity.try_clone().unwrap();
        drop(first);
        assert!(exclusive(directory.path()).is_err());
        drop(second);
        assert!(exclusive(directory.path()).is_ok());
    }
}
