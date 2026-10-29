//! One registered library's managed files plus canonical financial recovery evidence.
use super::{Entry, Index, files::Selection, invalid, publication, semantics::Lineage, verify};
use crate::account::{Account, backup, commands, types::Workspace};
use crate::library::Catalog;
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::path::Path;

const FINANCIAL_FILES: [&str; 2] = ["account/backup.json", "account/accounting.sqlite"];
const OMISSIONS: &[&str] = &[
    "provider settings, secrets and login profiles",
    "locks, sockets, executable hooks and models",
    "unmanaged files outside run folders and shared cast",
    "external source originals and global configuration",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Preview,
    Create,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    Workspace,
    SelectedVideo(&'a str),
    AllWorkspaces,
}

#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub root: &'a Path,
    pub output: &'a Path,
    pub mode: Mode,
    pub target: Target<'a>,
}

struct Source {
    workspace: Workspace,
    catalog: Catalog,
    selection: Selection,
}

#[must_use]
pub fn respond(root: &Path, output: &Path, mode: Mode) -> Payload {
    let request = Request {
        root,
        output,
        mode,
        target: Target::Workspace,
    };
    respond_request(request)
}

#[must_use]
pub fn respond_request(request: Request<'_>) -> Payload {
    let result = canonical_account().and_then(|account| apply(&account, request));
    match result {
        Ok(contents) => Payload::Result {
            outcome: Outcome::Completed,
            contents,
        },
        Err(error) => error.into_payload(),
    }
}

/// Explicit accounts are for isolated application callers; CLI always uses the canonical one.
pub fn apply(account: &Account, request: Request<'_>) -> Result<serde_json::Value, Diagnostic> {
    let is_all = matches!(request.target, Target::AllWorkspaces);
    if is_all {
        apply_all(account, request.output, request.mode)
    } else {
        apply_single(account, request)
    }
}

fn apply_single(account: &Account, request: Request<'_>) -> Result<serde_json::Value, Diagnostic> {
    match request.mode {
        Mode::Preview => preview(account, request),
        Mode::Create => create(account, request),
    }
}

fn apply_all(
    account: &Account,
    output: &Path,
    mode: Mode,
) -> Result<serde_json::Value, Diagnostic> {
    match mode {
        Mode::Preview => preview_all(account, output),
        Mode::Create => create_all(account, output),
    }
}

fn canonical_account() -> Result<Account, Diagnostic> {
    let directory =
        crate::account::canonical_directory().map_err(|error| commands::diagnostic(&error))?;
    Account::open(&directory).map_err(|error| commands::diagnostic(&error))
}

fn discover(
    account: &Account,
    root: &Path,
    target: Target<'_>,
) -> Result<(Workspace, Catalog), Diagnostic> {
    let workspace = account
        .backup_workspace(root)
        .map_err(|error| commands::diagnostic(&error))?;
    let mut catalog = crate::discovery::scan(&workspace.root)?;
    let complete = catalog.issues.is_empty();
    if !complete {
        return Err(invalid());
    }
    if let Target::SelectedVideo(video_id) = target {
        let has_video = catalog.videos.iter().any(|video| video.id == video_id);
        if !has_video {
            return Err(Diagnostic::new(
                ErrorCode::RefNotFound,
                "Selected video does not exist in workspace catalog.",
            ));
        }
        catalog.videos.retain(|video| video.id == video_id);
    }
    Ok((workspace, catalog))
}

fn source(workspace: Workspace, catalog: Catalog) -> Result<Source, Diagnostic> {
    let selection = Selection::scan(&catalog)?;
    Ok(Source {
        workspace,
        catalog,
        selection,
    })
}

fn preview(account: &Account, request: Request<'_>) -> Result<serde_json::Value, Diagnostic> {
    let (workspace, catalog) = discover(account, request.root, request.target)?;
    let source = source(workspace, catalog)?;
    let output = publication::destination(request.output, &source.workspace.root)?;
    let financial = account
        .preview_backup(&output)
        .map_err(|error| commands::diagnostic(&error))?;
    validate_runs(&source.catalog)?;
    Ok(serde_json::json!({
        "text": "Read-only backup preview. Includes private scripts/media and full account cost metadata; excludes provider credentials. No files written.",
        "backup_preview": { "workspace": source.workspace, "account": financial,
            "files": source.selection.files, "workspace_bytes": source.selection.total_bytes,
            "omitted_paths": source.selection.omitted, "omissions": OMISSIONS, "live_snapshot": true }
    }))
}

