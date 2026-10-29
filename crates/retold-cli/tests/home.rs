//! `home` prints the Home screen's snapshot as plain text or JSON; a bare invocation outside a
//! terminal prints usage instead of opening the full-screen app. Nothing is written.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        for folder in ["config", "library"] {
            std::fs::create_dir(root.path().join(folder)).unwrap();
        }
        Self { root }
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.root.path().join(name)
    }

    fn run(&self, args: &[&str]) -> (String, i32) {
        let mut command = Command::cargo_bin("retold").unwrap();
        command.env("HOME", self.root.path());
        command.env("RETOLD_CONFIG_DIR", self.path("config"));
        command.env("RETOLD_LIBRARY", self.path("library"));
        command.env_remove("XDG_DATA_HOME");
        let output = command.args(args).output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        (stdout, output.status.code().unwrap())
    }

    fn files(&self) -> usize {
        count(self.root.path())
    }
}

fn count(path: &Path) -> usize {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() { 1 + count(&path) } else { 1 }
        })
        .sum()
}

#[test]
fn home_sections_report_their_own_failures_and_commands() {
    let sandbox = Sandbox::new();
    let before = sandbox.files();
    let (stdout, exit) = sandbox.run(&["--json", "home"]);
    assert_eq!(exit, 0, "{stdout}");
    let envelope: Value = serde_json::from_str(&stdout).unwrap();
    let home = &envelope["payload"]["data"]["home"];
    assert_eq!(home["videos"]["data"]["total"], 0);
    assert_eq!(home["today"]["error"]["code"], "RG-ACCOUNT-REQUIRED");
    assert_eq!(home["today"]["command"], "retold spend --today");
    let unavailable: Vec<&str> = home["actions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|action| action["available"] == false)
        .map(|action| action["label"].as_str().unwrap())
        .collect();
    assert!(unavailable.is_empty(), "{unavailable:?}");
    assert_eq!(sandbox.files(), before, "home writes nothing");
}

#[test]
fn plain_home_is_the_linear_path_and_bare_pipes_print_usage() {
    let sandbox = Sandbox::new();
    sandbox.run(&["account", "init"]);
    let (plain, exit) = sandbox.run(&["--plain", "home"]);
    assert_eq!(exit, 0);
    for expected in [
        "Today $0.00 · Month $0.00 · no month limit; paid work is blocked",
        "Videos: 0 · showing the 0 most recent",
        "[library] Spend — retold spend",
        "[new] New video — retold new",
        "Read only; nothing was changed.",
    ] {
        assert!(plain.contains(expected), "{expected}\n{plain}");
    }
    let (usage, exit) = sandbox.run(&[]);
    assert_eq!(exit, 0);
    assert!(usage.starts_with("Usage: retold"), "{usage}");
}

const CONFIGURED_PROVIDER: &str =
    "[providers.configured]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]\n";

#[test]
fn separate_library_environment_keeps_the_same_provider_config_as_home() {
    let sandbox = Sandbox::new();
    std::fs::write(sandbox.path("config/providers.toml"), CONFIGURED_PROVIDER).unwrap();
    let before = sandbox.files();
    let (home, exit) = sandbox.run(&["--json", "home"]);
    assert_eq!(exit, 0, "{home}");
    let home: Value = serde_json::from_str(&home).unwrap();
    let (providers, exit) = sandbox.run(&["--json", "providers", "list"]);
    assert_eq!(exit, 0, "{providers}");
    let providers: Value = serde_json::from_str(&providers).unwrap();
    assert_eq!(
        providers["payload"]["data"]["providers"][0]["id"],
        "configured"
    );
    assert_eq!(
        home["payload"]["data"]["home"]["providers"]["data"]["health"][0]["id"],
        "configured"
    );
    assert_eq!(sandbox.files(), before);
}

#[test]
fn explicit_portable_root_does_not_borrow_global_provider_settings() {
    let sandbox = Sandbox::new();
    std::fs::write(sandbox.path("config/providers.toml"), CONFIGURED_PROVIDER).unwrap();
    let portable = CONFIGURED_PROVIDER.replace("configured", "portable");
    std::fs::write(sandbox.path("library/providers.toml"), portable).unwrap();
    let library = sandbox.path("library");
    let (home, exit) = sandbox.run(&["--workspace", library.to_str().unwrap(), "--json", "home"]);
    assert_eq!(exit, 0, "{home}");
    let home: Value = serde_json::from_str(&home).unwrap();
    assert_eq!(
        home["payload"]["data"]["home"]["providers"]["data"]["health"][0]["id"],
        "portable"
    );
}

#[test]
fn invalid_explicit_workspace_does_not_fall_back_to_global_provider_health() {
    let sandbox = Sandbox::new();
    std::fs::write(sandbox.path("config/providers.toml"), CONFIGURED_PROVIDER).unwrap();
    std::fs::write(
        sandbox.path("library/workspace.toml"),
        "schema_version = 999\n",
    )
    .unwrap();
    let library = sandbox.path("library");
    let (home, exit) = sandbox.run(&["--workspace", library.to_str().unwrap(), "--json", "home"]);
    assert_eq!(exit, 0, "{home}");
    let home: Value = serde_json::from_str(&home).unwrap();
    let providers = &home["payload"]["data"]["home"]["providers"];
    assert_eq!(providers["error"]["code"], "RG-DATA-INVALID");
    assert!(providers.get("data").is_none());
}
