//! Redacted, offline support bundle generation for diagnostics and issue reporting.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::doctor;
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};

const BUNDLE_FORMAT: &str = "retold-support-bundle";
const RED_SECRET: &str = "[REDACTED]";
const RED_CANARY: &str = "[REDACTED_CANARY]";
const RED_API_KEY: &str = "[REDACTED_API_KEY]";
const RED_PROMPT: &str = "[REDACTED_PROMPT]";
const RED_AUTH: &str = "[REDACTED_AUTH_HEADER]";
const MAX_EVENTS: usize = 50;
const MIN_KEY_LEN: usize = 16;

#[derive(Debug, Clone)]
pub struct BundleOptions<'a> {
    pub output: &'a Path,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleFileEntry {
    pub path: String,
    pub class: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format: String,
    pub version: String,
    pub created_at: String,
    pub file_count: usize,
    pub content_classes: Vec<String>,
    pub files: Vec<BundleFileEntry>,
    pub redactions_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleFilePreview {
    pub path: String,
    pub class: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedText {
    pub text: String,
    pub redactions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedValue {
    pub value: serde_json::Value,
    pub redactions: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnsiFilterState {
    Normal,
    Escaping,
}

const fn next_ansi_pair(
    state: AnsiFilterState,
    current_char: char,
) -> (AnsiFilterState, Option<char>) {
    match state {
        AnsiFilterState::Normal => {
            if current_char == '\x1b' {
                (AnsiFilterState::Escaping, None)
            } else {
                (AnsiFilterState::Normal, Some(current_char))
            }
        }
        AnsiFilterState::Escaping => {
            if current_char.is_ascii_alphabetic() {
                (AnsiFilterState::Normal, None)
            } else {
                (AnsiFilterState::Escaping, None)
            }
        }
    }
}

fn strip_ansi_escapes(raw_text: &str) -> String {
    let (_, result_text) = raw_text.chars().fold(
        (
            AnsiFilterState::Normal,
            String::with_capacity(raw_text.len()),
        ),
        |(state, mut acc), current_char| {
            let (next_state, maybe_char) = next_ansi_pair(state, current_char);
            if let Some(clean_char) = maybe_char {
                acc.push(clean_char);
            }
            (next_state, acc)
        },
    );
    result_text
}

fn find_token_end(token_tail: &str) -> usize {
    token_tail
        .find(|token_char: char| {
            !token_char.is_alphanumeric() && token_char != '_' && token_char != '-'
        })
        .unwrap_or(token_tail.len())
}

fn find_canary_index(text: &str) -> Option<usize> {
    text.to_ascii_uppercase().find("CANARY")
}

fn redact_canaries_pass(source_text: &str) -> RedactedText {
    let mut output_text = String::new();
    let mut remaining = source_text;
    let mut redactions = 0usize;
    while let Some(index) = find_canary_index(remaining) {
        let (head, tail) = remaining.split_at(index);
        output_text.push_str(head);
        let token_len = find_token_end(tail);
        output_text.push_str(RED_CANARY);
        redactions = redactions.saturating_add(1);
        remaining = &tail[token_len..];
    }
    output_text.push_str(remaining);
    RedactedText {
        text: output_text,
        redactions,
    }
}

fn redact_api_tokens_pass(source_text: &str) -> RedactedText {
    let mut output_text = String::new();
    let mut remaining = source_text;
    let mut redactions = 0usize;
    while let Some(index) = remaining.find("sk-") {
        let (head, tail) = remaining.split_at(index);
        output_text.push_str(head);
        let token_len = find_token_end(tail);
        let is_real_token = token_len >= MIN_KEY_LEN;
        if is_real_token {
            output_text.push_str(RED_API_KEY);
            redactions = redactions.saturating_add(1);
        } else {
            output_text.push_str(&tail[..token_len]);
        }
        remaining = &tail[token_len..];
    }
    output_text.push_str(remaining);
    RedactedText {
        text: output_text,
        redactions,
    }
}

fn is_auth_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("authorization:") || lower.contains("bearer ")
}

fn transform_auth_line(line: &str) -> (String, usize) {
    if is_auth_line(line) {
        (format!("{RED_AUTH}\n"), 1)
    } else {
        (format!("{line}\n"), 0)
    }
}

fn normalize_trailing_newline(
    source_text: &str,
    mut output_text: String,
    redactions: usize,
) -> RedactedText {
    let should_pop = !source_text.ends_with('\n') && output_text.ends_with('\n');
    if should_pop {
        output_text.pop();
    }
    RedactedText {
        text: output_text,
        redactions,
    }
}

fn redact_auth_headers_pass(source_text: &str) -> RedactedText {
    let mut output_text = String::new();
    let mut redactions = 0usize;
    for line in source_text.lines() {
        let (line_text, line_redactions) = transform_auth_line(line);
        output_text.push_str(&line_text);
        redactions = redactions.saturating_add(line_redactions);
    }
    normalize_trailing_newline(source_text, output_text, redactions)
}

#[must_use]
pub fn redact_string(input_text: &str) -> RedactedText {
    let stripped = strip_ansi_escapes(input_text);
    let canary_pass = redact_canaries_pass(&stripped);
    let token_pass = redact_api_tokens_pass(&canary_pass.text);
    let auth_pass = redact_auth_headers_pass(&token_pass.text);
    let total_redactions = canary_pass
        .redactions
        .saturating_add(token_pass.redactions)
        .saturating_add(auth_pass.redactions);
    RedactedText {
        text: auth_pass.text,
        redactions: total_redactions,
    }
}

fn is_sensitive_key(key_name: &str) -> bool {
    key_name.contains("secret")
        || key_name.contains("token")
        || key_name.contains("password")
        || key_name.contains("key")
        || key_name.contains("credential")
        || key_name.contains("auth")
}

fn is_prompt_key(key_name: &str) -> bool {
    key_name.contains("prompt")
        || key_name.contains("source_text")
        || key_name.contains("script_text")
}

fn redact_map_entry(key: &str, inner_value: &serde_json::Value) -> (serde_json::Value, usize) {
    let key_lower = key.to_ascii_lowercase();
    let is_sensitive = is_sensitive_key(&key_lower);
    let is_prompt = is_prompt_key(&key_lower);
    if is_sensitive {
        (serde_json::json!(RED_SECRET), 1)
    } else if is_prompt {
        (serde_json::json!(RED_PROMPT), 1)
    } else {
        let inner_res = redact_json_value(inner_value);
        (inner_res.value, inner_res.redactions)
    }
}

fn redact_object_map(source_map: &serde_json::Map<String, serde_json::Value>) -> RedactedValue {
    let mut sanitized_map = serde_json::Map::new();
    let mut total_redactions = 0usize;
    for (key, inner_value) in source_map {
        let (entry_value, entry_redactions) = redact_map_entry(key, inner_value);
        total_redactions = total_redactions.saturating_add(entry_redactions);
        sanitized_map.insert(key.clone(), entry_value);
    }
    RedactedValue {
        value: serde_json::Value::Object(sanitized_map),
        redactions: total_redactions,
    }
}

fn redact_array_items(item_list: &[serde_json::Value]) -> RedactedValue {
    let mut sanitized_items = Vec::with_capacity(item_list.len());
    let mut total_redactions = 0usize;
    for item in item_list {
        let item_res = redact_json_value(item);
        total_redactions = total_redactions.saturating_add(item_res.redactions);
        sanitized_items.push(item_res.value);
    }
    RedactedValue {
        value: serde_json::Value::Array(sanitized_items),
        redactions: total_redactions,
    }
}

#[must_use]
pub fn redact_json_value(input_value: &serde_json::Value) -> RedactedValue {
    match input_value {
        serde_json::Value::Object(source_map) => redact_object_map(source_map),
        serde_json::Value::Array(item_list) => redact_array_items(item_list),
        serde_json::Value::String(text_content) => {
            let res = redact_string(text_content);
            RedactedValue {
                value: serde_json::Value::String(res.text),
                redactions: res.redactions,
            }
        }
        other_value => RedactedValue {
            value: other_value.clone(),
            redactions: 0,
        },
    }
}

struct BundleComponent {
    filename: &'static str,
    class: &'static str,
    content: String,
    redactions: usize,
}

fn collect_health_component(library_root: &Path) -> Result<BundleComponent, Diagnostic> {
    let report_val = health_value(library_root)?;
    let sanitized = redact_json_value(&report_val);
    let content = serde_json::to_string_pretty(&sanitized.value)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, "Failed to encode health report."))?;
    Ok(BundleComponent {
        filename: "health.json",
        class: "health",
        content,
        redactions: sanitized.redactions,
    })
}

