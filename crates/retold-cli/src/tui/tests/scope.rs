//! Session paths also own local preferences; an invalid layout never falls back to another root.
use super::*;
use serde_json::json;

fn split_scope(root: &std::path::Path) -> Scope {
    let config = root.join("config");
    let library = root.join("media");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    Scope {
        root: Option::default(),
        selected: OnceLock::from(Context {
            root: library.clone(),
            config,
            library,
            managed: false,
        }),
    }
}

fn contents(payload: Payload) -> serde_json::Value {
    let Payload::Result { contents, .. } = payload else {
        panic!("result")
    };
    contents
}

#[test]
fn local_preference_reads_and_writes_use_the_pinned_config_directory() {
    let sandbox = tempfile::tempdir().unwrap();
    let scope = split_scope(sandbox.path());
    let context = scope.context().unwrap();
    let saved = json!({"last":{"guide":"selected-guide"}});
    write_choices(&context.config, &saved);
    let loaded = contents(scope.local(&Task::LoadDefaults(Option::default())));
    assert_eq!(loaded["defaults"], saved);
    let replacement = json!({"last":{"guide":"new-choice"}});
    let task = Task::SaveDefaults(Option::default(), replacement.to_string());
    assert!(matches!(scope.local(&task), Payload::Result { .. }));
    let loaded = contents(scope.local(&Task::LoadDefaults(Option::default())));
    assert_eq!(loaded["defaults"], replacement);
    assert!(!context.library.join(".tui-defaults.json").exists());
}

fn managed_scope(root: &std::path::Path) -> Scope {
    let manifest = "schema_version = 1\nid = 'fixture-id'\nname = 'Fixture'\nlibrary_path = \"library\"\nconfig_path = 'config'\n";
    std::fs::write(root.join("workspace.toml"), manifest).unwrap();
    Scope::new(Some(root.to_path_buf()))
}

#[test]
fn deleting_a_selected_manifest_fails_closed_but_keeps_global_help_available() {
    let sandbox = tempfile::tempdir().unwrap();
    let scope = managed_scope(sandbox.path());
    assert!(scope.context().unwrap().managed);
    std::fs::remove_file(sandbox.path().join("workspace.toml")).unwrap();
    assert!(scope.context().is_err());
    assert!(matches!(
        scope.request(vec!["retold".into(), "schema".into()]),
        Payload::Result { .. }
    ));
    let result = scope.request(vec!["retold".into(), "providers".into(), "list".into()]);
    assert!(matches!(result, Payload::Error { .. }));
}

#[test]
fn a_changed_managed_layout_requires_an_explicit_new_selection() {
    let sandbox = tempfile::tempdir().unwrap();
    let scope = managed_scope(sandbox.path());
    let path = sandbox.path().join("workspace.toml");
    let changed = std::fs::read_to_string(&path).unwrap().replace(
        "library_path = \"library\"",
        "library_path = \"other-media\"",
    );
    std::fs::write(path, changed).unwrap();
    assert!(scope.context().is_err());
}

fn write_choices(config: &std::path::Path, choices: &serde_json::Value) {
    std::fs::write(config.join(".tui-defaults.json"), choices.to_string()).unwrap();
}
