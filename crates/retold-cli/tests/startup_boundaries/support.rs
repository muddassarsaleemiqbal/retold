use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct Fixture {
    root: tempfile::TempDir,
    pub library: PathBuf,
    pub state: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        Self {
            library: root.path().join("library"),
            state: root.path().join("s"),
            root,
        }
    }

    pub fn inactive_library(mut self) -> Self {
        self.library = self.retired().join("library");
        self
    }

    pub fn inactive_state(mut self) -> Self {
        self.state = self.retired().join("s");
        self
    }

    fn retired(&self) -> PathBuf {
        let retired = self.root.path().join("retired");
        std::fs::create_dir(&retired).unwrap();
        std::fs::write(retired.join(".retold-restore.json"), b"{}").unwrap();
        retired
    }

    pub fn invoke(&self, args: &[&str]) -> (Value, i32) {
        let mut command = self.command();
        let output = command.args(args).output().unwrap();
        (
            serde_json::from_slice(&output.stdout).unwrap(),
            output.status.code().unwrap(),
        )
    }

    fn command(&self) -> Command {
        let mut command = Command::new(assert_cmd::cargo::cargo_bin("retold"));
        let folders = [("HOME", "home"), ("RETOLD_CONFIG_DIR", "config")];
        command.envs(folders.map(|(key, folder)| (key, self.root.path().join(folder))));
        command
            .env("RETOLD_LIBRARY", &self.library)
            .env("RETOLD_STATE", &self.state);
        command
            .env("PATH", "")
            .env_remove("RETOLD_WORKSPACE")
            .env_remove("XDG_DATA_HOME");
        command.arg("--json");
        command
    }

    pub fn account(&self) -> (retold_app::account::Account, PathBuf) {
        let (preview, code) = self.invoke(&["account", "init", "--timezone", "UTC", "--dry-run"]);
        assert_eq!(code, 0, "{preview}");
        let directory = PathBuf::from(preview["payload"]["data"]["directory"].as_str().unwrap());
        assert!(directory.starts_with(self.root.path()));
        let account = retold_app::account::Account::initialize(&directory, "UTC").unwrap();
        (account, directory)
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn mark_inactive(&self) {
        std::fs::write(self.root().join(".retold-restore.json"), b"{}").unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.invoke(&["studio", "stop"]);
    }
}
