//! Process inspection is a bounded read-only view of supervised workers and retained uncertainty.
mod common;

use common::invoke;

#[test]
fn empty_processes_inventory_is_readable_and_bounds_are_refused() {
    let home = tempfile::tempdir().unwrap();
    invoke(home.path(), &["account", "init"], 0);

    let stdout = invoke(home.path(), &["--json", "account", "processes"], 0);
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["payload"]["data"]["processes"], serde_json::json!([]));
    assert_eq!(
        value["payload"]["data"]["next_after"],
        serde_json::json!(null)
    );

    let plain = invoke(home.path(), &["account", "processes"], 0);
    assert!(plain.contains("No supervised processes are recorded."));
    assert!(plain.contains("nothing was signalled or changed."));

    for limit in ["0", "101"] {
        invoke(
            home.path(),
            &["--json", "account", "processes", "--limit", limit],
            2,
        );
    }
}
