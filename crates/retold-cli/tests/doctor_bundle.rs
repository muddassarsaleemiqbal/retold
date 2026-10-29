//! Integration tests for offline diagnostic support bundle generation (F20).
#![expect(clippy::unwrap_used, reason = "test assertions and fixtures")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

const EXPECTED_BUNDLE_FILES: usize = 5;
const EXPECTED_MANIFEST_FILES: usize = 4;
const SHA256_HEX_LENGTH: usize = 64;
const TEST_CANARY_SECRET: &str = "CANARY_SECRET_X789_TOP_SECRET";
const TEST_API_KEY: &str = "sk-proj-1234567890abcdef12345678";
const REDACTED_CANARY_MARKER: &str = "[REDACTED_CANARY]";
const REDACTED_API_KEY_MARKER: &str = "[REDACTED_API_KEY]";

struct DoctorBundleFixture {
    _temp: TempDir,
    library: PathBuf,
}

fn create_library_dir(library: &Path, name: &str) {
    let dir_path = library.join(name);
    fs::create_dir_all(dir_path).unwrap();
}

fn seed_standard_subdirectories(library: &Path) {
    create_library_dir(library, "guides");
    create_library_dir(library, "channels");
    create_library_dir(library, "series");
    create_library_dir(library, ".retold/runs");
}

fn seed_workspace_manifest(library: &Path) {
    let ws_toml = library.join("workspace.toml");
    let ws_content = format!(
        "name = \"Diagnostic Workspace\"\nsecret_token = \"{TEST_CANARY_SECRET}\"\napi_key = \"{TEST_API_KEY}\"\n"
    );
    fs::write(ws_toml, ws_content).unwrap();
}

fn build_sample_run_payload() -> String {
    format!(
        "{{\"event\": \"step_complete\", \"canary\": \"{TEST_CANARY_SECRET}\", \"key\": \"{TEST_API_KEY}\"}}"
    )
}

fn seed_sample_run_log(library: &Path) {
    let sample_run = library.join(".retold/runs/run-1.json");
    let run_payload = build_sample_run_payload();
    fs::write(sample_run, run_payload).unwrap();
}

fn seed_workspace_structure(library: &Path) {
    seed_standard_subdirectories(library);
    seed_workspace_manifest(library);
    seed_sample_run_log(library);
}

fn run_bundle_cli(library: &Path, args: &[&str]) -> Output {
    let mut cli = Command::cargo_bin("retold").unwrap();
    cli.args(["--library"]).arg(library);
    cli.args(args);
    cli.output().unwrap()
}

fn run_bundle_json(library: &Path, args: &[&str]) -> Value {
    let mut cli = Command::cargo_bin("retold").unwrap();
    cli.args(["--json", "--library"]).arg(library);
    cli.args(args);
    let output = cli.output().unwrap();
    serde_json::from_slice(&output.stdout).unwrap()
}

impl DoctorBundleFixture {
    fn setup() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let library = temp_dir.path().join("library");
        fs::create_dir_all(&library).unwrap();
        seed_workspace_structure(&library);
        Self {
            _temp: temp_dir,
            library,
        }
    }

    fn output_bundle_path(&self, name: &str) -> PathBuf {
        self.library.parent().unwrap().join(name)
    }
}

#[test]
fn bundle_dry_run_previews_classes_and_leaves_filesystem_untouched() {
    let fixture = DoctorBundleFixture::setup();
    let bundle_path = fixture.output_bundle_path("test-dry-run-bundle");
    let path_str = bundle_path.display().to_string();

    let result = run_bundle_json(
        &fixture.library,
        &["doctor", "bundle", "--output", &path_str, "--dry-run"],
    );

    assert_eq!(result["type"], "result");
    let preview = &result["payload"]["data"]["bundle_preview"];
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["output_path"], path_str);
    assert_eq!(preview["file_count"], EXPECTED_BUNDLE_FILES);

    let classes = preview["content_classes"].as_array().unwrap();
    assert!(classes.iter().any(|c| c == "manifest"));
    assert!(classes.iter().any(|c| c == "health"));
    assert!(classes.iter().any(|c| c == "config_sanitized"));
    assert!(classes.iter().any(|c| c == "environment"));
    assert!(classes.iter().any(|c| c == "events_summary"));

    assert!(!bundle_path.exists());
}

fn verify_single_bundle_file(bundle_path: &Path, file_entry: &Value) {
    let rel_path = file_entry["path"].as_str().unwrap();
    let expected_sha256 = file_entry["sha256"].as_str().unwrap();
    let expected_bytes = file_entry["bytes"].as_u64().unwrap();

    let disk_path = bundle_path.join(rel_path);
    assert!(disk_path.is_file(), "File must exist: {rel_path}");
    let actual_bytes = fs::metadata(&disk_path).unwrap().len();
    assert_eq!(expected_bytes, actual_bytes);

    assert_eq!(
        expected_sha256.len(),
        SHA256_HEX_LENGTH,
        "Checksum must be 64-char hex string"
    );
    assert!(
        expected_sha256.chars().all(|c| c.is_ascii_hexdigit()),
        "Checksum must contain only hex digits"
    );
}

