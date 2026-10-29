//! Durable cooldown observations. Raw provider errors and credentials are never stored.
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use retold_providers::{ProviderError, ProvidersConfig};
use serde::{Deserialize, Serialize};

use crate::store::hash;

const LIMIT: u64 = 2 * 1024 * 1024;
const DEFAULT_WAIT: u64 = 120;

#[derive(Debug, Clone)]
pub struct HealthLedger {
    root: PathBuf,
    identities: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub kind: String,
    pub observed_at: u64,
    #[serde(default)]
    pub retry_at: u64,
    #[serde(default)]
    pub consecutive_failures: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    records: BTreeMap<String, Observation>,
}

impl HealthLedger {
    pub fn new(root: &Path, config: &ProvidersConfig) -> Result<Self, ProviderError> {
        let identities = config
            .providers
            .iter()
            .map(|(id, spec)| {
                fingerprint(spec)
                    .map(|fingerprint| (id.clone(), fingerprint))
                    .map_err(|_| storage())
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            root: root.into(),
            identities,
        })
    }

    pub fn observation(&self, id: &str) -> Result<Option<Observation>, ProviderError> {
        let state = self.read()?;
        let identity = self.identity(id)?;
        Ok(state.records.get(identity).cloned())
    }

    /// When a recorded cooldown for `id` ends, if it has not ended yet.
    pub fn cooldown_until(&self, id: &str) -> Result<Option<u64>, ProviderError> {
        let current = now()?;
        let observation = self.observation(id)?;
        Ok(observation
            .map(|record| record.retry_at)
            .filter(|&retry_at| retry_at > current))
    }

    fn identity(&self, id: &str) -> Result<&String, ProviderError> {
        self.identities.get(id).ok_or_else(storage)
    }

    fn read(&self) -> Result<State, ProviderError> {
        let file = self.root.join("provider-health.json");
        let bytes = crate::fsx::read_bounded(&file, LIMIT).map_err(|_| storage())?;
        let Some(bytes) = bytes else {
            return Ok(State {
                version: 1,
                records: BTreeMap::new(),
            });
        };
        let state: State = serde_json::from_slice(&bytes).map_err(|_| storage())?;
        let invalid = state.version != 1
            || state.records.values().any(|record| {
                (record.retry_at > 0 && record.retry_at < record.observed_at)
                    || !matches!(
                        record.kind.as_str(),
                        "limited" | "unavailable" | "logged_out" | "failing" | "ok"
                    )
            });
        if invalid {
            return Err(storage());
        }
        Ok(state)
    }

    fn record(&self, id: &str, observation: Observation) -> Result<(), ProviderError> {
        let _lock = self.writer()?;
        let mut state = self.read()?;
        let identity = self.identity(id)?;
        let prior = state.records.get(identity);
        let observation = merge_observation(observation, prior, now()?);
        state.records.insert(identity.clone(), observation);
        self.publish(&state)
    }

    fn publish(&self, state: &State) -> Result<(), ProviderError> {
        let bytes = serde_json::to_vec(&state).map_err(|_| storage())?;
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if size > LIMIT {
            return Err(storage());
        }
        crate::fsx::write_atomic(&self.root.join("provider-health.json"), &bytes)
            .map_err(|_| storage())
    }

    fn writer(&self) -> Result<File, ProviderError> {
        std::fs::create_dir_all(&self.root).map_err(|_| storage())?;
        let path = self.root.join(".provider-health.lock");
        crate::fsx::read_bounded(&path, LIMIT).map_err(|_| storage())?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(false);
        options.read(true).write(true);
        let file = options.open(path).map_err(|_| storage())?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| storage())?;
        Ok(file)
    }
}

impl retold_providers::health::Health for HealthLedger {
    fn check(&self, id: &str) -> Result<(), ProviderError> {
        let observation = self.observation(id)?;
        if let Some(record) = observation {
            if record.kind == "logged_out" {
                return Err(ProviderError::Unavailable(
                    "provider is logged out; inspect providers health or login".into(),
                ));
            }
            let now = now()?;
            if record.retry_at > now {
                return Err(ProviderError::RateLimited {
                    message: "provider has a recorded cooldown; inspect providers health".into(),
                    retry_after: Some(std::time::Duration::from_secs(record.retry_at - now)),
                });
            }
        }
        Ok(())
    }

    fn failed(&self, id: &str, error: &ProviderError) -> Result<(), ProviderError> {
        let observed_at = now()?;
        let observation = build_failure_observation(error, observed_at);
        let Some(observation) = observation else {
            return Ok(());
        };
        self.record(id, observation)
    }

