//! Attempt inspection selects only the canonical installation account and never reconciles.
mod common;

use common::invoke;

#[test]
fn empty_attempt_inventory_is_readable_and_invalid_filters_are_refused() {
    let home = tempfile::tempdir().unwrap();
    invoke(home.path(), &["account", "init"], 0);
    let stdout = invoke(home.path(), &["--json", "account", "attempts"], 0);
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        value["payload"]["data"]["account_attempts"]["attempts"],
        serde_json::json!([])
    );
    invoke(
        home.path(),
        &["--json", "account", "attempts", "--limit", "0"],
        2,
    );
    invoke(
        home.path(),
        &["--json", "account", "attempts", "--state", "invented"],
        2,
    );
    let plain = invoke(home.path(), &["--plain", "account", "attempts"], 0);
    assert!(plain.contains("No recorded attempts"));
}
