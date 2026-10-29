//! Consistent account backups. A snapshot is taken only with the maintenance barrier held
//! exclusively, staged beside the destination and published by one rename, so a crash never
//! leaves a partial backup at the chosen path. Backups are recovery evidence: nothing here
//! installs one over the canonical account.
use super::barrier::{self, Owner};
use super::database::{self, DATABASE_NAME};
use super::types::{Balance, Unit};
use super::{Account, Error, Result, state::State, usage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const BACKUP_FORMAT: &str = "retold-account-backup";
pub const BACKUP_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "backup.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub const EXCLUDED_ITEMS: [&str; 4] = [
    "provider secrets and API keys",
    "provider login profiles",
    "locks and owner records",
    "workspace settings, manifests and media",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub account_id: String,
    pub timezone: String,
    pub created_at: jiff::Timestamp,
    pub database: Snapshot,
    pub contents: Contents,
    pub omitted: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub file: String,
    pub bytes: u64,
    pub blake3: String,
}

/// What the snapshot holds, recomputed from it on verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contents {
    pub workspaces: usize,
    pub jobs: usize,
    pub attempts: usize,
    pub unresolved: usize,
    pub limits: usize,
    pub usd_micros: Balance,
}

#[derive(Debug, Serialize)]
pub struct Published {
    pub output: PathBuf,
    pub manifest: Manifest,
}

/// What a backup would capture now, and who would make it wait.
#[derive(Debug, Serialize)]
pub struct Preview {
    pub output: PathBuf,
    pub account_id: String,
    pub contents: Contents,
    pub busy: Vec<Owner>,
    pub omitted: &'static [&'static str],
}

#[derive(Debug, Serialize)]
pub struct Verified {
    pub path: PathBuf,
    pub manifest: Manifest,
    /// Whether the backup came from this installation's account; unknown without one.
    pub same_account: Option<bool>,
}

impl Account {
    pub fn backup(&self, output: &Path) -> Result<Published> {
        let output = destination(output, &self.directory)?;
        let _exclusive = barrier::exclusive(&self.directory)?;
        let staging = staging(&output)?;
        let published = capture(&self.directory, &staging).and_then(|manifest| {
            std::fs::rename(&staging, &output)
                .map_err(|_| Error::Backup("Could not publish the backup; nothing was written."))?;
            Ok(manifest)
        });
        if published.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        Ok(Published {
            output,
            manifest: published?,
        })
    }

    /// Read-only: the current contents and owners; nothing is locked or written.
    pub fn preview_backup(&self, output: &Path) -> Result<Preview> {
        let output = destination(output, &self.directory)?;
        let state = database::read(&self.directory)?;
        Ok(Preview {
            output,
            account_id: state.account_id.clone(),
            contents: contents(&state)?,
            busy: barrier::owners(&self.directory)?,
            omitted: &EXCLUDED_ITEMS,
        })
    }
}

/// Checks a backup folder offline: manifest, checksum, schema and recorded contents.
pub fn verify(path: &Path, current: Option<&Account>) -> Result<Verified> {
    let manifest = manifest(path)?;
    verify_checksum(path, &manifest)?;
    let state = database::read(path)
        .map_err(|_| Error::BackupInvalid("The backup database is not valid accounting."))?;
    let consistent = matches_manifest(&state, &manifest)?;
    if !consistent {
        return Err(Error::BackupInvalid(
            "The backup database disagrees with its manifest.",
        ));
    }
    let same_account = current
        .map(|account| database::read(&account.directory))
        .transpose()?
        .map(|current| current.account_id == manifest.account_id);
    Ok(Verified {
        path: path.into(),
        manifest,
        same_account,
    })
}

fn verify_checksum(path: &Path, manifest: &Manifest) -> Result<()> {
    let database = path.join(DATABASE_NAME);
    let bytes = std::fs::read(&database)
        .map_err(|_| Error::BackupInvalid("The backup database is missing or unreadable."))?;
    let intact = u64::try_from(bytes.len()).ok() == Some(manifest.database.bytes)
        && blake3::hash(&bytes).to_hex().as_str() == manifest.database.blake3;
    if !intact {
        return Err(Error::BackupInvalid(
            "The backup database does not match its recorded checksum.",
        ));
    }
    Ok(())
}

fn matches_manifest(state: &State, manifest: &Manifest) -> Result<bool> {
    Ok(state.account_id == manifest.account_id
        && state.timezone == manifest.timezone
        && contents(state)? == manifest.contents)
}

fn manifest(path: &Path) -> Result<Manifest> {
    let unreadable = || Error::BackupInvalid("The backup manifest is missing or unreadable.");
    let bytes = retold_engine::fsx::read_bounded(&path.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)
        .map_err(|_| unreadable())?
        .ok_or_else(unreadable)?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| unreadable())?;
    let supported = manifest.format == BACKUP_FORMAT
        && manifest.version == BACKUP_VERSION
        && manifest.database.file == DATABASE_NAME;
    if !supported {
        return Err(Error::BackupInvalid(
            "This is not a supported account backup.",
        ));
    }
    Ok(manifest)
}

