//! Read-only offline discovery. Presence never grants dispatch authority.
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

#[must_use]
pub fn scan() -> Payload {
    let Some(base) = directories::BaseDirs::new() else {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "Could not locate the home folder.",
        )
        .into_payload();
    };
    let search_path = std::env::var_os("PATH").unwrap_or_default();
    match retold_engine::provider_discovery::scan(base.home_dir(), &search_path) {
        Ok(inventory) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "inventory": inventory,
                "text": "Offline scan complete. See JSON for tool paths, profile folders and credential-variable presence. Versions, logins, local model files, .env and Keychain were not inspected. No requests sent."
            }),
        },
        Err(message) => Diagnostic::new(ErrorCode::InputRequired, &message).into_payload(),
    }
}

pub use retold_engine::provider_settings::AddRequest;

#[must_use]
pub fn add(request: &AddRequest<'_>) -> Payload {
    use retold_engine::provider_settings;
    match provider_settings::add(request) {
        Ok(added) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"added": added, "text": if request.dry_run {
                "Provider definition is valid. Dry run: no files changed. Routes and authentication were not tested."
            } else {
                "Provider added. Existing routes were preserved. No provider requests sent."
            }}),
        },
        Err(error) => configuration_error(&error),
    }
}

pub use retold_engine::provider_routes::RouteRequest;

/// Every supported task tier, including tiers without a configured fallback chain.
#[must_use]
pub fn routing_tiers() -> Vec<&'static str> {
    retold_engine::provider_routes::tiers()
}

#[must_use]
pub fn routes(root: &std::path::Path) -> Payload {
    match retold_engine::provider_routes::show(root) {
        Ok(routing) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"routing": routing}),
        },
        Err(error) => configuration_error(&error),
    }
}

#[must_use]
pub fn set_route(request: &RouteRequest<'_>) -> Payload {
    match retold_engine::provider_routes::set(request) {
        Ok(route) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"route": route, "text": if request.dry_run {
                "Route is valid. Dry run: no files changed or requests sent."
            } else {
                "Route updated. Other routes and providers were preserved. No requests sent."
            }}),
        },
        Err(error) => configuration_error(&error),
    }
}

pub use retold_engine::provider_routes::SuggestRequest;

/// Routes every unrouted tier from the configured providers; a dry run only shows them.
#[must_use]
pub fn suggest_routes(request: SuggestRequest<'_>) -> Payload {
    match retold_engine::provider_routes::suggest(request) {
        Ok(suggested) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "suggested": suggested,
                "text": suggestion_report(&suggested),
            }),
        },
        Err(error) => configuration_error(&error),
    }
}

/// The outcome sentence, then one `tier: first -> fallback` line per suggested route.
fn suggestion_report(suggested: &retold_engine::provider_routes::Suggested) -> String {
    let routes = suggested
        .routes
        .iter()
        .map(|(tier, chain)| format!("  {tier}: {}", chain.join(" -> ")));
    std::iter::once(suggestion_text(suggested).to_owned())
        .chain(routes)
        .collect::<Vec<_>>()
        .join("\n")
}

fn suggestion_text(suggested: &retold_engine::provider_routes::Suggested) -> &'static str {
    match (suggested.routes.is_empty(), suggested.dry_run) {
        (true, _) => {
            "Nothing to suggest: every tier is routed, or no configured provider suits the rest. Paid models, voices and transcription are chosen with providers route."
        }
        (false, true) => {
            "Suggested routes for unrouted tiers. Dry run: nothing was saved and no requests were sent."
        }
        (false, false) => {
            "Suggested routes saved for unrouted tiers. Existing routes were kept. No requests sent."
        }
    }
}

fn configuration_error(error: &retold_engine::provider_settings::AddError) -> Payload {
    use retold_engine::provider_settings::AddError;
    let code = match error {
        AddError::Changed => ErrorCode::PlanChanged,
        AddError::Invalid(_) => ErrorCode::InputRequired,
        AddError::Storage(_) | AddError::Busy => ErrorCode::ExecutionFailed,
    };
    Diagnostic::new(code, &error.to_string()).into_payload()
}

pub use retold_engine::provider_settings::RemoveRequest;