fn health_value(root: &Path) -> Result<serde_json::Value, Diagnostic> {
    match doctor::generate_doctor_report(root) {
        Ok(report) => serde_json::to_value(report).map_err(|_| {
            Diagnostic::new(ErrorCode::DataInvalid, "Failed to serialize doctor report.")
        }),
        Err(_) => Ok(serde_json::json!({
            "healthy": false,
            "configuration_invalid": true,
            "message": "Workspace configuration could not be inspected. Repair workspace.toml before production; no workspace paths were trusted."
        })),
    }
}

fn collect_channel_names(channels_dir: &Path) -> Vec<String> {
    let mut channel_names = Vec::new();
    if let Ok(entries) = fs::read_dir(channels_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let is_directory = entry_path.is_dir();
            if is_directory {
                channel_names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    channel_names
}

fn collect_config_component(library_root: &Path) -> Result<BundleComponent, Diagnostic> {
    let ws_file = library_root.join("workspace.toml");
    let ws_exists = ws_file.is_file();
    let ws_content = if ws_exists {
        fs::read_to_string(&ws_file).unwrap_or_default()
    } else {
        String::new()
    };
    let channels_dir = library_root.join("channels");
    let channel_names = collect_channel_names(&channels_dir);
    let config_val = serde_json::json!({
        "workspace_root": library_root.display().to_string(),
        "workspace_toml_present": ws_exists,
        "workspace_toml_sanitized": ws_content,
        "installed_channels": channel_names,
    });
    let sanitized = redact_json_value(&config_val);
    let content = serde_json::to_string_pretty(&sanitized.value).map_err(|_| {
        Diagnostic::new(ErrorCode::DataInvalid, "Failed to encode config component.")
    })?;
    Ok(BundleComponent {
        filename: "config_sanitized.json",
        class: "config_sanitized",
        content,
        redactions: sanitized.redactions,
    })
}

fn collect_environment_component() -> Result<BundleComponent, Diagnostic> {
    let os_family = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let safe_keys = ["PATH", "SHELL", "TERM", "LANG", "LC_ALL", "USER"];
    let mut safe_vars = serde_json::Map::new();
    for key in safe_keys {
        if let Ok(env_val_str) = std::env::var(key) {
            safe_vars.insert(key.to_string(), serde_json::json!(env_val_str));
        }
    }
    let env_val = serde_json::json!({
        "os": os_family,
        "arch": arch,
        "retold_version": env!("CARGO_PKG_VERSION"),
        "safe_env": safe_vars,
    });
    let sanitized = redact_json_value(&env_val);
    let content = serde_json::to_string_pretty(&sanitized.value).map_err(|_| {
        Diagnostic::new(
            ErrorCode::DataInvalid,
            "Failed to encode environment component.",
        )
    })?;
    Ok(BundleComponent {
        filename: "environment.json",
        class: "environment",
        content,
        redactions: sanitized.redactions,
    })
}

fn parse_single_run_log(path: &Path) -> Option<RedactedValue> {
    let text = fs::read_to_string(path).ok()?;
    let parsed = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    Some(redact_json_value(&parsed))
}

fn scan_single_log_path(path: &Path) -> Option<(serde_json::Value, usize)> {
    let is_json = path.extension().is_some_and(|ext| ext == "json");
    if !is_json {
        return None;
    }
    let event_res = parse_single_run_log(path)?;
    Some((event_res.value, event_res.redactions))
}

fn populate_run_events(run_logs_dir: &Path) -> (Vec<serde_json::Value>, usize) {
    let mut recorded_events = Vec::new();
    let mut total_redactions = 0usize;
    if let Ok(entries) = fs::read_dir(run_logs_dir) {
        for entry in entries.flatten() {
            if recorded_events.len() >= MAX_EVENTS {
                break;
            }
            if let Some((event_val, redactions)) = scan_single_log_path(&entry.path()) {
                total_redactions = total_redactions.saturating_add(redactions);
                recorded_events.push(event_val);
            }
        }
    }
    (recorded_events, total_redactions)
}

fn collect_events_component(library_root: &Path) -> Result<BundleComponent, Diagnostic> {
    let run_logs_dir = library_root.join(".retold/runs");
    let (recorded_events, total_redactions) = populate_run_events(&run_logs_dir);
    let events_val = serde_json::json!({
        "events_count": recorded_events.len(),
        "recent_events": recorded_events,
    });
    let content = serde_json::to_string_pretty(&events_val).map_err(|_| {
        Diagnostic::new(ErrorCode::DataInvalid, "Failed to encode events component.")
    })?;
    Ok(BundleComponent {
        filename: "events_summary.json",
        class: "events_summary",
        content,
        redactions: total_redactions,
    })
}

fn collect_all_components(library_root: &Path) -> Result<Vec<BundleComponent>, Diagnostic> {
    let health = collect_health_component(library_root)?;
    let config = collect_config_component(library_root)?;
    let environment = collect_environment_component()?;
    let events = collect_events_component(library_root)?;
    Ok(vec![health, config, environment, events])
}

fn compute_sha256_digest(input_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input_bytes);
    format!("{:x}", hasher.finalize())
}

fn assemble_preview_entries(
    components: &[BundleComponent],
) -> (Vec<BundleFilePreview>, Vec<String>) {
    let mut files = vec![BundleFilePreview {
        path: "manifest.json".to_string(),
        class: "manifest".to_string(),
    }];
    let mut classes = vec!["manifest".to_string()];
    for item in components {
        files.push(BundleFilePreview {
            path: item.filename.to_string(),
            class: item.class.to_string(),
        });
        let has_class = classes.contains(&item.class.to_string());
        if !has_class {
            classes.push(item.class.to_string());
        }
    }
    (files, classes)
}

fn build_preview_payload(
    output: &Path,
    components: &[BundleComponent],
    redactions_count: usize,
) -> Payload {
    let (files, classes) = assemble_preview_entries(components);
    let preview = serde_json::json!({
        "output_path": output.display().to_string(),
        "dry_run": true,
        "file_count": files.len(),
        "content_classes": classes,
        "files": files,
        "redactions_count": redactions_count,
    });
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": "Support bundle preview (dry-run). No files written.",
            "bundle_preview": preview,
        }),
    }
}

