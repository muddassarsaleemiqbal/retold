//! Integration tests for workspace root initialization, inspection, validation, and switching.
#![expect(clippy::unwrap_used, reason = "test assertions and fixtures")]
#![expect(clippy::expect_used, reason = "test assertions and fixtures")]

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

struct WorkspaceFixture {
    sandbox: TempDir,
    root: PathBuf,
}

impl WorkspaceFixture {
    fn new(subdir_name: &str) -> Self {
        let sandbox = TempDir::new().unwrap();
        let root = sandbox.path().join(subdir_name);
        Self { sandbox, root }
    }

    fn invoke(&self, operands: &[&str]) -> Value {
        invoke_isolated(operands, self.sandbox.path())
    }
}

fn invoke_isolated(operands: &[&str], installation: &std::path::Path) -> Value {
    let mut invocation = Command::cargo_bin("retold").unwrap();
    invocation.env("RETOLD_CONFIG_DIR", installation.join("config"));
    invocation.env("HOME", installation);
    invocation.env_remove("RETOLD_WORKSPACE");
    invocation.env_remove("RETOLD_LIBRARY");
    invocation.env_remove("XDG_DATA_HOME");
    invocation.arg("--json").args(operands);
    let execution = invocation.output().expect("cli output");
    let text = String::from_utf8(execution.stdout).expect("utf-8");
    let line = text.lines().next().expect("envelope line");
    serde_json::from_str(line).expect("json parse")
}

#[test]
fn workspace_init_creates_structure_manifest_and_settings() {
    let fixture = WorkspaceFixture::new("my-studio");
    let root_str = fixture.root.to_string_lossy().to_string();

    let result = fixture.invoke(&["workspace", "init", &root_str, "--name", "Demo Studio"]);
    assert_eq!(result["type"], "result");
    assert_eq!(result["payload"]["outcome"], "completed");

    let ws_info = &result["payload"]["data"]["workspace"];
    assert_eq!(ws_info["name"], "Demo Studio");
    assert_eq!(ws_info["schema_version"], 1);
    let ws_id = ws_info["id"].as_str().unwrap();
    assert!(ws_id.starts_with("ws-"));

    let manifest_path = fixture.root.join("workspace.toml");
    assert!(manifest_path.is_file());

    let manifest_text = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_text.contains("schema_version = 1"));
    assert!(manifest_text.contains("Demo Studio"));

    assert!(fixture.root.join("config").is_dir());
    assert!(fixture.root.join("config/guides").is_dir());
    assert!(fixture.root.join("config/channels").is_dir());
    assert!(fixture.root.join("config/settings.toml").is_file());
    assert!(fixture.root.join("library").is_dir());
}

#[test]
fn workspace_init_dry_run_writes_no_files() {
    let fixture = WorkspaceFixture::new("dry-run-studio");
    let root_str = fixture.root.to_string_lossy().to_string();

    let result = fixture.invoke(&["workspace", "init", &root_str, "--dry-run"]);
    assert_eq!(result["type"], "result");
    assert_eq!(result["payload"]["outcome"], "completed");

    let ws_info = &result["payload"]["data"]["workspace"];
    assert_eq!(ws_info["dry_run"], true);
    assert!(!fixture.root.exists());
}

#[test]
fn workspace_init_existing_manifest_rejects_with_operation_exists() {
    let fixture = WorkspaceFixture::new("dup-studio");
    let root_str = fixture.root.to_string_lossy().to_string();

    let init1 = fixture.invoke(&["workspace", "init", &root_str]);
    assert_eq!(init1["payload"]["outcome"], "completed");

    let init2 = fixture.invoke(&["workspace", "init", &root_str]);
    assert_eq!(init2["type"], "error");
    assert_eq!(init2["payload"]["code"], "RG-DATA-INVALID");
}

#[test]
fn workspace_show_displays_paths_id_and_registration() {
    let fixture = WorkspaceFixture::new("show-studio");
    let root_str = fixture.root.to_string_lossy().to_string();

    fixture.invoke(&["workspace", "init", &root_str, "--name", "Showcase"]);

    let show_envelope = fixture.invoke(&["workspace", "show", "--workspace", &root_str]);
    assert_eq!(show_envelope["type"], "result");
    assert_eq!(show_envelope["payload"]["outcome"], "completed");

    let ws = &show_envelope["payload"]["data"]["workspace"];
    assert_eq!(ws["name"], "Showcase");
    assert_eq!(ws["manifest_present"], true);

    let paths = &ws["paths"];
    assert!(paths["root"].is_string());
    assert!(paths["config"].is_string());
    assert!(paths["library"].is_string());
    assert!(paths["state"].is_string());
    assert!(paths["cache"].is_string());
}

