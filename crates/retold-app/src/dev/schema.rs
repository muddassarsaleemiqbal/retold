//! Versioned JSON schemas for machine integration, assistive tools, and validation.

use crate::protocol::{self, Diagnostic, ErrorCode, Outcome, Payload};

const SCHEMA_DRAFT_07: &str = "http://json-schema.org/draft-07/schema#";
const MANIFEST_SCHEMA_VERSION: u64 = 1;
const WORKSPACE_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemaKind {
    #[default]
    Events,
    Manifest,
    Commands,
    Config,
}

impl SchemaKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Events => "events",
            Self::Manifest => "manifest",
            Self::Commands => "commands",
            Self::Config => "config",
        }
    }

    #[must_use]
    pub fn parse(input_text: &str) -> Option<Self> {
        let normalized = input_text.trim().to_ascii_lowercase();
        parse_primary(&normalized).or_else(|| parse_secondary(&normalized))
    }
}

fn parse_primary(normalized: &str) -> Option<SchemaKind> {
    match normalized {
        "events" => Some(SchemaKind::Events),
        "manifest" => Some(SchemaKind::Manifest),
        _ => None,
    }
}

fn parse_secondary(normalized: &str) -> Option<SchemaKind> {
    match normalized {
        "commands" => Some(SchemaKind::Commands),
        "config" => Some(SchemaKind::Config),
        _ => None,
    }
}

fn build_events_schema() -> Result<serde_json::Value, Diagnostic> {
    let raw_schema = schemars::schema_for!(protocol::Envelope);
    serde_json::to_value(raw_schema).map_err(|_| {
        Diagnostic::new(
            ErrorCode::ExecutionFailed,
            "Could not serialize the events schema.",
        )
    })
}

fn build_manifest_editions_spec() -> serde_json::Value {
    serde_json::json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "lang": { "type": "string" },
                "aspect": { "type": "string" },
                "status": { "type": "string" }
            },
            "required": ["lang", "aspect", "status"]
        }
    })
}

fn build_manifest_deliveries_spec() -> serde_json::Value {
    serde_json::json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "edition": { "type": "string" },
                "format": { "type": "string" },
                "path": { "type": "string" }
            },
            "required": ["id", "edition", "format", "path"]
        }
    })
}

fn build_manifest_aspect_spec() -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "enum": ["9:16", "16:9", "1:1"],
        "description": "Aspect ratio for rendering"
    })
}

fn build_manifest_tags_spec() -> serde_json::Value {
    serde_json::json!({
        "type": "array",
        "items": { "type": "string" },
        "description": "Categorization tags"
    })
}

fn build_manifest_properties() -> serde_json::Value {
    serde_json::json!({
        "schema": {
            "type": "integer",
            "minimum": MANIFEST_SCHEMA_VERSION,
            "description": "Manifest schema version"
        },
        "id": {
            "type": "string",
            "description": "Unique immutable video identifier"
        },
        "title": {
            "type": "string",
            "description": "Human-readable title of the video"
        },
        "created": {
            "type": "string",
            "format": "date-time",
            "description": "Creation timestamp in ISO-8601 format"
        },
        "aspect": build_manifest_aspect_spec(),
        "tags": build_manifest_tags_spec(),
        "editions": build_manifest_editions_spec(),
        "deliveries": build_manifest_deliveries_spec()
    })
}

fn build_manifest_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": SCHEMA_DRAFT_07,
        "title": "Retold Video Manifest Schema",
        "description": "Schema definition for video manifest.json descriptors",
        "type": "object",
        "properties": build_manifest_properties(),
        "required": ["schema", "id", "title", "created"],
        "examples": [{
            "schema": 1,
            "id": "harbor-ep2",
            "title": "Harbor Patrol Episode 2",
            "created": "2026-10-20T10:00:00Z",
            "aspect": "9:16",
            "tags": ["harbor", "documentary"]
        }]
    })
}

fn build_core_commands() -> serde_json::Value {
    serde_json::json!({
        "workspace": {
            "description": "Workspace root registry, inspection, and switching",
            "subcommands": ["init", "show", "check", "list", "switch"],
            "examples": ["retold workspace show", "retold workspace list"]
        },
        "video": {
            "description": "Video production lifecycle, inspection, review, and packaging",
            "subcommands": ["list", "plan", "continue", "review", "export", "package", "verify", "paths", "clean"],
            "examples": ["retold video list", "retold video continue harbor/2 --until storyboard"]
        },
        "queue": {
            "description": "Studio execution queue management and job control",
            "subcommands": ["list", "show", "move", "pause", "unpause", "stop", "attach"],
            "examples": ["retold queue list", "retold queue pause job-123"]
        },
        "assets": {
            "description": "Workspace media asset enumeration, provenance, and verification",
            "subcommands": ["list", "show", "verify", "uses"],
            "examples": ["retold assets list", "retold assets verify asset-456"]
        }
    })
}