fn write_file_and_hash(target: &Path, content: &str) -> Result<(u64, String), Diagnostic> {
    let bytes = content.as_bytes();
    fs::write(target, bytes).map_err(|_| {
        Diagnostic::new(ErrorCode::DataInvalid, "Failed to write bundle component.")
    })?;
    let sha256 = compute_sha256_digest(bytes);
    Ok((bytes.len() as u64, sha256))
}

fn target_component_path(staging_dir: &Path, filename: &str) -> PathBuf {
    staging_dir.join(filename)
}

fn write_component_file(
    staging_dir: &Path,
    component: &BundleComponent,
) -> Result<BundleFileEntry, Diagnostic> {
    let target = target_component_path(staging_dir, component.filename);
    let (bytes, sha256) = write_file_and_hash(&target, &component.content)?;
    Ok(BundleFileEntry {
        path: component.filename.to_string(),
        class: component.class.to_string(),
        bytes,
        sha256,
    })
}

fn write_all_components(
    staging_dir: &Path,
    components: &[BundleComponent],
) -> Result<Vec<BundleFileEntry>, Diagnostic> {
    let mut entries = Vec::new();
    for item in components {
        let entry = write_component_file(staging_dir, item)?;
        entries.push(entry);
    }
    Ok(entries)
}

