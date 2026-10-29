//! Explicit offline observations stay bound to the configuration they describe.
use super::{Snapshot, configuration_error, inspect};
use crate::protocol::{Diagnostic, ErrorCode};
use retold_engine::provider_health::Summary as HealthSummary;
pub use retold_engine::provider_login::LoginStatus;
use std::path::Path;

#[derive(Debug)]
pub struct ObservationSet {
    pub version: String,
    pub health: Vec<HealthObservation>,
    pub profiles: Vec<ProfileObservation>,
}

#[derive(Debug)]
pub struct HealthObservation {
    pub id: String,
    pub status: &'static str,
    pub last_result: Option<String>,
    pub observed_at: Option<String>,
    pub retry_at: Option<String>,
    pub consecutive_failures: u32,
}

#[derive(Debug)]
pub struct ProfileObservation {
    pub id: String,
    pub status: Result<LoginStatus, Diagnostic>,
}

pub fn observations(root: &Path, expected_version: &str) -> Result<ObservationSet, Diagnostic> {
    let snapshot = inspect(root)?;
    require_version(&snapshot, expected_version)?;
    let health = retold_engine::provider_health::inspect(root)
        .map_err(|error| configuration_error(&error))?;
    let profiles = profile_statuses(&snapshot);
    require_version(&inspect(root)?, expected_version)?;
    Ok(ObservationSet {
        version: expected_version.into(),
        health: health.into_iter().map(project_health).collect(),
        profiles,
    })
}

fn project_health(summary: HealthSummary) -> HealthObservation {
    let record = summary.observation.as_ref();
    HealthObservation {
        id: summary.id,
        status: summary.status,
        last_result: record.map(|record| record.kind.clone()),
        observed_at: record.and_then(|record| utc_time(record.observed_at)),
        retry_at: record.and_then(|record| utc_time(record.retry_at)),
        consecutive_failures: record.map_or(0, |record| record.consecutive_failures),
    }
}

fn utc_time(seconds: u64) -> Option<String> {
    let present = seconds != 0;
    present
        .then_some(seconds)
        .and_then(|value| i64::try_from(value).ok())
        .and_then(|value| jiff::Timestamp::from_second(value).ok())
        .map(|value| value.to_string())
}

fn profile_statuses(snapshot: &Snapshot) -> Vec<ProfileObservation> {
    snapshot
        .providers
        .iter()
        .filter(|provider| matches!(provider.kind, "codex_cli" | "claude_cli" | "openrouter"))
        .map(|provider| ProfileObservation {
            id: provider.id.clone(),
            status: retold_engine::provider_login::status(&snapshot.root, &provider.id)
                .map_err(|error| configuration_error(&error)),
        })
        .collect()
}

fn require_version(snapshot: &Snapshot, expected: &str) -> Result<(), Diagnostic> {
    if snapshot.version != expected {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "Provider settings changed during inspection. Refresh settings before inspecting them again.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{inspect, observations};
    use crate::desktop::providers::{Change, preview};
    #[test]
    fn profile_presence_never_claims_authentication_and_invalid_health_is_preserved() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let profile = root.join("profile");
        std::fs::create_dir(&profile).unwrap();
        let definition = format!(
            "kind = 'codex_cli'\nmax_parallel = 1\nconfig_dir = {:?}",
            profile.to_str().unwrap()
        );
        let snapshot = inspect(root).unwrap();
        let change = Change::Add {
            id: "fixture".into(),
            definition,
        };
        let review = preview(root, &snapshot.version, &change).unwrap();
        retold_engine::provider_settings::reviewed::apply(&review).unwrap();
        let snapshot = inspect(root).unwrap();
        let status = observations(root, &snapshot.version).unwrap();
        let login = status.profiles[0].status.as_ref().unwrap();
        assert_eq!(login.status, "profile_found");
        assert!(!login.authentication_checked);
        assert!(!login.authenticated);
        let health = root.join("provider-health.json");
        std::fs::write(&health, b"invalid saved health").unwrap();
        assert!(observations(root, &snapshot.version).is_err());
        assert_eq!(std::fs::read(&health).unwrap(), b"invalid saved health");
        assert!(!root.join(".provider-health.lock").exists());
    }
    #[test]
    fn unconfigured_and_changed_observations_stay_read_only() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("missing");
        let empty = inspect(&root).unwrap();
        let status = observations(&root, &empty.version).unwrap();
        assert!(status.health.is_empty());
        assert!(!root.exists());
        let change = Change::Add {
            id: "voice".into(),
            definition: "kind = 'local_voice'\nmax_parallel = 1".into(),
        };
        let review = preview(&root, &empty.version, &change).unwrap();
        retold_engine::provider_settings::reviewed::apply(&review).unwrap();
        assert!(observations(&root, &empty.version).is_err());
        let current = inspect(&root).unwrap();
        let status = observations(&root, &current.version).unwrap();
        assert_eq!(status.health[0].status, "not_checked");
        assert!(!root.join("provider-health.json").exists());
    }
}