fn build_aux_commands() -> serde_json::Value {
    serde_json::json!({
        "storage": {
            "description": "Workspace storage analysis, portable backup, restore, and activation",
            "subcommands": ["usage", "backup", "verify", "restore", "activate"],
            "examples": ["retold storage usage --by video", "retold storage backup --output ./backup"]
        },
        "doctor": {
            "description": "Workspace diagnostic inspection, repairs, and support bundles",
            "subcommands": ["inspect", "repair", "bundle"],
            "examples": ["retold doctor", "retold doctor bundle --output ./diag-bundle"]
        },
        "config": {
            "description": "Configuration profile management, validation, and migration",
            "subcommands": ["profile", "validate", "migrate"],
            "examples": ["retold config profile list", "retold config validate --root ./config"]
        },
        "dev": {
            "description": "Developer tooling and versioned JSON schemas",
            "subcommands": ["schema"],
            "examples": ["retold dev schema --kind commands", "retold dev schema --kind config"]
        }
    })
}

fn build_command_groups() -> serde_json::Value {
    let mut core = build_core_commands();
    if let (Some(core_map), Some(aux_map)) =
        (core.as_object_mut(), build_aux_commands().as_object())
    {
        for (key, value) in aux_map {
            core_map.insert(key.clone(), value.clone());
        }
    }
    core
}

fn build_commands_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": SCHEMA_DRAFT_07,
        "title": "Retold Command Grammar Schema",
        "description": "Exposes command families, required arguments, options, and usage examples",
        "type": "object",
        "properties": {
            "commands": build_command_groups(),
            "global_options": {
                "json": { "type": "boolean", "description": "Emit machine-readable NDJSON" },
                "plain": { "type": "boolean", "description": "Emit accessible linear plain text" },
                "quiet": { "type": "boolean", "description": "Suppress informational progress" },
                "workspace": { "type": "string", "description": "Explicit target workspace path" }
            }
        },
        "required": ["commands", "global_options"]
    })
}

fn build_workspace_config_spec() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "schema_version": { "type": "integer", "minimum": WORKSPACE_SCHEMA_VERSION },
            "id": { "type": "string" },
            "name": { "type": "string" },
            "library_path": { "type": "string" }
        },
        "required": ["schema_version", "id", "name"]
    })
}

fn build_profile_config_spec() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "aspect": { "type": "string", "enum": ["9:16", "16:9", "1:1"] },
            "usage_policy": { "type": "string" },
            "export_preset": { "type": "string" }
        },
        "required": ["name"]
    })
}

fn build_channel_config_spec() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "description": { "type": "string" },
            "guides": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["name"]
    })
}

fn build_config_definitions() -> serde_json::Value {
    serde_json::json!({
        "WorkspaceConfig": build_workspace_config_spec(),
        "ProfileConfig": build_profile_config_spec(),
        "ChannelConfig": build_channel_config_spec()
    })
}

fn build_config_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": SCHEMA_DRAFT_07,
        "title": "Retold Configuration Schema",
        "description": "Exposes schemas for workspace.toml, settings profiles, and channel configurations",
        "type": "object",
        "definitions": build_config_definitions(),
        "properties": {
            "workspace": { "$ref": "#/definitions/WorkspaceConfig" },
            "profile": { "$ref": "#/definitions/ProfileConfig" },
            "channel": { "$ref": "#/definitions/ChannelConfig" }
        },
        "examples": [{
            "workspace": {
                "schema_version": 1,
                "id": "ws-main",
                "name": "Main Studio"
            }
        }]
    })
}

pub fn schema_for_kind(kind: SchemaKind) -> Result<serde_json::Value, Diagnostic> {
    match kind {
        SchemaKind::Events => build_events_schema(),
        SchemaKind::Manifest => Ok(build_manifest_schema()),
        SchemaKind::Commands => Ok(build_commands_schema()),
        SchemaKind::Config => Ok(build_config_schema()),
    }
}

fn payload_from_kind(kind: SchemaKind) -> Payload {
    match schema_for_kind(kind) {
        Ok(schema_value) => Payload::Result {
            outcome: Outcome::Completed,
            contents: schema_value,
        },
        Err(diagnostic) => diagnostic.into_payload(),
    }
}

#[must_use]
pub fn default_events_schema() -> Payload {
    payload_from_kind(SchemaKind::Events)
}

#[must_use]
pub fn dev_schema(kind_input: &str) -> Payload {
    match SchemaKind::parse(kind_input) {
        Some(kind) => payload_from_kind(kind),
        None => Diagnostic::new(
            ErrorCode::InputRequired,
            "Invalid schema kind. Supported kinds: events, manifest, commands, config.",
        )
        .into_payload(),
    }
}
