//! Diagnostic checks for workspace integrity.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::account::Account;
use crate::protocol::{Diagnostic, ErrorCode, Outcome, Payload};
use crate::workspace::resolution::resolve_workspace_root;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckItem {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

fn validate_manifest(root: &Path) -> CheckItem {
    match retold_engine::Paths::from_workspace(root) {
        Ok(_) => CheckItem {
            name: "manifest_file",
            passed: true,
            detail: "workspace.toml and its confined paths are valid".into(),
        },
        Err(err) => CheckItem {
            name: "manifest_file",
            passed: false,
            detail: format!("failed to parse workspace.toml: {err}"),
        },
    }
}

fn check_manifest(root: &Path) -> CheckItem {
    let manifest_file = root.join("workspace.toml");
    if !manifest_file.is_file() {
        return CheckItem {
            name: "manifest_file",
            passed: false,
            detail: "workspace.toml not found".to_string(),
        };
    }

    validate_manifest(root)
}

fn check_directory(target: &Path, check_name: &'static str) -> CheckItem {
    if target.is_dir() {
        CheckItem {
            name: check_name,
            passed: true,
            detail: format!("{} directory exists", target.display()),
        }
    } else {
        CheckItem {
            name: check_name,
            passed: false,
            detail: format!("{} directory missing", target.display()),
        }
    }
}

fn is_directory_writable(dir: &Path) -> bool {
    fs::metadata(dir).is_ok_and(|metadata| metadata.is_dir() && !metadata.permissions().readonly())
}

fn check_writable(root: &Path) -> CheckItem {
    let write_ok = is_directory_writable(root);
    let detail = if write_ok {
        "Workspace root exists and is not marked read-only; mutations recheck access".to_string()
    } else {
        "Workspace root is not writable (permission error)".to_string()
    };
    CheckItem {
        name: "writable",
        passed: write_ok,
        detail,
    }
}

fn check_account_registration(root: &Path) -> CheckItem {
    let is_registered = crate::account::canonical_directory()
        .ok()
        .and_then(|dir| Account::open(&dir).ok())
        .and_then(|acc| acc.workspace_at(root).ok().flatten())
        .is_some();

    CheckItem {
        name: "account_binding",
        passed: true,
        detail: if is_registered {
            "Workspace is registered with canonical account".to_string()
        } else {
            "Workspace is ready for account binding or local offline use".to_string()
        },
    }
}

fn run_checks(root: &Path) -> Vec<CheckItem> {
    let paths = super::resolution::derive_workspace_paths(root, "inspection");
    let (config, library) = paths.map_or_else(
        |_| (root.join("config"), root.join("library")),
        |paths| (paths.config, paths.library),
    );
    check_paths(root, (&config, &library))
}

fn check_paths(root: &Path, paths: (&Path, &Path)) -> Vec<CheckItem> {
    let (config, library) = paths;
    vec![
        check_manifest(root),
        check_directory(config, "config_dir"),
        check_directory(library, "library_dir"),
        check_writable(root),
        check_account_registration(root),
    ]
}

#[must_use]
pub fn check_context(context: &super::context::Context) -> Payload {
    let checks = check_paths(&context.root, (&context.config, &context.library));
    format_check_payload(&context.root, &checks)
}

fn format_check_payload(root: &Path, checks: &[CheckItem]) -> Payload {
    let all_passed = checks.iter().all(|item| item.passed);
    let outcome = if all_passed {
        Outcome::Completed
    } else {
        Outcome::Blocked
    };

    let passed_count = checks.iter().filter(|item| item.passed).count();
    let total_count = checks.len();
    let text = format!(
        "Workspace check at '{}': {passed_count}/{total_count} checks passed.",
        root.display()
    );

    Payload::Result {
        outcome,
        contents: serde_json::json!({
            "text": text,
            "workspace_root": root,
            "all_passed": all_passed,
            "checks": checks,
        }),
    }
}

#[must_use]
pub fn check_workspace(explicit_path: Option<&Path>) -> Payload {
    let Some(resolved_root) = resolve_workspace_root(explicit_path) else {
        return Diagnostic::new(
            ErrorCode::InputRequired,
            "Could not resolve workspace. Run 'retold workspace init <path>' or specify --workspace.",
        )
        .into_payload();
    };

    let checks = run_checks(&resolved_root);
    format_check_payload(&resolved_root, &checks)
}