#[must_use]
pub fn remove(request: &RemoveRequest<'_>) -> Payload {
    match retold_engine::provider_settings::remove(request) {
        Ok(removed) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"removed": removed, "text": if request.dry_run {
                "Provider can be removed. Dry run: no files changed."
            } else {
                "Provider removed. Routes, credentials and provider files were preserved."
            }}),
        },
        Err(error) => configuration_error(&error),
    }
}

#[must_use]
pub fn edit(request: &AddRequest<'_>) -> Payload {
    match retold_engine::provider_settings::edit(request) {
        Ok(edited) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"edited": edited, "text": if request.dry_run {
                "Replacement definition is valid. Dry run: no files changed. Models and logins were not tested."
            } else {
                "Provider definition replaced. Existing routes were preserved. No requests sent."
            }}),
        },
        Err(error) => configuration_error(&error),
    }
}

#[must_use]
pub fn list(root: &std::path::Path) -> Payload {
    match retold_engine::provider_inventory::list(root) {
        Ok(providers) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"providers": providers}),
        },
        Err(error) => configuration_error(&error),
    }
}

#[must_use]
pub fn health(root: &std::path::Path) -> Payload {
    match retold_engine::provider_health::inspect(root) {
        Ok(health) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"health": health.iter().map(health_summary).collect::<Vec<_>>()}),
        },
        Err(error) => configuration_error(&error),
    }
}

fn health_summary(summary: &retold_engine::provider_health::Summary) -> serde_json::Value {
    let observation = summary.observation.as_ref();
    serde_json::json!({
        "id": summary.id, "status": summary.status, "observation": observation,
        "observed_at_utc": observation.and_then(|record| utc_time(record.observed_at)),
        "retry_at_utc": observation.and_then(|record| utc_time(record.retry_at)),
    })
}

fn utc_time(seconds: u64) -> Option<String> {
    let recorded = seconds != 0;
    recorded
        .then_some(seconds)
        .and_then(|value| i64::try_from(value).ok())
        .and_then(|value| jiff::Timestamp::from_second(value).ok())
        .map(|value| value.to_string())
}

pub use retold_engine::provider_login::{LoginRequest, LoginResult, LoginStatus};

#[must_use]
pub fn login_status(root: &std::path::Path, id: &str) -> Payload {
    match retold_engine::provider_login::status(root, id) {
        Ok(login_status) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "login_status": login_status,
                "text": "Provider login status inspected offline. Credentials were not read into memory; no network requests sent."
            }),
        },
        Err(error) => configuration_error(&error),
    }
}

#[must_use]
pub fn login(request: &LoginRequest<'_>) -> Payload {
    match retold_engine::provider_login::configure(request) {
        Ok(result) => Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({
                "login_configured": result,
                "text": if request.dry_run {
                    "Provider login configuration is valid. Dry run: no files changed. No network requests sent."
                } else {
                    "Provider login configuration updated. Existing routes were preserved. No network requests sent."
                }
            }),
        },
        Err(error) => configuration_error(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::{health_summary, utc_time};
    use retold_engine::provider_health::{Observation, Summary};

    const OBSERVED: u64 = 1;
    const RETRY: u64 = 2;

    #[test]
    fn health_projection_preserves_raw_evidence_and_adds_exact_utc_times() {
        let summary = Summary {
            id: "example".into(),
            status: "cooldown",
            observation: Some(Observation {
                kind: "limited".into(),
                observed_at: OBSERVED,
                retry_at: RETRY,
                consecutive_failures: 1,
            }),
        };
        let projection = health_summary(&summary);
        assert_eq!(projection["status"], "cooldown");
        assert_eq!(projection["observation"]["retry_at"], RETRY);
        assert_eq!(projection["observed_at_utc"], "1970-01-01T00:00:01Z");
        assert_eq!(projection["retry_at_utc"], "1970-01-01T00:00:02Z");
    }

    #[test]
    fn unchecked_health_and_invalid_times_have_no_fabricated_timestamp() {
        let projection = health_summary(&Summary {
            id: "example".into(),
            status: "not_checked",
            observation: None,
        });
        assert_eq!(projection["status"], "not_checked");
        assert!(projection["observed_at_utc"].is_null());
        assert!(projection["retry_at_utc"].is_null());
        assert_eq!(utc_time(0), None);
        assert_eq!(utc_time(u64::MAX), None);
    }
}
