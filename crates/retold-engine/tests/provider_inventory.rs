//! Inventory is a projection, never credential or login discovery.
use retold_engine::provider_inventory::list;

const SOURCE: &str = r#"
[providers.primary]
kind = "codex_cli"
config_dir = "/PRIVATE-PROFILE"
binary = "/PRIVATE-BINARY"
max_parallel = 2
[providers.primary.tiers]
smart = "PRIVATE-MODEL"
[providers.fallback]
kind = "openrouter"
key_env = "PRIVATE-KEY-NAME"
base_url = "https://PRIVATE-ENDPOINT"
max_parallel = 1
[routing]
"text.smart" = ["primary:PRIVATE-SELECTOR", "fallback:PRIVATE-SELECTOR"]
image = ["primary:default"]
"#;

#[test]
fn inventory_preserves_order_assignments_and_redacts_provider_settings() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("providers.toml");
    std::fs::write(&target, SOURCE).unwrap();
    let providers = list(root.path()).unwrap();
    assert_eq!(providers.len(), 2);
    assert_eq!(providers[0].id, "primary");
    assert_eq!(providers[0].max_parallel, 2);
    assert_eq!(providers[1].kind, "openrouter");
    assert_eq!(providers[1].routes[0].position, 2);
    assert_eq!(providers[0].health, "not_checked");
    assert_eq!(providers[0].authentication, "not_checked");
    assert!(
        providers[0]
            .supported_tiers
            .contains(&retold_core::Tier::Image)
    );
    let serialized = serde_json::to_string(&providers).unwrap();
    assert!(!serialized.contains("PRIVATE"));
    assert_eq!(std::fs::read_to_string(target).unwrap(), SOURCE);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn empty_missing_invalid_and_future_configuration_are_distinct() {
    let root = tempfile::tempdir().unwrap();
    assert!(list(root.path()).is_err());
    let target = root.path().join("providers.toml");
    std::fs::write(&target, "[providers]\n[routing]\n").unwrap();
    assert_eq!(list(root.path()).unwrap().len(), 0);
    for invalid in [
        "schema_version = 99\n[providers]\n[routing]",
        "PRIVATE-INVALID",
    ] {
        std::fs::write(&target, invalid).unwrap();
        let error = list(root.path()).unwrap_err();
        assert!(!error.to_string().contains("PRIVATE"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), invalid);
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
