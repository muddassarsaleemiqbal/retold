//! `account limit` changes one scope and window in the installation account; dry runs and
//! invalid inputs write nothing. No provider is contacted.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

const INPUT_ERROR: i32 = 2;

fn invoke(home: &Path, args: &[&str]) -> (String, i32) {
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .env("HOME", home)
        .env_remove("XDG_DATA_HOME")
        .env("PATH", "");
    let output = command.args(args).output().unwrap();
    assert_eq!(output.stderr, Vec::<u8>::new());
    let stdout = String::from_utf8(output.stdout).unwrap();
    (stdout, output.status.code().unwrap())
}

fn limit(home: &Path, args: &[&str]) -> (Value, i32) {
    let mut arguments = vec!["--json", "account", "limit"];
    arguments.extend_from_slice(args);
    let (stdout, exit) = invoke(home, &arguments);
    (serde_json::from_str(&stdout).unwrap(), exit)
}

fn limits(home: &Path) -> Value {
    let (stdout, _) = invoke(home, &["--json", "account", "show"]);
    let show: Value = serde_json::from_str(&stdout).unwrap();
    show["payload"]["data"]["limits"].clone()
}

#[test]
fn one_limit_changes_and_merges_while_dry_runs_write_nothing() {
    let home = tempfile::tempdir().unwrap();
    invoke(home.path(), &["account", "init"]);
    let (set, exit) = limit(
        home.path(),
        &["account", "day", "--usd", "5", "--requests", "200"],
    );
    assert_eq!(exit, 0, "{set}");
    let changed = &set["payload"]["data"]["limit_changed"];
    assert_eq!(changed["after"]["usd_micros"], 5_000_000);
    assert_eq!(changed["headroom"]["requests"]["available"], 200);
    let before = limits(home.path());
    let (preview, _) = limit(home.path(), &["account", "day", "--usd", "0", "--dry-run"]);
    let preview = &preview["payload"]["data"]["limit_changed"];
    assert_eq!(preview["exhausted"], serde_json::json!(["usd_micros"]));
    assert_eq!(limits(home.path()), before, "dry runs write nothing");
    let provider = [
        "provider:openrouter:KEY",
        "lifetime",
        "--usd",
        "2",
        "--requests",
        "9",
    ];
    let (_, exit) = limit(home.path(), &provider);
    assert_eq!(exit, 0);
    assert_plain_changes(home.path());
}

/// Raises the account day limit, then removes it, leaving only the provider limit.
fn assert_plain_changes(home: &Path) {
    let (plain, _) = invoke(
        home,
        &["account", "limit", "account", "day", "--usd", "7.5"],
    );
    assert!(
        plain.contains("Account day limit: $5.00 · 200 requests → $7.50 · 200 requests"),
        "{plain}"
    );
    let (removed, _) = invoke(home, &["account", "limit", "account", "day", "--remove"]);
    assert!(
        removed.contains("Paid work that requires this limit is now blocked."),
        "{removed}"
    );
    let remaining = limits(home);
    assert_eq!(remaining.as_array().unwrap().len(), 1);
    assert_eq!(remaining[0]["scope"]["id"], "openrouter:KEY");
}

#[test]
fn invalid_limit_changes_are_input_errors_that_write_nothing() {
    let home = tempfile::tempdir().unwrap();
    invoke(home.path(), &["account", "init"]);
    let refusals: &[&[&str]] = &[
        &["account", "day", "--usd", "5"],
        &["account", "day"],
        &["account", "week", "--usd", "5", "--requests", "1"],
        &["video:../x", "lifetime", "--usd", "5", "--requests", "1"],
        &[
            "account",
            "day",
            "--resource",
            "cats=3",
            "--usd",
            "1",
            "--requests",
            "1",
        ],
        &["account", "month", "--remove"],
    ];
    for args in refusals {
        let (refused, exit) = limit(home.path(), args);
        assert_eq!(exit, INPUT_ERROR, "{args:?} {refused}");
    }
    assert_eq!(limits(home.path()), serde_json::json!([]));
}
