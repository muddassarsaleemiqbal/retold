//! Read-only schema and explicit configuration checks for user-owned resources.
use std::path::Path;

use retold_engine::Slug;
use retold_engine::retold_guide::{self, Guide, channel::Channel};

use super::invalid;
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

#[must_use]
pub fn schema(kind: &str) -> Payload {
    let schema = if kind == "guides" {
        retold_guide::guide_schema()
    } else {
        retold_guide::channel_schema()
    };
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({"schema": schema}),
    }
}

pub fn check(config: &Path, kind: &str, id: &str) -> Result<Payload, Diagnostic> {
    let slug: Slug = id
        .parse()
        .map_err(|_| invalid("Use a valid resource identifier."))?;
    let directory = config.join(format!("{kind}/{}", slug.as_str()));
    let pending = pending_configuration(&directory, kind)?;
    if !pending.is_empty() {
        return Err(unfinished(&pending));
    }
    Ok(checked_report(id))
}

fn checked_report(id: &str) -> Payload {
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": format!("{id}: schema and explicit configuration checks passed. Use doctor and video plan to check runtime readiness."),
            "id": id, "schema_valid": true, "configuration_complete": true,
        }),
    }
}

fn unfinished(pending: &[String]) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        ErrorCode::NotReady,
        "Schema is valid; replace CONFIGURE placeholders before production.",
    );
    diagnostic.context = serde_json::json!({"configuration_needed": pending});
    diagnostic
}

fn pending_configuration(directory: &Path, kind: &str) -> Result<Vec<String>, Diagnostic> {
    if kind == "guides" {
        let guide = Guide::from_dir(directory)
            .map_err(|error| invalid(format!("Guide validation failed: {error}")))?;
        return Ok(guide
            .files()
            .iter()
            .filter(|(_, value)| value.contains("CONFIGURE"))
            .map(|(name, _)| name.clone())
            .collect());
    }
    let channel = Channel::load(directory)
        .map_err(|error| invalid(format!("Channel validation failed: {error}")))?;
    let encoded = toml::to_string(channel.file())
        .map_err(|_| invalid("Could not inspect the channel configuration."))?;
    Ok(if encoded.contains("CONFIGURE") {
        vec!["channel.toml".into()]
    } else {
        Vec::new()
    })
}
