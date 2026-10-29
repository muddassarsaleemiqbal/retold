use retold_app::desktop::recovery::{Content, Draft, Repository};
const IDENTIFIER_LENGTH: usize = 64;
const SCRIPT_LIMIT: usize = 128 * 1024;
const RETENTION_CAPACITY: usize = 32;
const FILE_CEILING: u64 = 8 * 1024 * 1024;

fn draft() -> Draft {
    Draft {
        library: std::env::temp_dir().join("fictional-library"),
        reference: "channel/story".into(),
        video_id: "video-one".into(),
        title: "Fictional story".into(),
        content: Content::Script {
            section: "Opening".into(),
            source_version: "a".repeat(IDENTIFIER_LENGTH),
            text: "हिंदी اردو English".into(),
        },
    }
}

#[test]
fn absent_read_is_side_effect_free() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("absent");
    assert_eq!(Repository::new(absent.clone()).list().unwrap(), []);
    assert!(!absent.exists());
}

#[test]
fn restart_retains_exact_text_and_retry_receipt() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let id = "b".repeat(IDENTIFIER_LENGTH);
    let saved = repository.put(&id, None, draft()).unwrap();
    let reopened = Repository::new(root.path().into());
    assert_eq!(reopened.list().unwrap(), vec![saved.clone()]);
    assert_eq!(reopened.put(&id, None, draft()).unwrap(), saved);
    assert_eq!(saved.draft.content.text(), "हिंदी اردو English");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let permissions = std::fs::metadata(root.path().join("desktop-drafts.json"))
            .unwrap()
            .permissions();
        assert_eq!(permissions.mode() & 0o077, 0, "Draft text is private");
    }
}

#[test]
fn another_window_and_stale_cleanup_cannot_overwrite_newer_text() {
    let root = tempfile::tempdir().unwrap();
    let first = Repository::new(root.path().into());
    let second = first.clone();
    let id = "c".repeat(IDENTIFIER_LENGTH);
    let saved = first.put(&id, None, draft()).unwrap();
    let mut changed = draft();
    if let Content::Script { text, .. } = &mut changed.content {
        *text = "Newer text".into();
    }
    let next = second.put(&id, Some(&saved.version), changed).unwrap();
    assert!(first.put(&id, Some(&saved.version), draft()).is_err());
    assert!(first.remove(&id, &saved.version).is_err());
    assert_eq!(first.list().unwrap(), vec![next.clone()]);
    first.remove(&id, &next.version).unwrap();
    assert!(second.put(&id, Some(&next.version), draft()).is_err());
    second.remove(&id, &next.version).unwrap();
    assert_eq!(first.list().unwrap(), []);
}

#[test]
fn changed_target_is_rejected_and_corruption_is_preserved() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let id = "d".repeat(IDENTIFIER_LENGTH);
    let saved = repository.put(&id, None, draft()).unwrap();
    let mut changed = draft();
    changed.video_id = "other-video".into();
    assert!(repository.put(&id, Some(&saved.version), changed).is_err());
    let path = root.path().join("desktop-drafts.json");
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(repository.list().is_err());
    assert!(repository.put(&id, Some(&saved.version), draft()).is_err());
    assert!(repository.remove(&id, &saved.version).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"corrupt");
}

#[test]
fn oversized_text_and_embedded_execution_authority_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let mut oversized = draft();
    if let Content::Script { text, .. } = &mut oversized.content {
        *text = "x".repeat(SCRIPT_LIMIT + 1);
    }
    assert!(
        repository
            .put(&"e".repeat(IDENTIFIER_LENGTH), None, oversized)
            .is_err()
    );
    assert!(!root.path().join("desktop-drafts.json").exists());
    let mut payload = serde_json::to_value(draft()).unwrap();
    payload["run_allowance"] = serde_json::json!({"usd_micros": 100});
    assert!(serde_json::from_value::<Draft>(payload).is_err());
}

