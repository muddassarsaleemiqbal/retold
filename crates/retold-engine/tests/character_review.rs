//! Character approval must bind the displayed bytes, survive conflicts and serialize with generation.
#![expect(clippy::unwrap_used, reason = "fixture assertions")]
use std::{collections::BTreeMap, path::Path};

use retold_engine::{
    RunLayout, Stage,
    character_approval::{self, ReviewError},
    media_runtime::Asset,
};
use serde_json::json;

struct Fixture {
    temporary: tempfile::TempDir,
    layout: RunLayout,
    assets: BTreeMap<String, Asset>,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let layout = RunLayout::from_levels(
            temporary.path(),
            "fixture",
            vec!["story".parse().unwrap()],
            0,
        );
        let mut fixture = Self {
            temporary,
            layout,
            assets: BTreeMap::new(),
        };
        fixture.replace("Alice", b"original Alice image");
        fixture.replace("Bob", b"original Bob image");
        fixture
    }

    fn replace(&mut self, name: &str, bytes: &[u8]) {
        let directory = self.layout.stage(Stage::Characters);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("{name}.png"));
        std::fs::write(&path, bytes).unwrap();
        self.assets.insert(name.into(), asset(&path, bytes));
        self.save();
    }

    fn save(&self) {
        retold_engine::fsx::write_json_atomic(
            &self.layout.stage(Stage::Characters).join("review.json"),
            &self.assets,
        )
        .unwrap();
    }

    fn approvals(&self) -> std::path::PathBuf {
        self.layout.stage(Stage::Characters).join("approved.json")
    }
}

fn asset(path: &Path, bytes: &[u8]) -> Asset {
    serde_json::from_value(json!({
        "path": path, "content_hash": blake3::hash(bytes).to_hex().to_string(),
        "provider":"fixture", "model":"fixture",
        "usage":{"cost":0,"generation":null}, "quote":{"ceiling":0,"provider_tag":null},
        "request_id":"fixture"
    }))
    .unwrap()
}

#[test]
fn empty_review_does_not_create_a_run_or_lock() {
    let temporary = tempfile::tempdir().unwrap();
    let layout = RunLayout::from_levels(
        temporary.path(),
        "fixture",
        vec!["missing".parse().unwrap()],
        0,
    );
    assert!(character_approval::sheets(&layout).unwrap().is_empty());
    assert!(!layout.channel_dir().exists());
}

#[test]
fn only_reviewed_pixels_are_approved_and_other_approvals_survive_regeneration() {
    let mut fixture = Fixture::new();
    let original = character_approval::review(&fixture.layout, "Alice").unwrap();
    assert_eq!(original.bytes, b"original Alice image");
    character_approval::approve_review(&fixture.layout, "Alice", &original.sheet.content_hash)
        .unwrap();
    let bob = character_approval::review(&fixture.layout, "Bob").unwrap();
    assert!(!bob.sheet.approved);
    character_approval::approve_review(&fixture.layout, "Bob", &bob.sheet.content_hash).unwrap();
    let before = std::fs::read(fixture.approvals()).unwrap();
    fixture.replace("Alice", b"new Alice image");
    let result =
        character_approval::approve_review(&fixture.layout, "Alice", &original.sheet.content_hash);
    assert!(matches!(result, Err(ReviewError::Changed)));
    assert_eq!(std::fs::read(fixture.approvals()).unwrap(), before);
    let sheets = character_approval::sheets(&fixture.layout).unwrap();
    assert!(!sheets[0].approved);
    assert!(sheets[1].approved);
    assert!(
        character_approval::review(&fixture.layout, "Bob")
            .unwrap()
            .sheet
            .approved
    );
}

#[test]
fn altered_or_missing_image_cannot_be_approved_from_unchanged_metadata() {
    let fixture = Fixture::new();
    let original = character_approval::review(&fixture.layout, "Alice").unwrap();
    let path = &fixture.assets["Alice"].path;
    std::fs::write(path, b"altered pixels").unwrap();
    assert!(matches!(
        character_approval::review(&fixture.layout, "Alice"),
        Err(ReviewError::Changed)
    ));
    assert!(
        character_approval::approve_review(&fixture.layout, "Alice", &original.sheet.content_hash)
            .is_err()
    );
    assert!(!fixture.approvals().exists());
    std::fs::remove_file(path).unwrap();
    assert!(character_approval::review(&fixture.layout, "Alice").is_err());
}

#[test]
fn active_generation_blocks_approval_and_corrupt_approvals_are_preserved() {
    let fixture = Fixture::new();
    let original = character_approval::review(&fixture.layout, "Alice").unwrap();
    let owner = retold_engine::store::Store::open(&fixture.layout).unwrap();
    let result =
        character_approval::approve_review(&fixture.layout, "Alice", &original.sheet.content_hash);
    assert!(matches!(
        result,
        Err(ReviewError::Engine(
            retold_engine::EngineError::Locked { .. }
        ))
    ));
    assert!(!fixture.approvals().exists());
    drop(owner);
    std::fs::write(fixture.approvals(), b"broken record").unwrap();
    assert!(
        character_approval::approve_review(&fixture.layout, "Alice", &original.sheet.content_hash)
            .is_err()
    );
    assert_eq!(
        std::fs::read(fixture.approvals()).unwrap(),
        b"broken record"
    );
}

#[test]
fn sheet_metadata_cannot_read_files_outside_its_channel() {
    let mut fixture = Fixture::new();
    let external = fixture.temporary.path().join("external.png");
    std::fs::write(&external, b"outside").unwrap();
    fixture
        .assets
        .insert("Alice".into(), asset(&external, b"outside"));
    fixture.save();
    assert!(matches!(
        character_approval::review(&fixture.layout, "Alice"),
        Err(ReviewError::Invalid(_))
    ));
    assert!(!fixture.approvals().exists());
}
