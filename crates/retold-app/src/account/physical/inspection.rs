//! Physical claims are a bounded live inventory, not a mutation or dispatch authority.
use super::super::{
    Error, Result,
    barrier::Owner,
    processes::{Page, Process, Selection, Status},
};
use super::record::{RECORD_VERSION, Record};
use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

pub(in crate::account) fn inspect(directory: &Path, selection: Selection<'_>) -> Result<Page> {
    let root = directory.join(super::CLAIMS_FOLDER);
    let names = names(&root)?;
    let selected = names
        .into_iter()
        .filter(|name| selection.after.is_none_or(|after| name.as_str() > after));
    page(&root, selected, selection.limit)
}

fn page(root: &Path, selected: impl Iterator<Item = String>, limit: usize) -> Result<Page> {
    let records = selected.map(|name| read(&root.join(&name), name));
    let records = records
        .filter_map(std::result::Result::transpose)
        .take(limit.saturating_add(1));
    let mut processes = records.collect::<Result<Vec<_>>>()?;
    let next_after = cursor(&processes, limit);
    processes.truncate(limit);
    Ok(Page {
        processes,
        next_after,
    })
}

fn cursor(processes: &[Process], limit: usize) -> Option<String> {
    let complete = processes.len() <= limit;
    if complete {
        return None;
    }
    processes
        .get(limit.saturating_sub(1))
        .map(|process| process.id.clone())
}

fn names(root: &Path) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(Error::Corrupt),
    };
    let mut names = Vec::new();
    for entry in entries {
        if names.len() >= super::MAX_RECORDS {
            return Err(Error::Corrupt);
        }
        let name = entry
            .map_err(|_| Error::Corrupt)?
            .file_name()
            .into_string()
            .map_err(|_| Error::Corrupt)?;
        names.push(name);
    }
    names.sort();
    Ok(names)
}

fn read(path: &Path, id: String) -> Result<Option<Process>> {
    let bytes =
        retold_engine::fsx::read_bounded(path, super::MAX_BYTES).map_err(|_| Error::Corrupt)?;
    let Some(_) = bytes else {
        return Ok(Option::default());
    };
    let file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Option::default()),
        Err(_) => return Err(Error::Corrupt),
    };
    let active = active(&file)?;
    let bytes =
        retold_engine::fsx::read_bounded(path, super::MAX_BYTES).map_err(|_| Error::Corrupt)?;
    let Some(bytes) = bytes else {
        return Ok(Option::default());
    };
    let decoded = decode(id.clone(), &bytes, active);
    match decoded {
        Ok(process) => Ok(Some(process)),
        Err(_) if matches!(active, Ownership::Held) => Ok(Some(updating(id))),
        Err(error) => Err(error),
    }
}

fn active(file: &File) -> Result<Ownership> {
    match file.try_lock() {
        Ok(()) => Ok(Ownership::Released),
        Err(TryLockError::WouldBlock) => Ok(Ownership::Held),
        Err(_) => Err(Error::Corrupt),
    }
}

fn decode(id: String, bytes: &[u8], active: Ownership) -> Result<Process> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| Error::Corrupt)?;
    let legacy = value.get("schema_version").is_none();
    if legacy {
        return legacy_record(id, value);
    }
    let record: Record = serde_json::from_value(value).map_err(|_| Error::Corrupt)?;
    if record.schema_version != RECORD_VERSION {
        return Err(Error::Corrupt);
    }
    Ok(current_record(id, record, active))
}

fn current_record(id: String, record: Record, active: Ownership) -> Process {
    let supervisor_observation = Some(retold_engine::process_supervision::identity::observe(
        &record.supervisor,
    ));
    let worker_observation = record
        .worker
        .as_ref()
        .map(retold_engine::process_supervision::identity::observe);
    Process {
        id,
        supervisor_observation,
        worker_observation,
        status: if matches!(active, Ownership::Held) {
            Status::Active
        } else {
            Status::Unconfirmed
        },
        owner: record.owner,
        schema_version: Some(record.schema_version),
        supervisor: Some(record.supervisor),
        worker: record.worker,
    }
}

