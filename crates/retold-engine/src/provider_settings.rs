//! Provider configuration writes are serialized and never construct a provider.
pub mod reviewed;

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use retold_providers::ProvidersConfig;
use retold_providers::config::ProviderConfig;
use serde::Serialize;

const CONFIG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ID_BYTES: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum AddError {
    #[error("Provider settings changed. Refresh and review the change again; your draft is kept.")]
    Changed,
    #[error("{0}")]
    Invalid(&'static str),
    #[error("{0}")]
    Storage(&'static str),
    #[error("Another provider configuration writer is active. Retry after it finishes.")]
    Busy,
}

#[derive(Debug, Serialize)]
pub struct Added {
    pub id: String,
    pub dry_run: bool,
    pub backup: Option<PathBuf>,
}

/// Where a provider definition comes from: a TOML file, or the same TOML built from options.
#[derive(Debug, Clone, Copy)]
pub enum Definition<'a> {
    File(&'a Path),
    Inline(&'a str),
}

/// Explicit provider import options; preview never acquires a writer lock.
#[derive(Debug)]
pub struct AddRequest<'a> {
    pub root: &'a Path,
    pub id: &'a str,
    pub definition: Definition<'a>,
    pub dry_run: bool,
}

impl<'a> AddRequest<'a> {
    #[must_use]
    pub const fn new(root: &'a Path, id: &'a str, definition: &'a Path) -> Self {
        Self {
            root,
            id,
            definition: Definition::File(definition),
            dry_run: false,
        }
    }

    /// A definition given as TOML text, as `providers add --kind` builds it.
    #[must_use]
    pub const fn inline(root: &'a Path, id: &'a str, definition: &'a str) -> Self {
        Self {
            root,
            id,
            definition: Definition::Inline(definition),
            dry_run: false,
        }
    }

    #[must_use]
    pub const fn preview(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

/// Import one provider definition. Existing aliases and routes are preserved.
pub fn add(request: &AddRequest<'_>) -> Result<Added, AddError> {
    let AddRequest {
        root,
        id,
        definition,
        dry_run,
    } = *request;
    validate_id(id)?;
    let provider = read_definition(definition)?;
    let edit = Edit { root, dry_run };
    let backup = update(&edit, |original| updated_config(original, id, provider))?;
    Ok(Added {
        id: id.to_owned(),
        dry_run,
        backup,
    })
}

fn validate_id(id: &str) -> Result<(), AddError> {
    let valid = !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte));
    if !valid {
        return Err(AddError::Invalid(
            "Provider IDs must use 1-64 ASCII letters, digits, hyphens or underscores.",
        ));
    }
    Ok(())
}

fn read_file(path: &Path) -> Result<Option<Vec<u8>>, AddError> {
    crate::fsx::read_bounded(path, CONFIG_BYTES).map_err(|_| {
        AddError::Invalid(
            "Expected a readable, bounded regular configuration file; symlinks are refused.",
        )
    })
}

fn read_definition(definition: Definition<'_>) -> Result<ProviderConfig, AddError> {
    match definition {
        Definition::File(path) => {
            let bytes =
                read_file(path)?.ok_or(AddError::Invalid("Provider definition is missing."))?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| AddError::Invalid("Provider definition must be UTF-8 TOML."))?;
            parse_definition(text)
        }
        Definition::Inline(text) => parse_definition(text),
    }
}

fn parse_definition(text: &str) -> Result<ProviderConfig, AddError> {
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > CONFIG_BYTES {
        return Err(AddError::Invalid(
            "Provider definition exceeds the input size limit.",
        ));
    }
    let provider: ProviderConfig = toml::from_str(text).map_err(|_| {
        AddError::Invalid("Provider definition does not match the typed provider schema.")
    })?;
    let parallel = provider.max_parallel();
    if parallel == 0 {
        return Err(AddError::Invalid(
            "Provider concurrency must be at least 1.",
        ));
    }
    Ok(provider)
}

fn updated_config(
    original: Option<&[u8]>,
    id: &str,
    provider: ProviderConfig,
) -> Result<Vec<u8>, AddError> {
    let config = existing(original)?;
    let config = insert_provider(config, id, provider)?;
    serialize(&config)
}

pub(crate) fn serialize(config: &ProvidersConfig) -> Result<Vec<u8>, AddError> {
    let serialized = config
        .to_toml()
        .map_err(|_| AddError::Invalid("Could not serialize provider configuration."))?;
    parse(&serialized).map_err(AddError::Invalid)?;
    let size = u64::try_from(serialized.len()).unwrap_or(u64::MAX);
    if size > CONFIG_BYTES {
        return Err(AddError::Invalid(
            "Updated configuration exceeds the file size limit.",
        ));
    }
    Ok(serialized.into_bytes())
}

pub(crate) fn parse(text: &str) -> Result<ProvidersConfig, &'static str> {
    let config: ProvidersConfig =
        toml::from_str(text).map_err(|_| "providers.toml does not match the provider schema")?;
    if config.schema_version != retold_providers::config::SCHEMA_VERSION {
        return Err("providers.toml has an unsupported schema version");
    }
    let problems = config.problems();
    if !problems.is_empty() {
        return Err("providers.toml has invalid routes or concurrency limits");
    }
    Ok(config)
}