fn create(account: &Account, request: Request<'_>) -> Result<serde_json::Value, Diagnostic> {
    let maintenance = account
        .maintenance()
        .map_err(|error| commands::diagnostic(&error))?;
    let (workspace, catalog) = discover(account, request.root, request.target)?;
    let output = publication::destination(request.output, &workspace.root)?;
    account
        .preview_backup(&output)
        .map_err(|error| commands::diagnostic(&error))?;
    let _runs = lock_runs(&catalog)?;
    validate_runs(&catalog)?;
    let source = source(workspace, catalog)?;
    let staging = publication::staging(&output)?;
    let financial = maintenance
        .snapshot(&staging.path().join("account"))
        .map_err(|error| commands::diagnostic(&error))?;
    capture(&source, staging.path(), &financial)?;
    let verified = verify(staging.path())?;
    publication::publish(staging.path(), &output)?;
    Ok(
        serde_json::json!({ "text": "Workspace backup created and verified. Execution remains unauthorized.",
        "backup_created": { "output": output, "verified": verified } }),
    )
}

fn validate_registered_workspaces(workspaces: &[Workspace]) -> Result<(), Diagnostic> {
    for workspace in workspaces {
        let is_dir = workspace.root.is_dir();
        if !is_dir {
            return Err(Diagnostic::new(
                ErrorCode::NotAvailable,
                "A registered workspace root is missing or inaccessible. Multi-workspace backup failed closed.",
            ));
        }
    }
    Ok(())
}

fn preview_all(account: &Account, output: &Path) -> Result<serde_json::Value, Diagnostic> {
    let workspaces = account
        .registered_workspaces()
        .map_err(|error| commands::diagnostic(&error))?;
    validate_registered_workspaces(&workspaces)?;
    let financial = account
        .preview_backup(output)
        .map_err(|error| commands::diagnostic(&error))?;
    let mut total_files: usize = 0;
    let mut total_bytes: u64 = 0;
    for workspace in &workspaces {
        let (discovered_ws, catalog) = discover(account, &workspace.root, Target::Workspace)?;
        validate_runs(&catalog)?;
        let src = source(discovered_ws, catalog)?;
        total_files += src.selection.files.len();
        total_bytes += src.selection.total_bytes;
    }
    Ok(serde_json::json!({
        "text": "Read-only multi-workspace backup preview. Backs up all registered workspaces under account maintenance barrier.",
        "multi_backup_preview": {
            "account": financial,
            "workspace_count": workspaces.len(),
            "total_files": total_files,
            "total_bytes": total_bytes,
            "live_snapshot": true,
        }
    }))
}

fn create_all(account: &Account, output: &Path) -> Result<serde_json::Value, Diagnostic> {
    let workspaces = account
        .registered_workspaces()
        .map_err(|error| commands::diagnostic(&error))?;
    validate_registered_workspaces(&workspaces)?;
    let maintenance = account
        .maintenance()
        .map_err(|error| commands::diagnostic(&error))?;
    let staging = publication::staging(output)?;
    let financial = maintenance
        .snapshot(&staging.path().join("account"))
        .map_err(|error| commands::diagnostic(&error))?;
    let context = MultiBackupContext {
        account,
        staging_root: staging.path(),
        financial: &financial,
    };
    backup_each_workspace(&context, &workspaces)?;
    publication::publish(staging.path(), output)?;
    Ok(serde_json::json!({
        "text": "Multi-workspace backup created and verified for all registered workspaces under account barrier.",
        "multi_backup_created": {
            "output": output,
            "workspace_count": workspaces.len(),
        }
    }))
}

struct MultiBackupContext<'a> {
    account: &'a Account,
    staging_root: &'a Path,
    financial: &'a backup::Manifest,
}

fn backup_each_workspace(
    context: &MultiBackupContext<'_>,
    workspaces: &[Workspace],
) -> Result<(), Diagnostic> {
    for workspace in workspaces {
        backup_single_workspace(context, workspace)?;
    }
    Ok(())
}

fn backup_single_workspace(
    context: &MultiBackupContext<'_>,
    workspace: &Workspace,
) -> Result<(), Diagnostic> {
    let (discovered_ws, catalog) = discover(context.account, &workspace.root, Target::Workspace)?;
    let _runs = lock_runs(&catalog)?;
    validate_runs(&catalog)?;
    let src = source(discovered_ws, catalog)?;
    let ws_staging = context.staging_root.join(&src.workspace.id);
    std::fs::create_dir_all(&ws_staging).map_err(|_| invalid())?;
    copy_account_snapshot(context.staging_root, &ws_staging)?;
    capture(&src, &ws_staging, context.financial)?;
    verify(&ws_staging).map(|_| ())
}

