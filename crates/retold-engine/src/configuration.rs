//! Read-only configuration checks. Never constructs providers or writes defaults.
use std::path::Path;

/// Validate existing settings, provider routes, guides and channels without exposing file contents.
pub fn validate(root: &Path) -> Result<(), String> {
    let paths = crate::Paths {
        config: root.to_owned(),
        library: root.join("unused"),
    };
    paths
        .settings()
        .map_err(|_| "settings.toml is unreadable or invalid".to_owned())?;
    validate_providers(&paths)?;
    validate_catalogs(&paths)
}

fn validate_providers(paths: &crate::Paths) -> Result<(), String> {
    let file = paths.providers_file();
    match std::fs::read_to_string(&file) {
        Ok(text) => crate::provider_settings::parse(&text)
            .map(drop)
            .map_err(str::to_owned),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("providers.toml is unreadable".to_owned()),
    }
}

fn validate_catalogs(paths: &crate::Paths) -> Result<(), String> {
    for directory in [paths.guides(), paths.channels()] {
        match std::fs::read_dir(&directory) {
            Ok(entries) => {
                for entry in entries {
                    entry.map_err(|_| "Configuration directory could not be read".to_owned())?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Configuration directory could not be read".into()),
        }
    }
    let catalog = retold_guide::Catalog::load(Some(&paths.guides()))
        .map_err(|_| "The guide catalog could not be read".to_owned())?;
    let broken_guides = catalog.broken();
    if !broken_guides.is_empty() {
        return Err("One or more installed guides are invalid".into());
    }
    let (_, broken) = retold_guide::channel::list(&paths.channels());
    if !broken.is_empty() {
        return Err("One or more installed channels are invalid".into());
    }
    Ok(())
}

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct MigrationResult {
    pub migrated: bool,
    pub from_version: u32,
    pub to_version: u32,
    pub backup: Option<PathBuf>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationMode {
    Apply,
    DryRun,
}

pub fn migrate(root: &Path) -> Result<MigrationResult, String> {
    run_migration(root, MigrationMode::Apply)
}

pub fn dry_run_migrate(root: &Path) -> Result<MigrationResult, String> {
    run_migration(root, MigrationMode::DryRun)
}

struct MigrationContext<'a> {
    root: &'a Path,
    file: &'a Path,
    bytes: &'a [u8],
    mode: MigrationMode,
    from_version: u32,
    target_version: u32,
}

fn run_migration(root: &Path, mode: MigrationMode) -> Result<MigrationResult, String> {
    let file = root.join("providers.toml");
    cleanup_stale_staging(root);
    let bytes =
        std::fs::read(&file).map_err(|_| "providers.toml is missing or unreadable".to_owned())?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| "providers.toml must be UTF-8".to_owned())?;
    let (from_version, needs_migration) = detect_schema_version(text)?;
    let target_version = retold_providers::config::SCHEMA_VERSION;
    if !needs_migration {
        return Ok(MigrationResult {
            migrated: false,
            from_version,
            to_version: target_version,
            backup: None,
            dry_run: mode == MigrationMode::DryRun,
        });
    }
    let ctx = MigrationContext {
        root,
        file: &file,
        bytes: &bytes,
        mode,
        from_version,
        target_version,
    };
    apply_or_dry_run(&ctx, text)
}

fn apply_or_dry_run(ctx: &MigrationContext<'_>, text: &str) -> Result<MigrationResult, String> {
    let updated = update_schema_version(text, ctx.target_version)?;
    crate::provider_settings::parse(&updated)
        .map_err(|e| format!("Migrated configuration is invalid: {e}"))?;
    if ctx.mode == MigrationMode::DryRun {
        return Ok(MigrationResult {
            migrated: true,
            from_version: ctx.from_version,
            to_version: ctx.target_version,
            backup: None,
            dry_run: true,
        });
    }
    commit_migration(ctx, &updated)
}

fn commit_migration(ctx: &MigrationContext<'_>, updated: &str) -> Result<MigrationResult, String> {
    let backup_path = create_backup(ctx.root, ctx.bytes)?;
    match crate::fsx::write_atomic(ctx.file, updated.as_bytes()) {
        Ok(()) => Ok(MigrationResult {
            migrated: true,
            from_version: ctx.from_version,
            to_version: ctx.target_version,
            backup: Some(backup_path),
            dry_run: false,
        }),
        Err(err) => {
            let _ = std::fs::copy(&backup_path, ctx.file);
            Err(format!("Could not write migrated configuration: {err}"))
        }
    }
}

fn detect_schema_version(text: &str) -> Result<(u32, bool), String> {
    let value: toml::Value =
        toml::from_str(text).map_err(|_| "providers.toml is invalid TOML".to_owned())?;
    let target = retold_providers::config::SCHEMA_VERSION;
    let version = value
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        .map_or(0, |version_num| u32::try_from(version_num).unwrap_or(0));
    if version > target {
        return Err("providers.toml has an unsupported newer schema version".to_owned());
    }
    Ok((version, version < target))
}

fn update_schema_version(text: &str, target_version: u32) -> Result<String, String> {
    let mut value: toml::Value =
        toml::from_str(text).map_err(|_| "providers.toml is invalid TOML".to_owned())?;
    let table = value
        .as_table_mut()
        .ok_or_else(|| "providers.toml must be a TOML table".to_owned())?;
    table.insert(
        "schema_version".into(),
        toml::Value::Integer(i64::from(target_version)),
    );
    toml::to_string(&value).map_err(|_| "Could not serialize migrated configuration".to_owned())
}

fn create_backup(root: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "Could not timestamp configuration backup".to_owned())?
        .as_nanos();
    let destination = root.join(format!("providers.toml.backup-{stamp}"));
    crate::fsx::write_atomic(&destination, bytes)
        .map_err(|_| "Could not write configuration backup".to_owned())?;
    Ok(destination)
}

fn cleanup_stale_staging(root: &Path) {
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(".providers.toml.tmp") || name_str.ends_with(".tmp") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}