fn writer_lock(root: &Path) -> Result<File, AddError> {
    let path = root.join(".providers.lock");
    read_file(&path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    let lock = options
        .truncate(false)
        .open(path)
        .map_err(|_| AddError::Storage("Could not open the provider writer lock."))?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| AddError::Busy)?;
    Ok(lock)
}

fn publish(
    root: &Path,
    original: Option<&[u8]>,
    updated: &[u8],
) -> Result<Option<PathBuf>, AddError> {
    let backup = original.map(|bytes| backup(root, bytes)).transpose()?;
    write_config(root, updated)?;
    Ok(backup)
}

fn write_config(root: &Path, bytes: &[u8]) -> Result<(), AddError> {
    crate::fsx::write_atomic(&root.join("providers.toml"), bytes).map_err(|_| {
        AddError::Storage(
            "Could not publish provider configuration. Inspect providers.toml before retrying.",
        )
    })
}

fn backup(root: &Path, bytes: &[u8]) -> Result<PathBuf, AddError> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AddError::Storage("Could not timestamp the configuration backup."))?
        .as_nanos();
    let destination = root.join(format!("providers.toml.backup-{stamp}"));
    let mut staged = tempfile::NamedTempFile::new_in(root)
        .map_err(|_| AddError::Storage("Could not stage the configuration backup."))?;
    staged
        .write_all(bytes)
        .and_then(|()| staged.as_file().sync_all())
        .map_err(|_| AddError::Storage("Could not flush the configuration backup."))?;
    staged
        .persist_noclobber(&destination)
        .map_err(|_| AddError::Storage("Could not publish the configuration backup."))?;
    File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| AddError::Storage("Could not flush the configuration folder."))?;
    Ok(destination)
}

/// A preview of a missing folder describes the change against no providers.
fn preview_root(root: &Path) -> Result<PathBuf, AddError> {
    if root.exists() {
        existing_root(root)
    } else {
        Ok(root.to_owned())
    }
}

/// Writes create a missing configuration folder.
fn created_root(root: &Path) -> Result<PathBuf, AddError> {
    std::fs::create_dir_all(root)
        .map_err(|_| AddError::Storage("Could not create the configuration folder."))?;
    existing_root(root)
}

fn existing_root(root: &Path) -> Result<PathBuf, AddError> {
    let root = root
        .canonicalize()
        .map_err(|_| AddError::Invalid("Select an existing configuration folder."))?;
    if !root.is_dir() {
        return Err(AddError::Invalid(
            "Select an existing configuration folder.",
        ));
    }
    Ok(root)
}

fn insert_provider(
    mut config: ProvidersConfig,
    id: &str,
    provider: ProviderConfig,
) -> Result<ProvidersConfig, AddError> {
    let duplicate = config.providers.contains_key(id);
    if duplicate {
        return Err(AddError::Invalid(
            "Provider ID already exists; existing providers are never overwritten.",
        ));
    }
    config.providers.insert(id.to_owned(), provider);
    Ok(config)
}

/// Shared provider-file transaction used by import and route editing.
pub(crate) struct Edit<'a> {
    pub root: &'a Path,
    pub dry_run: bool,
}

