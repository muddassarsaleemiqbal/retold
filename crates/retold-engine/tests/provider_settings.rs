//! Provider imports preserve prior bytes, validate before publication and serialize writers.
#![expect(clippy::unwrap_used, reason = "fixture assertions")]
use std::path::{Path, PathBuf};

use retold_engine::provider_settings::{AddError, AddRequest, add, ensure, load};

const FUTURE: &str = "schema_version = 99\n[providers]\n[routing]\n";
const ORIGINAL: &str = "# preserve exactly in backup\n[providers.existing]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]\n\"text.smart\" = ['existing:smart']\n";
const PROVIDER_TOML: &str =
    "kind = 'codex_cli'\nmax_parallel = 2\nconfig_dir = '/profiles/personal'\n";

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("config");
    std::fs::create_dir(&root).unwrap();
    let definition = temporary.path().join("provider.toml");
    std::fs::write(&definition, PROVIDER_TOML).unwrap();
    (temporary, root, definition)
}

#[test]
fn dry_run_writes_nothing_and_import_retains_exact_backup() {
    let (_temporary, root, definition) = fixture();
    let target = root.join("providers.toml");
    std::fs::write(&target, ORIGINAL).unwrap();
    let preview = add(&AddRequest::new(&root, "personal", &definition).preview()).unwrap();
    assert!(preview.dry_run);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    let result = add(&AddRequest::new(&root, "personal", &definition)).unwrap();
    let backup = result.backup.unwrap();
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), ORIGINAL);
    assert!(add(&AddRequest::new(&root, "personal", &definition)).is_err());
    add(&AddRequest::new(&root, "second", &definition)).unwrap();
    assert_eq!(std::fs::read_to_string(backup).unwrap(), ORIGINAL);
    assert_config(&target);
}

fn assert_config(target: &Path) {
    const EXPECTED_PROVIDERS: usize = 3;
    let text = std::fs::read_to_string(target).unwrap();
    let config: retold_providers::ProvidersConfig = toml::from_str(&text).unwrap();
    assert_eq!(config.providers.len(), EXPECTED_PROVIDERS);
    assert!(config.providers.contains_key("personal"));
    assert!(config.providers.contains_key("second"));
    let chain = &config.routing[&retold_core::Tier::TextSmart];
    assert_eq!(chain[0].provider, "existing");
    assert_eq!(chain[0].selector, "smart");
}

#[test]
fn invalid_definition_and_future_schema_preserve_the_target() {
    let (_temporary, root, definition) = fixture();
    let target = root.join("providers.toml");
    std::fs::write(&target, ORIGINAL).unwrap();
    for text in [
        "kind = 'codex_cli'\nmax_parallel = 0",
        "secret = 'NEVER-ECHO'",
    ] {
        std::fs::write(&definition, text).unwrap();
        assert!(add(&AddRequest::new(&root, "personal", &definition)).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), ORIGINAL);
    }
    std::fs::write(&definition, PROVIDER_TOML).unwrap();

    std::fs::write(&target, FUTURE).unwrap();
    assert!(add(&AddRequest::new(&root, "personal", &definition)).is_err());
    assert_eq!(std::fs::read_to_string(target).unwrap(), FUTURE);
}

#[test]
fn writer_contention_blocks_instead_of_overwriting() {
    let (_temporary, root, definition) = fixture();
    let lock = std::fs::File::create(root.join(".providers.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(matches!(
        add(&AddRequest::new(&root, "personal", &definition)),
        Err(AddError::Busy)
    ));
    assert!(!root.join("providers.toml").exists());
    drop(lock);
    let added = add(&AddRequest::new(&root, "personal", &definition)).unwrap();
    assert!(added.backup.is_none());
}

#[cfg(unix)]
#[test]
fn symlink_targets_are_refused_without_touching_referents() {
    let (_temporary, root, definition) = fixture();
    let target = root.join("providers.toml");
    std::os::unix::fs::symlink(&definition, &target).unwrap();
    assert!(add(&AddRequest::new(&root, "personal", &definition)).is_err());
    assert_eq!(std::fs::read_to_string(definition).unwrap(), PROVIDER_TOML);
}

#[test]
fn invalid_ids_and_oversized_definitions_create_no_configuration() {
    const OVERSIZED_BYTES: usize = 2 * 1024 * 1024 + 1;
    let (_temporary, root, definition) = fixture();
    for id in ["../escape", "", "line\nbreak"] {
        assert!(add(&AddRequest::new(&root, id, &definition)).is_err());
    }
    std::fs::write(&definition, vec![b' '; OVERSIZED_BYTES]).unwrap();
    assert!(add(&AddRequest::new(&root, "personal", &definition)).is_err());
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}

