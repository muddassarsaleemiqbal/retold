//! Integration tests for `retold dev schema [--kind <kind>]`
#![expect(clippy::unwrap_used, reason = "test assertions and fixtures")]
#![expect(clippy::expect_used, reason = "test assertions and fixtures")]

use assert_cmd::Command;

const DRAFT_07_URL: &str = "http://json-schema.org/draft-07/schema#";

fn run_retold(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::cargo_bin("retold").unwrap();
    cmd.args(args);
    let output = cmd.output();
    output.expect("failed to execute retold binary")
}

fn query_dev_schema_json(extra_args: &[&str]) -> serde_json::Value {
    let mut args = vec!["--json", "dev", "schema"];
    args.extend_from_slice(extra_args);

    let output = run_retold(&args);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("valid utf-8 output");
    let line = stdout.lines().next().expect("at least one line of output");
    serde_json::from_str(line).expect("valid json envelope")
}

#[test]
fn dev_schema_default_returns_events_envelope_schema() {
    let result = query_dev_schema_json(&[]);
    assert_eq!(result["type"], "result");
    let schema_data = &result["payload"]["data"];
    assert_eq!(schema_data["title"], "Envelope");
    assert!(schema_data["properties"]["request_id"].is_object());
    assert!(schema_data["properties"]["schema_version"].is_object());
    assert!(schema_data["oneOf"].is_array());
}

#[test]
fn dev_schema_kind_events_matches_default() {
    let default_res = query_dev_schema_json(&[]);
    let events_res = query_dev_schema_json(&["--kind", "events"]);

    assert_eq!(
        default_res["payload"]["data"],
        events_res["payload"]["data"]
    );
}

#[test]
fn dev_schema_plain_output_renders_formatted_json_schema() {
    let output = run_retold(&["--plain", "dev", "schema", "--kind", "events"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("valid utf-8 output");
    assert!(stdout.contains("\"title\": \"Envelope\""));
}

#[test]
fn dev_schema_kind_manifest_exposes_required_fields_and_types() {
    let result = query_dev_schema_json(&["--kind", "manifest"]);
    assert_eq!(result["type"], "result");
    let schema_data = &result["payload"]["data"];
    assert_eq!(schema_data["$schema"], DRAFT_07_URL);
    assert_eq!(schema_data["title"], "Retold Video Manifest Schema");

    let required = schema_data["required"].as_array().unwrap();
    assert!(required.iter().any(|field| field == "schema"));
    assert!(required.iter().any(|field| field == "id"));
    assert!(required.iter().any(|field| field == "title"));
    assert!(required.iter().any(|field| field == "created"));

    let properties = &schema_data["properties"];
    assert!(properties["aspect"].is_object());
    assert!(properties["tags"].is_object());
    assert!(properties["editions"].is_object());
    assert!(properties["deliveries"].is_object());
}

#[test]
fn dev_schema_kind_commands_exposes_families_and_examples() {
    let result = query_dev_schema_json(&["--kind", "commands"]);
    assert_eq!(result["type"], "result");
    let schema_data = &result["payload"]["data"];
    assert_eq!(schema_data["$schema"], DRAFT_07_URL);
    assert_eq!(schema_data["title"], "Retold Command Grammar Schema");

    let commands = &schema_data["properties"]["commands"];
    assert!(commands["workspace"].is_object());
    assert!(commands["video"].is_object());
    assert!(commands["queue"].is_object());
    assert!(commands["assets"].is_object());
    assert!(commands["storage"].is_object());
    assert!(commands["doctor"].is_object());
    assert!(commands["config"].is_object());
    assert!(commands["dev"].is_object());

    let video_examples = commands["video"]["examples"].as_array().unwrap();
    assert_ne!(video_examples.len(), 0);
}

#[test]
fn dev_schema_kind_config_exposes_workspace_profile_and_channel_definitions() {
    let result = query_dev_schema_json(&["--kind", "config"]);
    assert_eq!(result["type"], "result");
    let schema_data = &result["payload"]["data"];
    assert_eq!(schema_data["$schema"], DRAFT_07_URL);
    assert_eq!(schema_data["title"], "Retold Configuration Schema");

    let definitions = &schema_data["definitions"];
    assert!(definitions["WorkspaceConfig"].is_object());
    assert!(definitions["ProfileConfig"].is_object());
    assert!(definitions["ChannelConfig"].is_object());

    let ws_required = definitions["WorkspaceConfig"]["required"]
        .as_array()
        .unwrap();
    assert!(ws_required.iter().any(|field| field == "schema_version"));
    assert!(ws_required.iter().any(|field| field == "id"));
}