#[test]
fn concurrent_writer_keeps_prior_checkpoint_bytes() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let id = "f".repeat(IDENTIFIER_LENGTH);
    let saved = repository.put(&id, None, draft()).unwrap();
    let path = root.path().join("desktop-drafts.json");
    let before = std::fs::read(&path).unwrap();
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .open(root.path().join(".desktop-drafts.lock"))
        .unwrap();
    lock.lock().unwrap();
    assert!(repository.remove(&id, &saved.version).is_err());
    assert_eq!(repository.list().unwrap(), vec![saved]);
    lock.unlock().unwrap();
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn inactive_configuration_keeps_prior_checkpoint_bytes() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let id = "f".repeat(IDENTIFIER_LENGTH);
    let saved = repository.put(&id, None, draft()).unwrap();
    let path = root.path().join("desktop-drafts.json");
    let before = std::fs::read(&path).unwrap();
    std::fs::write(root.path().join(".retold-restore.json"), b"{}").unwrap();
    assert!(repository.remove(&id, &saved.version).is_err());
    assert!(repository.put(&id, Some(&saved.version), draft()).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn checksum_and_duplicate_records_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let saved = repository
        .put(&"a".repeat(IDENTIFIER_LENGTH), None, draft())
        .unwrap();
    let path = root.path().join("desktop-drafts.json");
    let mut tampered = saved.clone();
    tampered.draft.title = "Changed without a checksum".into();
    let payload = serde_json::json!({"schema":1,"entries":[tampered]});
    std::fs::write(&path, serde_json::to_vec(&payload).unwrap()).unwrap();
    assert!(repository.list().is_err());
    let duplicated = serde_json::json!({"schema":1,"entries":[saved.clone(),saved]});
    std::fs::write(&path, serde_json::to_vec(&duplicated).unwrap()).unwrap();
    assert!(repository.list().is_err());
}

#[test]
fn replacement_production_cannot_receive_an_older_draft() {
    use retold_app::desktop::recovery::{capture, validate_target};
    let root = tempfile::tempdir().unwrap();
    let video = root.path().join("channel/story/.retold");
    std::fs::create_dir_all(&video).unwrap();
    let mut manifest = serde_json::json!({"schema":1,"id":"original", "created":"2026-10-26T00:00:00Z", "title":"Recovery", "guide":{"id":"fixture","version":"1","hash":"fixture"}, "channel":"channel","inputs":{},"variants":["en:en"],"quality":"standard","budget":0,"skip_reviews":false,"folder":["story"],"cast_levels":0});
    let path = video.join("run.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let captured = capture(root.path(), "channel/story", draft().content).unwrap();
    validate_target(&captured).unwrap();
    manifest["id"] = serde_json::json!("replacement");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(validate_target(&captured).is_err());
    assert!(!root.path().join(".retold/studio").exists());
}

#[test]
fn reaching_capacity_preserves_every_existing_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    for index in 0..RETENTION_CAPACITY {
        repository
            .put(&format!("{index:064x}"), None, draft())
            .unwrap();
    }
    let path = root.path().join("desktop-drafts.json");
    let before = std::fs::read(&path).unwrap();
    assert!(
        repository
            .put(&"f".repeat(IDENTIFIER_LENGTH), None, draft())
            .is_err()
    );
    assert_eq!(repository.list().unwrap().len(), RETENTION_CAPACITY);
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn oversized_file_is_rejected_without_replacement_or_unbounded_read() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("desktop-drafts.json");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(FILE_CEILING + 1).unwrap();
    let repository = Repository::new(root.path().into());
    assert!(repository.list().is_err());
    assert!(
        repository
            .put(&"f".repeat(IDENTIFIER_LENGTH), None, draft())
            .is_err()
    );
    assert_eq!(std::fs::metadata(path).unwrap().len(), FILE_CEILING + 1);
}