fn publish_staging(staging_dir: &Path, output: &Path) -> Result<(), Diagnostic> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|_| {
            Diagnostic::new(ErrorCode::DataInvalid, "Failed to create parent directory.")
        })?;
    }
    fs::rename(staging_dir, output)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, "Failed to publish support bundle."))
}

fn assemble_content_classes(components: &[BundleComponent]) -> Vec<String> {
    let mut classes = vec!["manifest".to_string()];
    for item in components {
        let has_class = classes.contains(&item.class.to_string());
        if !has_class {
            classes.push(item.class.to_string());
        }
    }
    classes
}

struct StagingContext<'a> {
    staging_dir: &'a Path,
    output: &'a Path,
    redactions: usize,
}

const fn next_total_file_count(count: usize) -> usize {
    count.saturating_add(1)
}

fn current_timestamp_iso() -> String {
    jiff::Timestamp::now().to_string()
}

fn package_version_string() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn copy_manifest_file_entries(file_entries: &[BundleFileEntry]) -> Vec<BundleFileEntry> {
    file_entries.to_vec()
}

fn create_manifest_object(
    context: &StagingContext<'_>,
    components: &[BundleComponent],
    file_entries: &[BundleFileEntry],
) -> BundleManifest {
    let content_classes = assemble_content_classes(components);
    let created_at = current_timestamp_iso();
    let file_count = next_total_file_count(file_entries.len());
    let version = package_version_string();
    let files = copy_manifest_file_entries(file_entries);
    BundleManifest {
        format: BUNDLE_FORMAT.to_string(),
        version,
        created_at,
        file_count,
        content_classes,
        files,
        redactions_count: context.redactions,
    }
}