fn copy_account_snapshot(staging_root: &Path, ws_staging: &Path) -> Result<(), Diagnostic> {
    let source_dir = staging_root.join("account");
    let target_dir = ws_staging.join("account");
    std::fs::create_dir_all(&target_dir).map_err(|_| invalid())?;
    copy_snapshot_files(&source_dir, &target_dir)
}

fn copy_snapshot_files(source_dir: &Path, target_dir: &Path) -> Result<(), Diagnostic> {
    for file_name in ["backup.json", "accounting.sqlite"] {
        std::fs::copy(source_dir.join(file_name), target_dir.join(file_name))
            .map_err(|_| invalid())?;
    }
    Ok(())
}

fn capture(
    source: &Source,
    staging: &Path,
    financial: &backup::Manifest,
) -> Result<(), Diagnostic> {
    capture_files(source, staging)?;
    let lineage = lineage(source);
    write_json(&staging.join("workspace.json"), &lineage)?;
    let index = index(source, staging, financial)?;
    write_json(&staging.join("index.json"), &index)
}

fn lineage(source: &Source) -> Lineage {
    Lineage {
        schema_version: 1,
        workspace: source.workspace.clone(),
        archive_root: "workspace".into(),
        account_snapshot: "account".into(),
        video_ids: source
            .catalog
            .videos
            .iter()
            .map(|video| video.id.clone())
            .collect(),
        omitted_paths: source.selection.omitted.clone(),
        execution_authorized: false,
    }
}

fn index(
    source: &Source,
    staging: &Path,
    financial: &backup::Manifest,
) -> Result<Index, Diagnostic> {
    let mut files = source
        .selection
        .files
        .iter()
        .map(|entry| Entry {
            path: format!("workspace/{}", entry.path),
            bytes: entry.bytes,
            blake3: entry.blake3.clone(),
        })
        .collect::<Vec<_>>();
    for path in FINANCIAL_FILES.into_iter().chain(["workspace.json"]) {
        files.push(index_entry(staging, path)?);
    }
    Ok(Index {
        schema_version: 2,
        format: "retold-workspace-backup".into(),
        workspace_id: source.workspace.id.clone(),
        account_id: financial.account_id.clone(),
        files,
        omissions: OMISSIONS.iter().map(|value| (*value).into()).collect(),
    })
}

fn index_entry(root: &Path, relative: &str) -> Result<Entry, Diagnostic> {
    let path = root.join(relative);
    let bytes = super::regular_file(&path)?;
    if bytes > super::MAX_TOTAL_BYTES {
        return Err(invalid());
    }
    Ok(Entry {
        path: relative.into(),
        bytes,
        blake3: super::digest(&path, bytes)?,
    })
}

pub(super) fn validate_runs(catalog: &Catalog) -> Result<(), Diagnostic> {
    use retold_engine::store::inspection::{Integrity, inspect};
    for video in &catalog.videos {
        let inspection = inspect(&video.layout).map_err(|_| invalid())?;
        let intact = inspection
            .steps
            .iter()
            .all(|step| step.integrity == Integrity::EnvelopeVerified);
        if !intact {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn lock_runs(catalog: &Catalog) -> Result<Vec<File>, Diagnostic> {
    catalog
        .videos
        .iter()
        .map(|video| lock_run(&video.layout.lock_file()))
        .collect()
}

fn lock_run(path: &Path) -> Result<File, Diagnostic> {
    if std::fs::symlink_metadata(path).is_ok() {
        super::regular_file(path)?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    options.create(true).truncate(false);
    let file = options.open(path).map_err(|_| invalid())?;
    file.try_lock().map_err(|_| {
        Diagnostic::new(
            ErrorCode::AccountBusy,
            "A run is still owned by another process. Stop or drain it before taking a backup.",
        )
    })?;
    Ok(file)
}

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Diagnostic> {
    use std::io::Write;
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| invalid())?;
    if bytes.len() as u64 > super::INDEX_BYTES {
        return Err(invalid());
    }
    let mut file = File::create_new(path).map_err(|_| invalid())?;
    file.write_all(&bytes).map_err(|_| invalid())?;
    file.sync_all().map_err(|_| invalid())
}

fn capture_files(source: &Source, staging: &Path) -> Result<(), Diagnostic> {
    let workspace = staging.join("workspace");
    std::fs::create_dir(&workspace).map_err(|_| invalid())?;
    source.selection.copy_to(&workspace)?;
    Ok(())
}