#[test]
fn scene_recovery_retains_every_visual_field_and_excludes_review_authority() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let mut scene = draft();
    scene.content = Content::Scene {
        draft: retold_app::scenes::Draft {
            expected_version: "a".repeat(IDENTIFIER_LENGTH),
            unit: 1,
            action: "हिंदी اردو English action".into(),
            location: "A courtyard".into(),
            mood: "watchful".into(),
        },
        cast: Some(vec![retold_app::scenes::CastPose {
            name: "Ada".into(),
            expression: "calm".into(),
        }]),
    };
    let saved = repository
        .put(&"b".repeat(IDENTIFIER_LENGTH), None, scene)
        .unwrap();
    assert_eq!(repository.list().unwrap(), vec![saved.clone()]);
    let text = saved.draft.content.copy_text();
    assert!(text.contains("A courtyard") && text.contains("watchful"));
    assert!(
        matches!(&saved.draft.content, Content::Scene { cast: Some(cast), .. } if cast.len() == 1)
    );
    let mut payload = serde_json::to_value(saved.draft).unwrap();
    payload["content"]["draft"]["approved"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Draft>(payload).is_err());
}

#[test]
fn incomplete_scene_text_can_be_retained_without_becoming_publishable() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let mut scene = draft();
    let pending = retold_app::scenes::Draft {
        expected_version: "a".repeat(IDENTIFIER_LENGTH),
        unit: 1,
        action: String::new(),
        location: "An unfinished location".into(),
        mood: String::new(),
    };
    assert!(pending.validate_text().is_err());
    scene.content = Content::Scene {
        draft: pending,
        cast: Option::default(),
    };
    let saved = repository
        .put(&"b".repeat(IDENTIFIER_LENGTH), None, scene)
        .unwrap();
    assert_eq!(repository.list().unwrap(), vec![saved]);
}

#[test]
fn contextual_notes_survive_restart_and_cannot_rebind_an_existing_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let mut draft = draft();
    let target = retold_app::notes::Target::frame(0, b"reviewed pixels").unwrap();
    draft.content = Content::Note {
        target: Some(target),
        append_id: "c".repeat(IDENTIFIER_LENGTH),
        source_version: "d".repeat(IDENTIFIER_LENGTH),
        text: "Keep the original context हिंदी اردو".into(),
    };
    let id = "e".repeat(IDENTIFIER_LENGTH);
    let saved = repository.put(&id, None, draft.clone()).unwrap();
    assert_eq!(
        Repository::new(root.path().into()).list().unwrap(),
        vec![saved.clone()]
    );
    if let Content::Note { target, .. } = &mut draft.content {
        *target = Some(retold_app::notes::Target::frame(0, b"replacement pixels").unwrap());
    }
    assert!(repository.put(&id, Some(&saved.version), draft).is_err());
    assert_eq!(repository.list().unwrap(), vec![saved]);
}

#[test]
fn legacy_note_payload_round_trips_without_adding_checksum_fields() {
    let old = serde_json::json!({"kind":"note","append_id":"a".repeat(IDENTIFIER_LENGTH),"source_version":"b".repeat(IDENTIFIER_LENGTH),"text":"legacy text"});
    let content: Content = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(serde_json::to_value(content).unwrap(), old);
}

#[test]
fn edition_line_recovery_keeps_language_unit_and_text_and_cannot_change_target() {
    let root = tempfile::tempdir().unwrap();
    let repository = Repository::new(root.path().into());
    let line = |unit: usize| retold_app::dialogue::Draft {
        expected_version: "a".repeat(IDENTIFIER_LENGTH),
        language: "hi".into(),
        unit,
        text: "एक नया वाक्य".into(),
    };
    let mut retained = draft();
    retained.content = Content::Line { draft: line(0) };
    let id = "c".repeat(IDENTIFIER_LENGTH);
    let saved = repository.put(&id, None, retained.clone()).unwrap();
    assert_eq!(saved.draft.content.text(), "एक नया वाक्य");
    retained.content = Content::Line { draft: line(1) };
    assert!(repository.put(&id, Some(&saved.version), retained).is_err());
}