fn write_manifest_file(
    context: &StagingContext<'_>,
    components: &[BundleComponent],
    file_entries: &[BundleFileEntry],
) -> Result<(BundleManifest, u64), Diagnostic> {
    let manifest = create_manifest_object(context, components, file_entries);
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|_| {
        Diagnostic::new(
            ErrorCode::DataInvalid,
            "Failed to serialize bundle manifest.",
        )
    })?;
    let manifest_path = context.staging_dir.join("manifest.json");
    fs::write(&manifest_path, &manifest_bytes)
        .map_err(|_| Diagnostic::new(ErrorCode::DataInvalid, "Failed to write bundle manifest."))?;
    Ok((manifest, manifest_bytes.len() as u64))
}

fn write_bundle_payload(
    context: &StagingContext<'_>,
    components: &[BundleComponent],
) -> Result<Payload, Diagnostic> {
    let file_entries = write_all_components(context.staging_dir, components)?;
    let (manifest, manifest_len) = write_manifest_file(context, components, &file_entries)?;
    let component_bytes: u64 = file_entries.iter().map(|entry| entry.bytes).sum();
    let total_bytes = component_bytes.saturating_add(manifest_len);
    publish_staging(context.staging_dir, context.output)?;

    let text = format!("Support bundle created at {}", context.output.display());
    Ok(Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({
            "text": text,
            "bundle_created": {
                "output_path": context.output.display().to_string(),
                "file_count": manifest.file_count,
                "total_bytes": total_bytes,
                "content_classes": &manifest.content_classes,
                "manifest": manifest,
            }
        }),
    })
}

fn validate_destination_available(output: &Path) -> Result<(), Diagnostic> {
    let target_exists = output.exists();
    if target_exists {
        Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Support bundle destination already exists; bundles never overwrite.",
        ))
    } else {
        Ok(())
    }
}

fn create_bundle_staging_dir(output: &Path) -> Result<tempfile::TempDir, Diagnostic> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|_| {
        Diagnostic::new(
            ErrorCode::DataInvalid,
            "Failed to create bundle parent directory.",
        )
    })?;
    tempfile::Builder::new()
        .prefix(".retold-bundle-")
        .tempdir_in(parent)
        .map_err(|_| {
            Diagnostic::new(
                ErrorCode::DataInvalid,
                "Failed to create bundle staging directory.",
            )
        })
}

pub fn create_support_bundle(
    library_root: &Path,
    options: &BundleOptions<'_>,
) -> Result<Payload, Diagnostic> {
    validate_destination_available(options.output)?;
    let components = collect_all_components(library_root)?;
    let total_redactions: usize = components.iter().map(|item| item.redactions).sum();
    if options.dry_run {
        return Ok(build_preview_payload(
            options.output,
            &components,
            total_redactions,
        ));
    }
    let staging = create_bundle_staging_dir(options.output)?;
    let context = StagingContext {
        staging_dir: staging.path(),
        output: options.output,
        redactions: total_redactions,
    };
    write_bundle_payload(&context, &components)
}
