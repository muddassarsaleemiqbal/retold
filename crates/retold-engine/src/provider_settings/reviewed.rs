//! An immutable reviewed configuration proposal; apply rechecks exact source bytes under lock.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{AddError, Edit};
use retold_providers::ProvidersConfig;

#[derive(Debug, Clone)]
pub enum Change {
    ClearRoute { tier: String },
    Add { id: String, definition: String },
    Replace { id: String, definition: String },
    Remove { id: String },
    Route { tier: String, order: String },
}

#[derive(Debug)]
pub struct Snapshot {
    pub root: PathBuf,
    pub version: String,
    pub providers: Vec<crate::provider_inventory::ProviderSummary>,
    pub definitions: BTreeMap<String, String>,
    pub routing: BTreeMap<String, Vec<String>>,
}

/// Private authority fields prevent a frontend from replacing approved bytes or their scope.
#[derive(Debug, Clone)]
pub struct Review {
    root: PathBuf,
    expected_version: String,
    proposed: Vec<u8>,
    summary: String,
    before: String,
    after: String,
}

impl Review {
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }
    #[must_use]
    pub fn before(&self) -> &str {
        &self.before
    }
    #[must_use]
    pub fn after(&self) -> &str {
        &self.after
    }
}

pub fn inspect(root: &Path) -> Result<Snapshot, AddError> {
    let root = absolute_root(root)?;
    let bytes = super::read_file(&root.join("providers.toml"))?;
    let config = super::existing(bytes.as_deref())?;
    Ok(Snapshot {
        root,
        version: version(bytes.as_deref()),
        providers: crate::provider_inventory::project(&config),
        definitions: definitions(&config)?,
        routing: routing(&config),
    })
}

fn definitions(config: &ProvidersConfig) -> Result<BTreeMap<String, String>, AddError> {
    config
        .providers
        .iter()
        .map(|(id, provider)| {
            toml::to_string_pretty(provider)
                .map(|text| (id.clone(), text))
                .map_err(|_| AddError::Invalid("Could not read the provider definition."))
        })
        .collect()
}

fn routing(config: &ProvidersConfig) -> BTreeMap<String, Vec<String>> {
    config
        .routing
        .iter()
        .map(|(tier, chain)| {
            (
                tier.to_string(),
                chain.iter().map(ToString::to_string).collect(),
            )
        })
        .collect()
}

pub fn preview(root: &Path, expected: &str, change: &Change) -> Result<Review, AddError> {
    let root = absolute_root(root)?;
    let bytes = super::read_file(&root.join("providers.toml"))?;
    check_version(bytes.as_deref(), expected)?;
    let proposed = propose(bytes.as_deref(), change)?;
    Ok(Review {
        root,
        expected_version: expected.into(),
        before: source_text(bytes.as_deref()),
        after: source_text(Some(&proposed)),
        proposed,
        summary: describe(change),
    })
}

pub fn apply(review: &Review) -> Result<Option<PathBuf>, AddError> {
    if absolute_root(&review.root)? != review.root {
        return Err(AddError::Changed);
    }
    let edit = Edit {
        root: &review.root,
        dry_run: false,
    };
    super::update(&edit, |original| {
        check_version(original, &review.expected_version)?;
        Ok(review.proposed.clone())
    })
}

fn propose(original: Option<&[u8]>, change: &Change) -> Result<Vec<u8>, AddError> {
    let config = super::existing(original)?;
    match change {
        Change::Add { id, definition } => {
            super::validate_id(id)?;
            let provider = super::parse_definition(definition)?;
            super::serialize(&super::insert_provider(config, id, provider)?)
        }
        Change::Replace { id, definition } => replacement(config, id, definition),
        Change::Remove { id } => {
            super::validate_id(id)?;
            super::without_provider(original, id)
        }
        Change::Route { tier, order } => route(config, tier, order),
        Change::ClearRoute { tier } => clear_route(config, tier),
    }
}

fn replacement(mut config: ProvidersConfig, id: &str, text: &str) -> Result<Vec<u8>, AddError> {
    super::validate_id(id)?;
    let provider = super::parse_definition(text)?;
    super::validate_replacement(&config, id, &provider)?;
    config.providers.insert(id.into(), provider);
    super::serialize(&config)
}

fn route(config: ProvidersConfig, tier: &str, order: &str) -> Result<Vec<u8>, AddError> {
    let tier = tier
        .parse::<retold_core::Tier>()
        .map_err(|_| AddError::Invalid("Unknown routing tier."))?;
    let chain = crate::provider_routes::parse_order(order)?;
    super::serialize(&crate::provider_routes::replace(config, tier, chain)?)
}

fn clear_route(mut config: ProvidersConfig, tier: &str) -> Result<Vec<u8>, AddError> {
    let tier = tier
        .parse::<retold_core::Tier>()
        .map_err(|_| AddError::Invalid("Unknown routing tier."))?;
    config.routing.shift_remove(&tier);
    super::serialize(&config)
}

fn check_version(original: Option<&[u8]>, expected: &str) -> Result<(), AddError> {
    if version(original) != expected {
        return Err(AddError::Changed);
    }
    Ok(())
}

fn version(bytes: Option<&[u8]>) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"retold-provider-config-v1\0");
    if let Some(bytes) = bytes {
        hasher.update(b"present\0");
        hasher.update(bytes);
    } else {
        hasher.update(b"missing\0");
    }
    hasher.finalize().to_hex().to_string()
}

fn source_text(bytes: Option<&[u8]>) -> String {
    bytes.map_or_else(
        || "(No configuration file)".into(),
        |value| String::from_utf8_lossy(value).into_owned(),
    )
}

fn describe(change: &Change) -> String {
    match change {
        Change::Add { id, .. } => format!("Add provider {id}"),
        Change::Replace { id, .. } => format!("Replace provider {id}"),
        Change::Remove { id } => format!("Remove provider {id}"),
        Change::Route { tier, .. } => format!("Update {tier} fallback order"),
        Change::ClearRoute { tier } => format!("Remove {tier} route"),
    }
}

/// Resolve existing ancestors without creating directories, so a changed symlink cannot retarget apply.
fn absolute_root(root: &Path) -> Result<PathBuf, AddError> {
    let path = std::path::absolute(root)
        .map_err(|_| AddError::Invalid("Configuration path is unavailable."))?;
    match path.canonicalize() {
        Ok(path) if path.is_dir() => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or(AddError::Invalid("Invalid configuration path."))?;
            let name = path
                .file_name()
                .ok_or(AddError::Invalid("Invalid configuration path."))?;
            Ok(absolute_root(parent)?.join(name))
        }
        _ => Err(AddError::Invalid("Configuration folder is unavailable.")),
    }
}