pub(crate) fn update(
    edit: &Edit<'_>,
    mutation: impl FnOnce(Option<&[u8]>) -> Result<Vec<u8>, AddError>,
) -> Result<Option<PathBuf>, AddError> {
    let root = if edit.dry_run {
        preview_root(edit.root)?
    } else {
        created_root(edit.root)?
    };
    let _lock = if edit.dry_run {
        None
    } else {
        Some(writer_lock(&root)?)
    };
    let original = read_file(&root.join("providers.toml"))?;
    let updated = mutation(original.as_deref())?;
    let backup = if edit.dry_run {
        None
    } else {
        publish(&root, original.as_deref(), &updated)?
    };
    Ok(backup)
}

/// Create the configuration folder and an empty `providers.toml` when either is missing.
/// Returns whether a file was written; an existing file is never touched.
pub fn ensure(root: &Path) -> Result<bool, AddError> {
    let root = created_root(root)?;
    let file = root.join("providers.toml");
    if read_file(&file)?.is_some() {
        return Ok(false);
    }
    let _lock = writer_lock(&root)?;
    if read_file(&file)?.is_some() {
        return Ok(false);
    }
    write_config(&root, &serialize(&ProvidersConfig::empty())?)?;
    Ok(true)
}

pub fn load(root: &Path) -> Result<ProvidersConfig, AddError> {
    let bytes = read_file(&root.join("providers.toml"))?;
    existing(bytes.as_deref())
}

/// Inventory distinguishes an unconfigured installation from an explicitly empty one.
pub(crate) fn load_inventory(root: &Path) -> Result<ProvidersConfig, AddError> {
    let bytes = read_file(&root.join("providers.toml"))?.ok_or(AddError::Invalid(
        "providers.toml is missing; run setup first.",
    ))?;
    existing(Some(&bytes))
}

/// A missing file is a fresh installation with no providers.
pub(crate) fn existing(bytes: Option<&[u8]>) -> Result<ProvidersConfig, AddError> {
    let Some(bytes) = bytes else {
        return Ok(ProvidersConfig::empty());
    };
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AddError::Invalid("providers.toml must be UTF-8."))?;
    parse(text).map_err(AddError::Invalid)
}

/// Remove only an unreferenced provider; routes are never rewritten implicitly.
#[derive(Debug)]
pub struct RemoveRequest<'a> {
    pub root: &'a Path,
    pub id: &'a str,
    pub dry_run: bool,
}

pub fn remove(request: &RemoveRequest<'_>) -> Result<Added, AddError> {
    let RemoveRequest { root, id, dry_run } = *request;
    validate_id(id)?;
    let edit = Edit { root, dry_run };
    let backup = update(&edit, |original| without_provider(original, id))?;
    Ok(Added {
        id: id.to_owned(),
        dry_run,
        backup,
    })
}

fn without_provider(original: Option<&[u8]>, id: &str) -> Result<Vec<u8>, AddError> {
    let mut config = existing(original)?;
    let exists = config.providers.contains_key(id);
    if !exists {
        return Err(AddError::Invalid("Provider ID does not exist."));
    }
    let chains = config.routing.values();
    let referenced = chains.flatten().any(|route| route.provider == id);
    if referenced {
        return Err(AddError::Invalid(
            "Provider is referenced by routing. Replace those routes before removal.",
        ));
    }
    config.providers.shift_remove(id);
    serialize(&config)
}

/// Replace an existing definition after checking all its current route assignments.
pub fn edit(request: &AddRequest<'_>) -> Result<Added, AddError> {
    let AddRequest {
        root,
        id,
        definition,
        dry_run,
    } = *request;
    validate_id(id)?;
    let provider = read_definition(definition)?;
    let edit = Edit { root, dry_run };
    let backup = update(&edit, |original| {
        let mut config = existing(original)?;
        validate_replacement(&config, id, &provider)?;
        config.providers.insert(id.to_owned(), provider);
        serialize(&config)
    })?;
    Ok(Added {
        id: id.to_owned(),
        dry_run,
        backup,
    })
}

fn validate_replacement(
    config: &ProvidersConfig,
    id: &str,
    provider: &ProviderConfig,
) -> Result<(), AddError> {
    let exists = config.providers.contains_key(id);
    if !exists {
        return Err(AddError::Invalid(
            "Provider ID does not exist; add it explicitly first.",
        ));
    }
    for (tier, chain) in &config.routing {
        let referenced = chain.iter().any(|route| route.provider == id);
        if referenced && !provider.supports_tier(*tier) {
            return Err(AddError::Invalid(
                "Replacement adapter does not support an existing route. Change routes explicitly first.",
            ));
        }
    }
    Ok(())
}
