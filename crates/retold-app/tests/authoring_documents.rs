//! Structured edits validate before publishing and preserve user-owned sidecar files.
#![expect(clippy::unwrap_used, reason = "isolated fixtures")]
use retold_app::{
    authoring::{self, ChannelOptions, Edit, GuideOptions},
    protocol::Payload,
};
use serde_json::Value;
use std::path::Path;

fn contents(payload: Payload) -> Value {
    match payload {
        Payload::Result { contents, .. } => Some(contents),
        _ => None,
    }
    .unwrap()
}
fn hash(root: &Path, kind: &str) -> String {
    contents(authoring::document(root, kind, "example").unwrap())["document"]["hash"]
        .as_str()
        .unwrap()
        .into()
}
fn create(root: &Path, kind: &str) {
    if kind == "guides" {
        authoring::create_guide(
            root,
            &GuideOptions {
                id: "example".into(),
                language: Some("en".into()),
                ..Default::default()
            },
        )
        .unwrap();
    } else {
        authoring::create_channel(
            root,
            &ChannelOptions {
                id: "example".into(),
                language: Some("en".into()),
                ..Default::default()
            },
        )
        .unwrap();
    }
}
#[test]
fn edit_preview_preserves_bytes_and_rejects_stale_apply_for_both_resource_kinds() {
    isolated(
        "edit_preview_preserves_bytes_and_rejects_stale_apply_for_both_resource_kinds",
        verify_edit_preview_preserves_bytes_and_rejects_stale_apply_for_both_resource_kinds,
    );
}

fn verify_edit_preview_preserves_bytes_and_rejects_stale_apply_for_both_resource_kinds() {
    for kind in ["guides", "channels"] {
        assert_preview_and_apply(kind);
    }
}

fn assert_preview_and_apply(kind: &str) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    create(root, kind);
    let path = manifest_path(root, kind);
    let original = std::fs::read(&path).unwrap();
    let reviewed = hash(root, kind);
    let edit = Edit {
        field: "/name",
        value: "\"New title\"",
        expected_hash: &reviewed,
        dry_run: true,
    };
    let preview = contents(authoring::edit_document(root, (kind, "example"), &edit).unwrap());
    assert_eq!(preview["document_changed"]["after"], "New title");
    assert_eq!(std::fs::read(&path).unwrap(), original);
    authoring::edit_document(
        root,
        (kind, "example"),
        &Edit {
            dry_run: false,
            ..edit
        },
    )
    .unwrap();
    assert!(authoring::edit_document(root, (kind, "example"), &edit).is_err());
    let document = contents(authoring::document(root, kind, "example").unwrap());
    assert_eq!(document["document"]["value"]["name"], "New title");
}
#[test]
fn invalid_schema_and_identity_edits_preserve_files() {
    isolated(
        "invalid_schema_and_identity_edits_preserve_files",
        verify_invalid_schema_and_identity_edits_preserve_files,
    );
}

fn verify_invalid_schema_and_identity_edits_preserve_files() {
    let directory = tempfile::tempdir().unwrap();
    create(directory.path(), "channels");
    let reviewed = hash(directory.path(), "channels");
    for (field, value) in [
        ("/handle", "\"invalid\""),
        ("/id", "\"other\""),
        ("/name", "null"),
    ] {
        let edit = Edit {
            field,
            value,
            expected_hash: &reviewed,
            dry_run: false,
        };
        assert!(
            authoring::edit_document(directory.path(), ("channels", "example"), &edit).is_err()
        );
        assert_eq!(hash(directory.path(), "channels"), reviewed);
    }
}
#[test]
fn guide_sidecars_survive_manifest_edit() {
    isolated(
        "guide_sidecars_survive_manifest_edit",
        verify_guide_sidecars_survive_manifest_edit,
    );
}

fn verify_guide_sidecars_survive_manifest_edit() {
    let directory = tempfile::tempdir().unwrap();
    create(directory.path(), "guides");
    let sidecar = directory.path().join("guides/example/voice.md");
    let before = std::fs::read(&sidecar).unwrap();
    let reviewed = hash(directory.path(), "guides");
    authoring::edit_document(
        directory.path(),
        ("guides", "example"),
        &Edit {
            field: "/name",
            value: "\"Different\"",
            expected_hash: &reviewed,
            dry_run: false,
        },
    )
    .unwrap();
    assert_eq!(std::fs::read(sidecar).unwrap(), before);
}

fn manifest_path(root: &Path, kind: &str) -> std::path::PathBuf {
    let filename = if kind == "guides" {
        "guide.toml"
    } else {
        "channel.toml"
    };
    root.join(kind).join("example").join(filename)
}

