//! Finding installed provider CLIs and their versions, for `doctor`.

use std::path::PathBuf;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::process::{self, Limits};

/// A CLI Retold can use, and whether it is installed.
#[derive(Debug, Clone)]
pub struct CliInfo {
    pub binary: &'static str,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
}

impl CliInfo {
    #[must_use]
    pub const fn installed(&self) -> bool {
        self.path.is_some()
    }
}

/// Looks `binary` up on `PATH` and asks it for `--version`.
pub async fn detect(binary: &'static str) -> CliInfo {
    let Ok(path) = which::which(binary) else {
        return CliInfo {
            binary,
            path: None,
            version: None,
        };
    };
    let limits = Limits {
        timeout: Duration::from_secs(10),
        max_stdout: 64 * 1024,
        env: Vec::new(),
    };
    let cwd = std::env::temp_dir();
    let version = process::run(
        &path,
        &["--version".to_owned()],
        "",
        &cwd,
        &limits,
        &CancellationToken::new(),
    )
    .await
    .ok()
    .and_then(|o| {
        let text = if o.stdout.trim().is_empty() {
            o.stderr
        } else {
            o.stdout
        };
        text.lines().next().map(|l| l.trim().to_owned())
    })
    .filter(|v| !v.is_empty());
    CliInfo {
        binary,
        path: Some(path),
        version,
    }
}

/// Pulls a dotted version number out of text like `codex-cli 0.161.0`.
#[must_use]
pub fn version_number(text: &str) -> Option<&str> {
    text.split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .find(|w| {
            w.split('.').count() >= 2
                && w.split('.')
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_version_numbers() {
        assert_eq!(version_number("codex-cli 0.161.0"), Some("0.161.0"));
        assert_eq!(version_number("2.1.296 (Claude Code)"), Some("2.1.296"));
        assert_eq!(version_number("no version"), None);
    }

    #[tokio::test]
    async fn missing_binary_is_not_installed() {
        assert!(!detect("retold-definitely-not-a-binary").await.installed());
    }
}
