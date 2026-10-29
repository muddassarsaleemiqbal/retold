//! Native materials and audio projections read the shared asset and licence registries.
#![expect(clippy::unwrap_used, reason = "isolated fixture assertions")]
use std::path::Path;

use retold_app::desktop::materials;

fn manifest() -> Vec<u8> {
    let manifest = serde_json::json!({"schema":1,"id":"materials-fixture", "created":"2026-10-26T00:00:00Z", "title":"Materials", "guide":{"id":"fixture","version":"1","hash":"fixture"}, "channel":"studio","inputs":{},"variants":["en:en"],"quality":"standard","budget":0,"skip_reviews":false,"folder":["film"],"cast_levels":0});
    serde_json::to_vec(&manifest).unwrap()
}

fn save(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
}

fn production(root: &Path) {
    save(root, "studio/film/.retold/run.json", &manifest());
    save(root, "studio/film/work/scenes/frame-0.png", b"saved frame");
}

#[test]
fn a_video_lists_its_materials_with_detail_and_an_empty_audio_library_is_explicit() {
    let root = tempfile::tempdir().unwrap();
    production(root.path());
    let items = materials::list(root.path(), "studio/film").unwrap();
    assert!(!items.is_empty());
    let detail = materials::detail(root.path(), &items[0].id).unwrap();
    assert_eq!(detail.id, items[0].id);
    assert!(materials::audio(root.path()).unwrap().is_empty());
}

#[test]
fn an_imported_effect_records_its_declared_licence_and_requires_one() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("door.wav");
    std::fs::write(&source, b"RIFF fixture").unwrap();
    let mut declared = materials::AudioLicenceDeclaration {
        category: retold_app::audio::AudioCategory::Sfx,
        licence: " ".into(),
        commercial_use: true,
    };
    assert!(materials::import_audio(root.path(), &source, &declared).is_err());
    declared.licence = "CC0".into();
    let items = materials::import_audio(root.path(), &source, &declared).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].license.license_type, "CC0");
    assert!(items[0].license.commercial_use);
}