fn updating(id: String) -> Process {
    Process {
        id,
        status: Status::Updating,
        owner: Owner {
            pid: 0,
            purpose: "Supervisor evidence is being written.".into(),
            since: jiff::Timestamp::UNIX_EPOCH,
        },
        schema_version: None,
        supervisor_observation: None,
        worker_observation: None,
        supervisor: None,
        worker: None,
    }
}

#[derive(Clone, Copy)]
enum Ownership {
    Held,
    Released,
}

fn legacy_record(id: String, value: serde_json::Value) -> Result<Process> {
    let owner = serde_json::from_value(value).map_err(|_| Error::Corrupt)?;
    Ok(Process {
        id,
        status: Status::LegacyUnconfirmed,
        owner,
        schema_version: None,
        supervisor_observation: None,
        worker_observation: None,
        supervisor: None,
        worker: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(limit: usize) -> Selection<'static> {
        Selection { after: None, limit }
    }

    #[test]
    fn empty_inventory_is_read_only_and_creates_no_claim_directory() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            inspect(root.path(), selection(1)).unwrap().processes.len(),
            0
        );
        assert!(!root.path().join(super::super::CLAIMS_FOLDER).exists());
    }

    #[test]
    fn native_claims_remain_unconfirmed_after_lock_loss_even_with_a_live_pid() {
        let root = tempfile::tempdir().unwrap();
        let claim = super::super::Claim::begin(root.path()).unwrap();
        let active = inspect(root.path(), selection(1)).unwrap();
        assert!(matches!(active.processes[0].status, Status::Active));
        assert_eq!(
            active.processes[0].supervisor_observation,
            Some(retold_engine::process_supervision::identity::Observation::Matching)
        );
        drop(claim);
        let stale = inspect(root.path(), selection(1)).unwrap();
        assert!(matches!(stale.processes[0].status, Status::Unconfirmed));
        assert_eq!(stale.processes[0].schema_version, Some(RECORD_VERSION));
        assert!(super::super::super::barrier::join(root.path(), "replacement").is_err());
    }

    #[test]
    fn legacy_pages_preserve_evidence_and_never_invent_native_identity() {
        let root = tempfile::tempdir().unwrap();
        let claims = root.path().join(super::super::CLAIMS_FOLDER);
        std::fs::create_dir(&claims).unwrap();
        let bytes = br#"{"pid":1,"purpose":"legacy","since":"1970-01-01T00:00:00Z"}"#;
        for name in ["a", "b", "c"] {
            std::fs::write(claims.join(name), bytes).unwrap();
        }
        let first = inspect(root.path(), selection(2)).unwrap();
        assert_eq!(first.processes.len(), 2);
        assert_eq!(first.next_after.as_deref(), Some("b"));
        assert!(matches!(
            first.processes[0].status,
            Status::LegacyUnconfirmed
        ));
        assert!(first.processes[0].supervisor.is_none());
        let next = inspect(
            root.path(),
            Selection {
                after: first.next_after.as_deref(),
                limit: 2,
            },
        )
        .unwrap();
        assert_eq!(next.processes[0].id, "c");
        assert!(next.next_after.is_none());
        assert_eq!(std::fs::read(claims.join("a")).unwrap(), bytes);
    }

    #[test]
    fn partial_live_records_are_updating_and_partial_abandoned_records_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let claim = super::super::Claim::begin(root.path()).unwrap();
        std::fs::write(&claim.path, b"{").unwrap();
        let active = inspect(root.path(), selection(1)).unwrap();
        assert!(matches!(active.processes[0].status, Status::Updating));
        let path = claim.path.clone();
        drop(claim);
        assert!(matches!(
            inspect(root.path(), selection(1)),
            Err(Error::Corrupt)
        ));
        assert_eq!(std::fs::read(path).unwrap(), b"{");
    }
}