#[test]
fn workspace_check_validates_healthy_workspace() {
    let fixture = WorkspaceFixture::new("check-studio");
    let root_str = fixture.root.to_string_lossy().to_string();

    fixture.invoke(&["workspace", "init", &root_str]);

    let check_envelope = fixture.invoke(&["workspace", "check", "--workspace", &root_str]);
    assert_eq!(check_envelope["type"], "result");
    assert_eq!(check_envelope["payload"]["outcome"], "completed");

    let payload_data = &check_envelope["payload"]["data"];
    assert_eq!(payload_data["all_passed"], true);

    let checks = payload_data["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 5);
    for item in checks {
        assert_eq!(item["passed"], true);
    }
}

#[test]
fn workspace_switch_and_list() {
    let fixture = WorkspaceFixture::new("switch-studio");
    let root_str = fixture.root.to_string_lossy().to_string();

    fixture.invoke(&["workspace", "init", &root_str, "--name", "SwitchTarget"]);

    let switch_envelope = fixture.invoke(&["workspace", "switch", &root_str]);
    assert_eq!(switch_envelope["type"], "result");
    assert_eq!(switch_envelope["payload"]["outcome"], "completed");

    let workspaces = fixture.invoke(&["workspace", "list"]);
    assert_eq!(workspaces["type"], "result");
    assert_eq!(workspaces["payload"]["outcome"], "completed");

    let items = workspaces["payload"]["data"]["items"].as_array().unwrap();
    assert_ne!(items.len(), 0);
    let active_item = items.iter().find(|it| it["active"] == true);
    assert!(active_item.is_some());
}

#[test]
fn default_workspace_inspection_uses_the_resolved_config_and_media_paths() {
    let fixture = WorkspaceFixture::new("legacy-media");
    let config = legacy_inspection_paths(&fixture);
    let result = fixture.invoke(&["workspace", "show"]);
    let paths = &result["payload"]["data"]["workspace"]["paths"];
    assert_eq!(
        paths["root"],
        fixture.root.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(paths["library"], fixture.root.to_str().unwrap());
    assert_eq!(paths["config"], config.to_str().unwrap());
    let checked = fixture.invoke(&["workspace", "check"]);
    let checks = checked["payload"]["data"]["checks"].as_array().unwrap();
    for name in ["config_dir", "library_dir"] {
        let check = checks.iter().find(|check| check["name"] == name).unwrap();
        assert_eq!(check["passed"], true);
    }
}

fn install_guide(root: &std::path::Path) -> retold_guide::Guide {
    let guide = fixture_guide();
    let destination = root.join("config/guides").join(guide.id());
    guide.write_to(&destination).unwrap();
    guide
}

fn fixture_guide() -> retold_guide::Guide {
    retold_guide::catalog::bundled().unwrap().remove(0)
}

fn save_video(root: &std::path::Path) -> String {
    let guide = fixture_guide();
    let channel = fixture_channel(root);
    create_fixture_run(root, &guide, &channel)
}

fn fixture_channel(root: &std::path::Path) -> retold_guide::Channel {
    let config = root.join("config");
    let channel = retold_guide::channel::init_from_template(
        "test-studio",
        &config.join("channels/test-studio"),
    )
    .unwrap();
    empty_providers(&config);
    channel
}

fn empty_providers(config: &std::path::Path) {
    fs::write(
        config.join("providers.toml"),
        "schema_version = 1\n[providers]\n[routing]\n",
    )
    .unwrap();
}

fn create_fixture_run(
    root: &std::path::Path,
    guide: &retold_guide::Guide,
    channel: &retold_guide::Channel,
) -> String {
    let inputs = [
        ("title".into(), "Nosferatu".into()),
        ("kind".into(), "film".into()),
    ]
    .into();
    let new = retold_engine::NewRun {
        guide,
        channel,
        inputs: retold_guide::inputs::resolve(guide.manifest(), &inputs).unwrap(),
        variants: vec!["en".parse().unwrap()],
        quality: retold_engine::Quality::Standard,
        budget: retold_engine::gate::Usd::ZERO,
        skip_reviews: false,
        source_file: None,
    };
    let (_, meta) = retold_engine::run::create(&root.join("library"), new).unwrap();
    meta.id
}

#[test]
fn managed_video_list_plan_and_account_share_the_workspace_identity() {
    let fixture = WorkspaceFixture::new("production-root");
    let root = fixture.root.to_str().unwrap();
    let initialized = fixture.invoke(&["workspace", "init", root]);
    let identity = &initialized["payload"]["data"]["workspace"]["id"];
    let video = save_video(&fixture.root);
    fixture.invoke(&["workspace", "switch", root]);
    fixture.invoke(&["account", "init", "--timezone", "UTC"]);
    let listed = fixture.invoke(&["video", "list"]);
    assert_eq!(
        listed["payload"]["data"]["items"][0]["id"], video,
        "{listed}"
    );
    let planned = fixture.invoke(&["video", "plan", &video, "--until", "created"]);
    assert_eq!(planned["type"], "result", "{planned}");
    assert_eq!(
        &planned["payload"]["data"]["authority"]["workspace_id"], identity,
        "{planned}"
    );
    let continued = fixture.invoke(&["video", "continue", &video, "--until", "created", "--yes"]);
    assert_eq!(continued["type"], "result", "{continued}");
    let checked = fixture.invoke(&["workspace", "check", "--workspace", root]);
    assert_eq!(checked["payload"]["data"]["all_passed"], true, "{checked}");
}

#[test]
fn provider_and_configuration_commands_default_to_the_selected_workspace() {
    let fixture = WorkspaceFixture::new("configuration-context");
    let root = fixture.root.to_str().unwrap();
    fixture.invoke(&["workspace", "init", root]);
    empty_providers(&fixture.root.join("config"));
    fixture.invoke(&["workspace", "switch", root]);
    let providers = fixture.invoke(&["providers", "list"]);
    assert_eq!(providers["type"], "result", "{providers}");
    let configuration = fixture.invoke(&["config", "validate"]);
    assert_eq!(
        configuration["payload"]["data"]["valid"], true,
        "{configuration}"
    );
    let profiles = fixture.invoke(&["config", "profile", "list"]);
    assert_eq!(profiles["type"], "result", "{profiles}");
}

#[test]
fn existing_configuration_is_never_overwritten_by_initialization() {
    let fixture = WorkspaceFixture::new("existing-config");
    let config = fixture.root.join("config");
    fs::create_dir_all(&config).unwrap();
    let settings = config.join("settings.toml");
    let saved = b"default_channel = 'my-channel'\n";
    fs::write(&settings, saved).unwrap();
    let result = fixture.invoke(&["workspace", "init", fixture.root.to_str().unwrap()]);
    assert_eq!(result["type"], "error", "{result}");
    assert_eq!(fs::read(settings).unwrap(), saved);
    assert!(!fixture.root.join("workspace.toml").exists());
}

#[test]
fn unselected_setup_uses_installation_config_and_default_library() {
    let fixture = WorkspaceFixture::new("unused");
    let setup = fixture.invoke(&["setup"]);
    assert_eq!(setup["type"], "result", "{setup}");
    assert!(fixture.sandbox.path().join("config/guides").is_dir());
    assert!(fixture.sandbox.path().join("Retold/audio/music").is_dir());
    assert!(!fixture.sandbox.path().join("Retold/guides").exists());
}

#[test]
fn custom_workspace_content_paths_are_used_by_inspection_setup_and_resources() {
    let fixture = WorkspaceFixture::new("custom-paths");
    let root = fixture.root.to_str().unwrap();
    fixture.invoke(&["workspace", "init", root]);
    install_guide(&fixture.root);
    move_content_directories(&fixture.root);
    let show = fixture.invoke(&["workspace", "show", "--workspace", root]);
    let config = show["payload"]["data"]["workspace"]["paths"]["config"]
        .as_str()
        .unwrap();
    assert!(config.ends_with("creative"), "{show}");
    let guides = fixture.invoke(&["--workspace", root, "guides", "list"]);
    assert_eq!(guides["payload"]["data"]["total"], 1, "{guides}");
    let setup = fixture.invoke(&["--workspace", root, "setup"]);
    assert_eq!(setup["type"], "result", "{setup}");
    let check = fixture.invoke(&["workspace", "check", "--workspace", root]);
    assert_eq!(check["payload"]["data"]["all_passed"], true, "{check}");
    assert!(fixture.root.join("productions/audio/music").is_dir());
}

fn move_content_directories(root: &std::path::Path) {
    update_content_manifest(root);
    rename_directory(root, "config", "creative");
    rename_directory(root, "library", "productions");
}

fn rename_directory(root: &std::path::Path, from: &str, to: &str) {
    fs::rename(root.join(from), root.join(to)).unwrap();
}

fn update_content_manifest(root: &std::path::Path) {
    let manifest = root.join("workspace.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(manifest, custom_content_manifest(&original)).unwrap();
}

fn custom_content_manifest(original: &str) -> String {
    original
        .replace("config_path = \"config\"", "config_path = \"creative\"")
        .replace(
            "library_path = \"library\"",
            "library_path = \"productions\"",
        )
}

#[test]
fn managed_workspace_setup_switch_and_resource_queries_use_the_same_paths() {
    let fixture = WorkspaceFixture::new("managed");
    let root = fixture.root.to_str().unwrap();
    fixture.invoke(&["workspace", "init", root]);
    let guide_id = install_guide(&fixture.root).id().to_owned();
    let setup = fixture.invoke(&["--workspace", root, "setup"]);
    assert_eq!(setup["type"], "result", "{setup}");
    let explicit = fixture.invoke(&["--workspace", root, "guides", "list"]);
    assert_eq!(explicit["payload"]["data"]["items"][0]["id"], guide_id);
    fixture.invoke(&["workspace", "switch", root]);
    let selected = fixture.invoke(&["guides", "list"]);
    assert_eq!(
        selected["payload"]["data"]["items"],
        explicit["payload"]["data"]["items"]
    );
    let videos = fixture.invoke(&["video", "list"]);
    assert_eq!(videos["type"], "result", "{videos}");
    assert!(!fixture.root.join("guides").exists());
    assert!(fixture.root.join("library/audio/music").is_dir());
}

#[test]
fn explicit_workspace_does_not_borrow_the_remembered_workspaces_guides() {
    let fixture = WorkspaceFixture::new("selected");
    let selected = fixture.root.to_str().unwrap();
    fixture.invoke(&["workspace", "init", selected]);
    install_guide(&fixture.root);
    fixture.invoke(&["workspace", "switch", selected]);
    let other = fixture.sandbox.path().join("other");
    let explicit = other.to_str().unwrap();
    fixture.invoke(&["workspace", "init", explicit]);
    let empty = fixture.invoke(&["--workspace", explicit, "guides", "list"]);
    assert_eq!(empty["payload"]["data"]["total"], 0, "{empty}");
    let remembered = fixture.invoke(&["guides", "list"]);
    assert_eq!(remembered["payload"]["data"]["total"], 1, "{remembered}");
}

#[test]
fn workspace_switch_preserves_invalid_settings_instead_of_overwriting_them() {
    let fixture = WorkspaceFixture::new("invalid-settings");
    let root = fixture.root.to_str().unwrap();
    fixture.invoke(&["workspace", "init", root]);
    let config = fixture.sandbox.path().join("config");
    fs::create_dir_all(&config).unwrap();
    let settings = config.join("settings.toml");
    let invalid = b"selected_workspace = [\n";
    fs::write(&settings, invalid).unwrap();
    let result = fixture.invoke(&["workspace", "switch", root]);
    assert_eq!(result["type"], "error", "{result}");
    assert_eq!(fs::read(settings).unwrap(), invalid);
}

#[test]
fn workspace_paths_cannot_escape_the_selected_root() {
    let fixture = WorkspaceFixture::new("confined");
    let root = fixture.root.to_str().unwrap();
    fixture.invoke(&["workspace", "init", root]);
    let manifest_path = fixture.root.join("workspace.toml");
    let original = fs::read_to_string(&manifest_path).unwrap();
    for escape in ["../outside", "/tmp/outside"] {
        let modified = original.replace(
            "config_path = \"config\"",
            &format!("config_path = \"{escape}\""),
        );
        fs::write(&manifest_path, modified).unwrap();
        let result = fixture.invoke(&["--workspace", root, "guides", "list"]);
        assert_eq!(result["type"], "error", "{result}");
    }
}

#[cfg(unix)]
#[test]
fn workspace_paths_reject_symlinks_to_another_workspace() {
    let fixture = WorkspaceFixture::new("symlinked");
    let root = fixture.root.to_str().unwrap();
    fixture.invoke(&["workspace", "init", root]);
    redirect_config(&fixture);
    let result = fixture.invoke(&["--workspace", root, "guides", "list"]);
    assert_eq!(result["type"], "error", "{result}");
}

#[cfg(unix)]
fn redirect_config(fixture: &WorkspaceFixture) {
    let other = fixture.sandbox.path().join("outside");
    let config = fixture.root.join("config");
    fs::create_dir(&other).unwrap();
    fs::remove_dir_all(&config).unwrap();
    std::os::unix::fs::symlink(other, config).unwrap();
}

fn legacy_inspection_paths(fixture: &WorkspaceFixture) -> PathBuf {
    std::fs::create_dir_all(&fixture.root).unwrap();
    let config = fixture.sandbox.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let settings = format!("library = {:?}\n", fixture.root.to_str().unwrap());
    std::fs::write(config.join("settings.toml"), settings).unwrap();
    config
}