#[test]
fn editing_previews_without_writes_then_preserves_routes_and_exact_backup() {
    use retold_engine::provider_settings::edit;
    let (_temporary, root, definition) = fixture();
    let target = root.join("providers.toml");
    std::fs::write(&target, ORIGINAL).unwrap();
    let request = AddRequest::new(&root, "existing", &definition);
    edit(&AddRequest::new(&root, "existing", &definition).preview()).unwrap();
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    let changed = edit(&request).unwrap();
    assert_eq!(
        std::fs::read_to_string(changed.backup.unwrap()).unwrap(),
        ORIGINAL
    );
    let text = std::fs::read_to_string(target).unwrap();
    let actual: retold_providers::ProvidersConfig = toml::from_str(&text).unwrap();
    let before: retold_providers::ProvidersConfig = toml::from_str(ORIGINAL).unwrap();
    assert_eq!(actual.routing, before.routing);
    assert_eq!(actual.providers["existing"].max_parallel(), 2);
}

#[test]
fn editing_refuses_incompatible_fallbacks_missing_ids_and_invalid_sources() {
    use retold_engine::provider_settings::edit;
    let (_temporary, root, definition) = fixture();
    let target = root.join("providers.toml");
    let source = ORIGINAL.replace(
        "[routing]",
        "[providers.other]\nkind = 'codex_cli'\nmax_parallel = 1\n[routing]",
    );
    let source = format!("{source}\nimage = ['other:default', 'existing:default']\n");
    std::fs::write(&target, &source).unwrap();
    assert!(edit(&AddRequest::new(&root, "missing", &definition)).is_err());
    std::fs::write(&definition, "kind = 'claude_cli'\nmax_parallel = 1\n").unwrap();
    assert!(edit(&AddRequest::new(&root, "existing", &definition)).is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), source);
    std::fs::write(&target, FUTURE).unwrap();
    assert!(edit(&AddRequest::new(&root, "existing", &definition)).is_err());
    assert_eq!(std::fs::read_to_string(target).unwrap(), FUTURE);
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 2);
}

#[test]
fn editing_obeys_writer_lock_and_preserves_other_definitions() {
    use retold_engine::provider_settings::edit;
    let (_temporary, root, definition) = fixture();
    std::fs::write(root.join("providers.toml"), ORIGINAL).unwrap();
    add(&AddRequest::new(&root, "other", &definition)).unwrap();
    let lock = std::fs::File::open(root.join(".providers.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let request = AddRequest::new(&root, "existing", &definition);
    assert!(matches!(edit(&request), Err(AddError::Busy)));
    drop(lock);
    edit(&request).unwrap();
    assert_other_preserved(&root);
}

fn assert_other_preserved(root: &Path) {
    let text = std::fs::read_to_string(root.join("providers.toml")).unwrap();
    let actual: retold_providers::ProvidersConfig = toml::from_str(&text).unwrap();
    assert_eq!(actual.providers.len(), 2);
    assert_eq!(actual.providers["other"].max_parallel(), 2);
}

#[test]
fn fresh_install_creates_the_folder_and_file_on_first_write_only() {
    let (temporary, _root, definition) = fixture();
    let fresh = temporary.path().join("missing").join("retold");
    assert!(load(&fresh).unwrap().providers.is_empty());
    add(&AddRequest::new(&fresh, "personal", &definition).preview()).unwrap();
    assert!(!fresh.exists());
    add(&AddRequest::new(&fresh, "personal", &definition)).unwrap();
    let config = load(&fresh).unwrap();
    assert!(config.providers.contains_key("personal"));
    assert!(config.routing.is_empty());
}

#[test]
fn ensure_writes_an_empty_configuration_once_and_never_replaces_one() {
    let (temporary, root, _definition) = fixture();
    let fresh = temporary.path().join("new");
    assert!(ensure(&fresh).unwrap());
    assert!(load(&fresh).unwrap().providers.is_empty());
    assert!(!ensure(&fresh).unwrap());
    let target = root.join("providers.toml");
    std::fs::write(&target, ORIGINAL).unwrap();
    assert!(!ensure(&root).unwrap());
    assert_eq!(std::fs::read_to_string(target).unwrap(), ORIGINAL);
}