fn verify_all_bundle_files(bundle_path: &Path, files: &[Value]) {
    for file_entry in files {
        verify_single_bundle_file(bundle_path, file_entry);
    }
}

#[test]
fn bundle_generates_manifest_and_verifies_sha256_checksums() {
    let fixture = DoctorBundleFixture::setup();
    let bundle_path = fixture.output_bundle_path("test-real-bundle");
    let path_str = bundle_path.display().to_string();

    let result = run_bundle_json(
        &fixture.library,
        &["doctor", "bundle", "--output", &path_str],
    );

    assert_eq!(result["type"], "result");
    let created = &result["payload"]["data"]["bundle_created"];
    assert_eq!(created["file_count"], EXPECTED_BUNDLE_FILES);
    assert!(bundle_path.is_dir());

    let manifest_file = bundle_path.join("manifest.json");
    assert!(manifest_file.is_file());

    let manifest_val: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_file).unwrap()).unwrap();
    assert_eq!(manifest_val["format"], "retold-support-bundle");
    assert_eq!(manifest_val["file_count"], EXPECTED_BUNDLE_FILES);

    let files = manifest_val["files"].as_array().unwrap();
    assert_eq!(files.len(), EXPECTED_MANIFEST_FILES);
    verify_all_bundle_files(&bundle_path, files);
}

#[test]
fn bundle_fails_on_destination_collision() {
    let fixture = DoctorBundleFixture::setup();
    let bundle_path = fixture.output_bundle_path("collision-target");
    fs::create_dir_all(&bundle_path).unwrap();
    let path_str = bundle_path.display().to_string();

    let result = run_bundle_json(
        &fixture.library,
        &["doctor", "bundle", "--output", &path_str],
    );

    assert_eq!(result["type"], "error");
    assert_eq!(result["payload"]["code"], "RG-INPUT-REQUIRED");
}

fn inspect_file_for_leaks(entry_path: &Path) {
    let content = fs::read_to_string(entry_path).unwrap();
    assert!(
        !content.contains(TEST_CANARY_SECRET),
        "Bundle leaked canary secret in file: {}",
        entry_path.display()
    );
    assert!(
        !content.contains(TEST_API_KEY),
        "Bundle leaked API key in file: {}",
        entry_path.display()
    );
}

fn verify_directory_secrets_absent(bundle_path: &Path) {
    let entries = fs::read_dir(bundle_path).unwrap();
    let mut inspected_file_count = 0;
    for entry in entries {
        let entry_path = entry.unwrap().path();
        if entry_path.is_file() {
            inspect_file_for_leaks(&entry_path);
            inspected_file_count += 1;
        }
    }
    assert_eq!(inspected_file_count, EXPECTED_BUNDLE_FILES);
}

#[test]
fn bundle_redacts_secret_canaries_and_api_tokens() {
    let fixture = DoctorBundleFixture::setup();
    let bundle_path = fixture.output_bundle_path("canary-check-bundle");
    let path_str = bundle_path.display().to_string();

    let result = run_bundle_json(
        &fixture.library,
        &["doctor", "bundle", "--output", &path_str],
    );
    assert_eq!(result["type"], "result");
    verify_directory_secrets_absent(&bundle_path);

    let config_file = bundle_path.join("config_sanitized.json");
    let config_content = fs::read_to_string(config_file).unwrap();
    assert!(config_content.contains(REDACTED_CANARY_MARKER));
    assert!(config_content.contains(REDACTED_API_KEY_MARKER));
}

#[test]
fn bundle_human_projection_renders_dry_run_and_creation() {
    let fixture = DoctorBundleFixture::setup();
    let bundle_path = fixture.output_bundle_path("human-bundle");
    let path_str = bundle_path.display().to_string();

    let dry_output = run_bundle_cli(
        &fixture.library,
        &["doctor", "bundle", "--output", &path_str, "--dry-run"],
    );
    let dry_text = String::from_utf8_lossy(&dry_output.stdout);
    assert!(dry_text.contains("Support bundle preview for"));
    assert!(dry_text.contains("Dry run; no files were written to disk."));

    let create_output = run_bundle_cli(
        &fixture.library,
        &["doctor", "bundle", "--output", &path_str],
    );
    let create_text = String::from_utf8_lossy(&create_output.stdout);
    assert!(create_text.contains("Support bundle created at"));
    assert!(create_text.contains("Files: 5"));
}