    fn succeeded(&self, id: &str) -> Result<(), ProviderError> {
        let observed_at = now()?;
        self.record(
            id,
            Observation {
                kind: "ok".into(),
                observed_at,
                retry_at: 0,
                consecutive_failures: 0,
            },
        )
    }
}

fn build_failure_observation(error: &ProviderError, observed_at: u64) -> Option<Observation> {
    match error {
        ProviderError::RateLimited { retry_after, .. } => {
            Some(build_cooldown_observation(*retry_after, observed_at))
        }
        ProviderError::Unavailable(message) if is_login_problem(message) => Some(Observation {
            kind: "logged_out".into(),
            observed_at,
            retry_at: 0,
            consecutive_failures: 0,
        }),
        ProviderError::Unavailable(_) => Some(Observation {
            kind: "unavailable".into(),
            observed_at,
            retry_at: observed_at.saturating_add(DEFAULT_WAIT),
            consecutive_failures: 0,
        }),
        ProviderError::Failed(_) | ProviderError::Timeout(_) | ProviderError::Malformed(_) => {
            Some(Observation {
                kind: "failing".into(),
                observed_at,
                retry_at: 0,
                consecutive_failures: 1,
            })
        }
        _ => None,
    }
}

fn merge_observation(
    mut latest: Observation,
    prior: Option<&Observation>,
    current: u64,
) -> Observation {
    if let Some(prior) = prior {
        // Keep an active admission deadline without discarding the latest outcome.
        if prior.retry_at > current {
            latest.retry_at = latest.retry_at.max(prior.retry_at);
        }
        if latest.kind == "failing" {
            latest.consecutive_failures = prior.consecutive_failures.saturating_add(1);
        }
    }
    latest
}

fn build_cooldown_observation(
    retry_after: Option<std::time::Duration>,
    observed_at: u64,
) -> Observation {
    let wait_secs = retry_after.map_or(DEFAULT_WAIT, |duration| duration.as_secs().max(1));
    Observation {
        kind: "limited".into(),
        observed_at,
        retry_at: observed_at.saturating_add(wait_secs),
        consecutive_failures: 0,
    }
}

pub(crate) fn is_login_problem(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "not logged in",
        "please log in",
        "/login",
        "unauthorized",
        "authentication",
        "invalid api key",
        "401",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

fn now() -> Result<u64, ProviderError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| storage())
}

fn storage() -> ProviderError {
    ProviderError::Unavailable(
        "Provider health storage is unavailable, busy or invalid; dispatch refused.".into(),
    )
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub id: String,
    pub status: &'static str,
    pub observation: Option<Observation>,
}

pub fn inspect(root: &Path) -> Result<Vec<Summary>, crate::provider_settings::AddError> {
    use crate::provider_settings::AddError;
    let config = crate::provider_settings::load(root)?;
    let ledger = HealthLedger::new(root, &config)
        .map_err(|_| AddError::Storage("Could not inspect provider health."))?;
    let state = ledger
        .read()
        .map_err(|_| AddError::Storage("Provider health storage is invalid or unavailable."))?;
    let current = now().map_err(|_| AddError::Storage("Could not read the current time."))?;
    config
        .providers
        .keys()
        .map(|id| {
            let identity = ledger.identity(id).map_err(|_| {
                AddError::Storage("Provider health storage is invalid or unavailable.")
            })?;
            let observation = state.records.get(identity).cloned();
            let status = classify_status(observation.as_ref(), current);
            Ok(Summary {
                id: id.clone(),
                status,
                observation,
            })
        })
        .collect()
}

fn classify_status(observation: Option<&Observation>, current: u64) -> &'static str {
    match observation {
        Some(record)
            if matches!(record.kind.as_str(), "limited" | "ok") && record.retry_at > current =>
        {
            "cooldown"
        }
        Some(record) if record.kind == "limited" => "cooldown_elapsed",
        Some(record) if record.kind == "logged_out" => "logged_out",
        Some(record) if record.kind == "failing" => "failing",
        Some(record) if record.kind == "ok" => "ok",
        Some(record) if record.kind == "unavailable" => "unavailable",
        Some(_) => "cooldown_elapsed",
        None => "not_checked",
    }
}

fn fingerprint(spec: &retold_providers::ProviderConfig) -> Result<String, crate::EngineError> {
    let mut value =
        serde_json::to_value(spec).map_err(|error| crate::EngineError::Setup(error.to_string()))?;
    if let Some(fields) = value.as_object_mut() {
        for setting in [
            "max_parallel",
            "timeout_secs",
            "tiers",
            "max_tokens",
            "context_size",
            "binary",
        ] {
            fields.remove(setting);
        }
    }
    hash(&value)
}
