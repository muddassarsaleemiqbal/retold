//! Initialization logic for new workspaces.

use std::fs;
use std::path::Path;

use crate::account::Account;
use crate::account::types::Workspace;
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use crate::workspace::resolution::derive_workspace_paths;
use crate::workspace::types::{WORKSPACE_TOML_FILENAME, WorkspacePaths, WorkspaceToml};

const ID_PREFIX_LEN: usize = 8;
const HASH_SUFFIX_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceInitOptions {
    pub name: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitMode {
    Execute,
    DryRun,
}

impl WorkspaceInitOptions {
    #[must_use]
    pub const fn execution_mode(&self) -> InitMode {
        if self.dry_run {
            InitMode::DryRun
        } else {
            InitMode::Execute
        }
    }
}

fn compute_path_hash(name: &str, target: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(target.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn sanitize_prefix(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .take(ID_PREFIX_LEN)
        .collect::<String>()
        .to_ascii_lowercase()
}

fn generate_workspace_id(name: &str, target: &Path) -> String {
    let hash = compute_path_hash(name, target);
    let prefix = sanitize_prefix(name);
    let suffix = &hash[..HASH_SUFFIX_LEN];
    format!("ws-{prefix}-{suffix}")
}

fn resolve_name(target: &Path, explicit_name: Option<String>) -> String {
    explicit_name.unwrap_or_else(|| {
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("studio")
            .to_string()
    })
}

fn check_destination_available(target: &Path) -> Result<(), Diagnostic> {
    let reserved = [WORKSPACE_TOML_FILENAME, "config", "library"];
    let occupied = reserved
        .iter()
        .any(|name| fs::symlink_metadata(target.join(name)).is_ok());
    if occupied {
        return Err(Diagnostic::new(
            ErrorCode::DataInvalid,
            "Workspace files already exist here. Choose a new directory or inspect the existing workspace.",
        ));
    }
    Ok(())
}

fn verify_writable_space(target: &Path) -> Result<(), Diagnostic> {
    let writable = target
        .ancestors()
        .find_map(|ancestor| fs::metadata(ancestor).ok())
        .is_some_and(|metadata| metadata.is_dir() && !metadata.permissions().readonly());
    if writable {
        Ok(())
    } else {
        Err(Diagnostic::new(
            ErrorCode::ExecutionFailed,
            "Target location is missing or read-only. Check the parent directory.",
        ))
    }
}

fn create_workspace_directories(paths: &WorkspacePaths) -> Result<(), Diagnostic> {
    fs::create_dir_all(&paths.root).map_err(|_| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            "Failed to create workspace root.",
        )
    })?;
    fs::create_dir(&paths.config)
        .map_err(|_| Diagnostic::new(ErrorCode::ExecutionFailed, "Failed to create config dir."))?;
    fs::create_dir_all(paths.config.join("guides"))
        .map_err(|_| Diagnostic::new(ErrorCode::ExecutionFailed, "Failed to create guides dir."))?;
    fs::create_dir_all(paths.config.join("channels")).map_err(|_| {
        Diagnostic::new(ErrorCode::ExecutionFailed, "Failed to create channels dir.")
    })?;
    fs::create_dir(&paths.library)
        .map_err(|_| Diagnostic::new(ErrorCode::ExecutionFailed, "Failed to create library dir."))
}

fn write_workspace_manifests(
    paths: &WorkspacePaths,
    manifest: &WorkspaceToml,
) -> Result<(), Diagnostic> {
    let toml_content = toml::to_string_pretty(manifest).map_err(|_| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            "Failed to serialize workspace.toml.",
        )
    })?;
    persist_workspace_files(paths, &toml_content)
}

fn persist_workspace_files(paths: &WorkspacePaths, contents: &str) -> Result<(), Diagnostic> {
    persist_workspace_settings(&paths.config)?;
    let manifest = paths.root.join(WORKSPACE_TOML_FILENAME);
    retold_engine::fsx::write_atomic(&manifest, contents.as_bytes()).map_err(|_| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            "Failed to write workspace.toml.",
        )
    })
}

fn persist_workspace_settings(config: &Path) -> Result<(), Diagnostic> {
    retold_engine::fsx::write_atomic(&config.join("settings.toml"), b"# Workspace settings\n")
        .map_err(|_| Diagnostic::new(ErrorCode::ExecutionFailed, "Failed to write settings.toml."))
}

fn write_workspace_tree(
    paths: &WorkspacePaths,
    manifest: &WorkspaceToml,
) -> Result<(), Diagnostic> {
    create_workspace_directories(paths)?;
    write_workspace_manifests(paths, manifest)
}

fn try_register_workspace(workspace_id: &str, root: &Path) -> Result<(), Diagnostic> {
    let canonical_dir = crate::account::canonical_directory().map_err(|_| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            "Canonical account directory is unavailable.",
        )
    })?;
    let account = Account::open(&canonical_dir).map_err(|err| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to open account: {err}"),
        )
    })?;
    let entry = Workspace {
        id: workspace_id.to_string(),
        root: root.to_path_buf(),
    };
    account.register_workspace(entry).map_err(|err| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            &format!("Failed to register workspace with account: {err}"),
        )
    })
}

fn apply_initialization(
    paths: &WorkspacePaths,
    manifest: &WorkspaceToml,
    mode: InitMode,
) -> Result<(), Diagnostic> {
    if mode == InitMode::Execute {
        write_workspace_tree(paths, manifest)?;
        let _ = try_register_workspace(&manifest.id, &paths.root);
    }
    Ok(())
}

fn build_init_workspace_json(
    manifest: &WorkspaceToml,
    paths: &WorkspacePaths,
    mode: InitMode,
) -> serde_json::Value {
    serde_json::json!({
        "id": manifest.id,
        "name": manifest.name,
        "schema_version": manifest.schema_version,
        "root": paths.root,
        "config": paths.config,
        "library": paths.library,
        "dry_run": mode == InitMode::DryRun,
    })
}

fn format_init_payload(
    manifest: &WorkspaceToml,
    paths: &WorkspacePaths,
    mode: InitMode,
) -> Payload {
    let action_verb = if mode == InitMode::DryRun {
        "would initialize"
    } else {
        "initialized"
    };

    let text = format!(
        "Workspace '{}' ({}) {action_verb} at '{}'.",
        manifest.name,
        manifest.id,
        paths.root.display()
    );

    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "workspace": build_init_workspace_json(manifest, paths, mode),
        }),
    }
}

#[must_use]
pub fn init_workspace(target: &Path, options: &WorkspaceInitOptions) -> Payload {
    if let Err(diag) = check_destination_available(target) {
        return diag.into_payload();
    }
    if let Err(diag) = verify_writable_space(target) {
        return diag.into_payload();
    }

    let name = resolve_name(target, options.name.clone());
    let id = generate_workspace_id(&name, target);
    let paths = match derive_workspace_paths(target, &id) {
        Ok(derived) => derived,
        Err(diag) => return diag.into_payload(),
    };
    let manifest = WorkspaceToml::new(id, name);
    let mode = options.execution_mode();

    if let Err(diag) = apply_initialization(&paths, &manifest, mode) {
        return diag.into_payload();
    }

    format_init_payload(&manifest, &paths, mode)
}
