//! Rehashed tampering must fail semantic checks, rather than only checksum verification.
use super::*;

#[test]
fn managed_backups_report_semantic_verification_without_changing_archive() {
    let fixture = backed_up();
    let before = std::fs::read(fixture.output().join("index.json")).unwrap();
    let verified = storage::verify(&fixture.output()).unwrap();
    assert_eq!(verified.index.schema_version, 2);
    assert!(verified.managed_workspace_verified);
    assert!(!verified.execution_authorized);
    assert_eq!(
        std::fs::read(fixture.output().join("index.json")).unwrap(),
        before
    );
}

#[test]
fn rehashed_lineage_disagreements_are_rejected() {
    for (field, value) in [
        ("schema_version", json!(2)),
        ("archive_root", json!("elsewhere")),
        ("account_snapshot", json!("other-account")),
        ("execution_authorized", json!(true)),
        ("video_ids", json!([])),
        ("video_ids", json!(["video-fixture", "video-fixture"])),
        ("omitted_paths", json!(["../secret"])),
        ("unknown", json!(true)),
    ] {
        let fixture = backed_up();
        rewrite(&fixture.output(), "workspace.json", |lineage| {
            lineage[field] = value;
        });
        assert!(storage::verify(&fixture.output()).is_err(), "{field}");
    }
}

#[test]
fn account_identity_and_original_registration_must_agree() {
    let fixture = backed_up();
    rewrite(&fixture.output(), "workspace.json", |lineage| {
        lineage["workspace"]["root"] = json!("/different-original-root");
    });
    assert!(storage::verify(&fixture.output()).is_err());
    let fixture = backed_up();
    let path = fixture.output().join("index.json");
    let mut index = read_json(&path);
    index["account_id"] = json!("another-account");
    save_json(&path, &index);
    assert!(storage::verify(&fixture.output()).is_err());
}

#[test]
fn rehashed_invalid_run_and_account_manifests_are_rejected() {
    for path in [
        "workspace/channel/story/.retold/run.json",
        "account/backup.json",
    ] {
        let fixture = backed_up();
        rewrite(&fixture.output(), path, |value| *value = json!({}));
        assert!(storage::verify(&fixture.output()).is_err(), "{path}");
    }
}

#[test]
fn rehashed_state_without_cache_evidence_is_rejected() {
    let fixture = fixture();
    save_json(
        &fixture.metadata.join("state.json"),
        &json!({"schema":1,"active":null,"error":null,"completed":{}}),
    );
    let (result, exit) = fixture.invoke(&[]);
    assert_eq!(exit, 0, "{result}");
    rewrite(
        &fixture.output(),
        "workspace/channel/story/.retold/state.json",
        |state| {
            let digest = "a".repeat(HASH_LENGTH);
            state["completed"] =
                json!({"research":{"key":digest,"output_hash":digest,"receipt":null}});
        },
    );
    assert!(storage::verify(&fixture.output()).is_err());
}

#[test]
fn checksummed_unmanaged_files_are_rejected() {
    let fixture = backed_up();
    let root = fixture.output();
    save_json(&root.join("workspace/unmanaged.json"), &json!({}));
    add_index_entry(&root, "workspace/unmanaged.json");
    assert!(storage::verify(&root).is_err());
}

fn rewrite(root: &Path, relative: &str, mutate: impl FnOnce(&mut Value)) {
    let path = root.join(relative);
    let mut value = read_json(&path);
    mutate(&mut value);
    save_json(&path, &value);
    let path = root.join("index.json");
    let mut index = read_json(&path);
    for recorded in index["files"].as_array_mut().unwrap() {
        let selected = recorded["path"] == relative;
        if selected {
            *recorded = entry(root, relative);
        }
    }
    save_json(&path, &index);
}

fn entry(root: &Path, relative: &str) -> Value {
    let bytes = std::fs::read(root.join(relative)).unwrap();
    json!({"path":relative,"bytes":bytes.len(),"blake3":blake3::hash(&bytes).to_hex().to_string()})
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn save_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn add_index_entry(root: &Path, relative: &str) {
    let path = root.join("index.json");
    let mut index = read_json(&path);
    index["files"]
        .as_array_mut()
        .unwrap()
        .push(entry(root, relative));
    save_json(&path, &index);
}