fn isolated(name: &str, verify: fn()) {
    let child = std::env::var("RETOLD_DOCUMENT_TEST_CHILD").ok();
    if child.as_deref() == Some(name) {
        verify();
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let mut command = std::process::Command::new(executable);
    command.args(["--exact", name, "--nocapture"]);
    command.env("HOME", home.path()).env_remove("XDG_DATA_HOME");
    command.env("RETOLD_DOCUMENT_TEST_CHILD", name);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn list_edits_validate_elements_before_publication() {
    isolated(
        "list_edits_validate_elements_before_publication",
        verify_list_edits,
    );
}
fn verify_list_edits() {
    let directory = tempfile::tempdir().unwrap();
    create(directory.path(), "guides");
    let reviewed = hash(directory.path(), "guides");
    let edit = Edit {
        field: "/packaging/platforms",
        value: "[true]",
        expected_hash: &reviewed,
        dry_run: false,
    };
    assert!(authoring::edit_document(directory.path(), ("guides", "example"), &edit).is_err());
    assert_eq!(hash(directory.path(), "guides"), reviewed);
    let valid = Edit {
        value: "[\"youtube\",\"tiktok\"]",
        ..edit
    };
    authoring::edit_document(directory.path(), ("guides", "example"), &valid).unwrap();
    let value = contents(authoring::document(directory.path(), "guides", "example").unwrap());
    assert_eq!(
        value["document"]["value"]["packaging"]["platforms"],
        serde_json::json!(["youtube", "tiktok"])
    );
}

#[test]
fn omitted_schema_fields_can_be_added_but_unknown_fields_are_rejected() {
    isolated(
        "omitted_schema_fields_can_be_added_but_unknown_fields_are_rejected",
        verify_optional_field,
    );
}
fn verify_optional_field() {
    let directory = tempfile::tempdir().unwrap();
    create(directory.path(), "guides");
    let reviewed = hash(directory.path(), "guides");
    let edit = Edit {
        field: "/visuals/panels",
        value: "2",
        expected_hash: &reviewed,
        dry_run: true,
    };
    let preview =
        contents(authoring::edit_document(directory.path(), ("guides", "example"), &edit).unwrap());
    assert!(preview["document_changed"]["before"].is_null());
    assert_eq!(hash(directory.path(), "guides"), reviewed);
    let unknown = Edit {
        field: "/visuals/invented",
        ..edit
    };
    assert!(authoring::edit_document(directory.path(), ("guides", "example"), &unknown).is_err());
    let applied = Edit {
        dry_run: false,
        ..edit
    };
    authoring::edit_document(directory.path(), ("guides", "example"), &applied).unwrap();
    let document = contents(authoring::document(directory.path(), "guides", "example").unwrap());
    assert_eq!(document["document"]["value"]["visuals"]["panels"], 2);
}

#[test]
fn omitted_object_sections_validate_before_being_added() {
    isolated(
        "omitted_object_sections_validate_before_being_added",
        verify_object_section,
    );
}
fn verify_object_section() {
    let directory = tempfile::tempdir().unwrap();
    create(directory.path(), "guides");
    let reviewed = hash(directory.path(), "guides");
    let edit = Edit {
        field: "/research/canon",
        value: "{}",
        expected_hash: &reviewed,
        dry_run: false,
    };
    assert!(authoring::edit_document(directory.path(), ("guides", "example"), &edit).is_err());
    assert_eq!(hash(directory.path(), "guides"), reviewed);
    let valid = Edit {
        value: r#"{"work_input":"topic","part_input":"topic"}"#,
        ..edit
    };
    authoring::edit_document(directory.path(), ("guides", "example"), &valid).unwrap();
    let document = contents(authoring::document(directory.path(), "guides", "example").unwrap());
    assert_eq!(
        document["document"]["value"]["research"]["canon"]["work_input"],
        "topic"
    );
}
#[test]
fn offered_missing_sections_preview_with_required_members() {
    isolated(
        "offered_missing_sections_preview_with_required_members",
        verify_offered_missing_sections_preview_with_required_members,
    );
}

fn verify_offered_missing_sections_preview_with_required_members() {
    for kind in ["guides", "channels"] {
        let directory = tempfile::tempdir().unwrap();
        create(directory.path(), kind);
        let document = contents(authoring::document(directory.path(), kind, "example").unwrap());
        let reviewed = hash(directory.path(), kind);
        let missing = document["document"]["missing"].as_array().unwrap();
        let sections: Vec<_> = missing
            .iter()
            .filter(|field| field["value"].is_object())
            .collect();
        assert!(!sections.is_empty(), "{kind} offers no missing sections");
        for field in sections {
            let edit = Edit {
                field: field["path"].as_str().unwrap(),
                value: &field["value"].to_string(),
                expected_hash: &reviewed,
                dry_run: true,
            };
            let preview = authoring::edit_document(directory.path(), (kind, "example"), &edit);
            assert!(preview.is_ok(), "{kind} {field}: {preview:?}");
        }
    }
}