/// A new path in an existing folder, outside the account directory.
fn destination(output: &Path, account: &Path) -> Result<PathBuf> {
    let name = output
        .file_name()
        .ok_or(Error::Invalid("Name a new backup folder with --output."))?;
    let parent = match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let parent = parent
        .canonicalize()
        .map_err(|_| Error::Invalid("The backup's parent folder must exist."))?;
    let output = parent.join(name);
    if std::fs::symlink_metadata(&output).is_ok() {
        return Err(Error::Invalid(
            "The backup path already exists; backups never overwrite.",
        ));
    }
    let inside = account
        .canonicalize()
        .is_ok_and(|account| output.starts_with(account));
    if inside {
        return Err(Error::Invalid("Back up outside the account's own folder."));
    }
    Ok(output)
}

fn staging(output: &Path) -> Result<PathBuf> {
    let name = output.file_name().unwrap_or_default().to_string_lossy();
    let nonce = jiff::Timestamp::now().as_nanosecond();
    let staging = output.with_file_name(format!(".{name}.partial-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&staging)
        .map_err(|_| Error::Backup("Could not create the backup folder."))?;
    Ok(staging)
}

fn capture(directory: &Path, staging: &Path) -> Result<Manifest> {
    let database = staging.join(DATABASE_NAME);
    database::snapshot(directory, &database)?;
    let state = database::read(staging)?;
    let bytes =
        std::fs::read(&database).map_err(|_| Error::Backup("Could not read back the snapshot."))?;
    let manifest = build_manifest(&state, &bytes)?;
    write_manifest(staging, &manifest)?;
    Ok(manifest)
}

fn build_manifest(state: &State, bytes: &[u8]) -> Result<Manifest> {
    Ok(Manifest {
        format: BACKUP_FORMAT.into(),
        version: BACKUP_VERSION,
        account_id: state.account_id.clone(),
        timezone: state.timezone.clone(),
        created_at: jiff::Timestamp::now(),
        database: Snapshot {
            file: DATABASE_NAME.into(),
            bytes: u64::try_from(bytes.len()).map_err(|_| Error::Limit)?,
            blake3: blake3::hash(bytes).to_hex().to_string(),
        },
        contents: contents(state)?,
        omitted: EXCLUDED_ITEMS.map(String::from).into(),
    })
}

fn write_manifest(staging: &Path, manifest: &Manifest) -> Result<()> {
    let failed = |_| Error::Backup("Could not write the backup manifest.");
    let contents = serde_json::to_vec_pretty(manifest).map_err(|_| Error::Corrupt)?;
    let mut file = std::fs::File::create_new(staging.join(MANIFEST_FILE)).map_err(failed)?;
    file.write_all(&contents).map_err(failed)?;
    file.sync_all().map_err(failed)?;
    std::fs::File::open(staging.join(DATABASE_NAME))
        .and_then(|database| database.sync_all())
        .map_err(failed)
}

fn contents(state: &State) -> Result<Contents> {
    let (workspaces, jobs) = population(state);
    let (attempts, limits) = records(state);
    let unresolved = unresolved_count(state);
    let usd_micros = money_balance(state)?;
    Ok(Contents {
        workspaces,
        jobs,
        attempts,
        unresolved,
        limits,
        usd_micros,
    })
}

fn unresolved_count(state: &State) -> usize {
    state
        .attempts
        .values()
        .filter(|attempt| usage::unresolved(attempt))
        .count()
}

fn money_balance(state: &State) -> Result<Balance> {
    let lifetime: BTreeMap<Unit, Balance> = usage::summary(state)?.lifetime;
    Ok(lifetime.get(&Unit::UsdMicros).cloned().unwrap_or_default())
}

fn population(state: &State) -> (usize, usize) {
    (state.workspaces.len(), state.jobs.len())
}

fn records(state: &State) -> (usize, usize) {
    (state.attempts.len(), state.limits.len())
}

/// Keeps admission closed while the workspace and financial snapshot are captured together.
pub(crate) struct Maintenance<'a> {
    pub(super) account: &'a Account,
    _exclusive: barrier::Exclusive,
}

impl Account {
    pub(crate) fn maintenance(&self) -> Result<Maintenance<'_>> {
        Ok(Maintenance {
            account: self,
            _exclusive: barrier::exclusive(&self.directory)?,
        })
    }

    pub(crate) fn backup_workspace(&self, root: &Path) -> Result<super::types::Workspace> {
        let root = self.backup_root(root)?;
        let state = database::read(&self.directory)?;
        state
            .workspaces
            .into_values()
            .find(|workspace| workspace.root == root)
            .ok_or(Error::Invalid(
                "The selected library is not a registered workspace.",
            ))
    }
    fn backup_root(&self, root: &Path) -> Result<PathBuf> {
        let root = root
            .canonicalize()
            .map_err(|_| Error::Invalid("Workspace root must exist."))?;
        let account = self.directory.canonicalize().map_err(|_| Error::Corrupt)?;
        if root.starts_with(&account) || account.starts_with(&root) {
            return Err(Error::Invalid(
                "Workspace and account storage must be separate.",
            ));
        }
        Ok(root)
    }
}

impl Maintenance<'_> {
    pub(crate) fn snapshot(&self, path: &Path) -> Result<Manifest> {
        std::fs::create_dir(path)
            .map_err(|_| Error::Backup("Could not stage accounting evidence."))?;
        capture(&self.account.directory, path)
    }
}

/// Compares archived registration without resolving the original machine's filesystem paths.
pub(crate) fn contains_workspace(path: &Path, workspace: &super::types::Workspace) -> Result<bool> {
    let state = database::read(path)?;
    Ok(state.workspaces.get(&workspace.id) == Some(workspace))
}
