//! Native administration projections are read-only until a reviewed, unchanged action is confirmed.

use retold_app::desktop::admin;

#[test]
fn health_is_read_only_and_its_shareable_form_hides_workspace_paths() {
    let root = tempfile::tempdir().unwrap();
    let before: Vec<_> = std::fs::read_dir(root.path()).unwrap().collect();
    let report = admin::health(root.path()).unwrap();
    let after: Vec<_> = std::fs::read_dir(root.path()).unwrap().collect();
    assert_eq!(before.len(), after.len());
    let shared = admin::redacted(&report, root.path());
    assert!(!shared.contains(&*root.path().to_string_lossy()));
    assert!(shared.starts_with("Retold workspace health"));
}

#[test]
fn presets_and_profiles_start_empty_and_unknown_items_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    assert!(admin::presets(root.path()).unwrap().is_empty());
    assert!(admin::remove_preset(root.path(), "missing").is_err());
    let config = root.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let (profiles, active) = admin::profiles(&config).unwrap();
    assert!(profiles.is_empty() && active.is_none());
    assert!(admin::use_profile(&config, "missing").is_err());
}

#[test]
fn presets_and_profiles_can_be_created_and_invalid_values_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let file = retold_app::presets::PresetFile {
        aspects: vec!["16:9".into()],
        container: "mp4".into(),
        video_codec: "h264".into(),
        audio_codec: "aac".into(),
        caption_mode: "burned".into(),
    };
    assert_eq!(
        admin::create_preset(root.path(), "web", &file)
            .unwrap()
            .len(),
        1
    );
    let invalid = retold_app::presets::PresetFile {
        container: "avi".into(),
        ..file
    };
    assert!(admin::create_preset(root.path(), "bad", &invalid).is_err());
    let config = root.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let (profiles, active) = admin::create_profile(&config, "client").unwrap();
    assert_eq!(profiles.len(), 1);
    assert!(active.is_none());
    assert!(admin::create_profile(&config, "client").is_err());
}

#[test]
fn a_saved_view_round_trips_the_catalog_filter() {
    let root = tempfile::tempdir().unwrap();
    let options = retold_app::library::ListOptions {
        search: Some("ferry".into()),
        sort: Some("title".into()),
        archive: true,
        ..Default::default()
    };
    let saved = retold_app::desktop::views::save(root.path(), "ferries", &options).unwrap();
    assert_eq!(saved.len(), 1);
    let restored = retold_app::desktop::views::options(&saved[0]);
    assert_eq!(restored.search.as_deref(), Some("ferry"));
    assert!(restored.archive);
    assert!(retold_app::desktop::views::save(root.path(), "bad name!", &options).is_err());
    assert!(
        retold_app::desktop::views::remove(root.path(), "ferries")
            .unwrap()
            .is_empty()
    );
}
