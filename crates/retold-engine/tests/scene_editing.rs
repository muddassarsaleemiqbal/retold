//! Manual direction changes actual production input while preserving generated originals.
#![expect(clippy::unwrap_used, reason = "isolated fixture setup and assertions")]
#[expect(
    dead_code,
    reason = "the shared fake-provider fixture supports other workflow suites"
)]
mod support {
    include!("support/mod.rs");
}

use std::sync::{Arc, atomic::Ordering};

use retold_core::Usd;
use retold_engine::editing::{
    self, EditError,
    scenes::{self, SceneSnapshot, ShotEdit},
};
use retold_engine::pipeline::{Pipeline, Until};
use retold_engine::store::{Store, hash};
use retold_engine::{RunLayout, RunMeta, Stage};
use retold_guide::Guide;
use retold_providers::Router;
use tokio_util::sync::CancellationToken;

const EDITED_ACTION: &str = "An original character closes the open window and steps back.";
const STATE_LIMIT: u64 = 2 * 1024 * 1024;

struct Fixture {
    _root: tempfile::TempDir,
    layout: RunLayout,
    meta: RunMeta,
    guide: Guide,
    router: Arc<Router>,
    text: Arc<support::TextFake>,
}

impl Fixture {
    fn new() -> Self {
        let (root, context, text, _) = support::setup(&["en"], Usd::ZERO);
        Self {
            _root: root,
            layout: context.layout.clone(),
            meta: context.meta.clone(),
            guide: context.guide.clone(),
            router: Arc::clone(&context.text),
            text,
        }
    }
    async fn generated() -> Self {
        let fixture = Self::new();
        fixture.run().await.unwrap();
        fixture
    }
    async fn run(&self) -> Result<(), retold_engine::pipeline::PipelineError> {
        let cancel = CancellationToken::new();
        Pipeline {
            layout: &self.layout,
            meta: &self.meta,
            guide: &self.guide,
            router: &self.router,
            cancel: &cancel,
            progress: &|_, _| {},
            paid: None,
            scope: "",
        }
        .run(Until::Storyboard)
        .await
    }
    fn snapshot(&self) -> SceneSnapshot {
        scenes::snapshot(&self.layout).unwrap().unwrap()
    }
    fn change(&self, original: &SceneSnapshot) -> SceneSnapshot {
        self.save_action(original, EDITED_ACTION)
    }
    fn save_action(&self, original: &SceneSnapshot, action: &str) -> SceneSnapshot {
        let shot = &original.board.direction.shots[0];
        scenes::save_shot(
            &self.layout,
            ShotEdit {
                expected_version: &original.version,
                unit: shot.unit,
                action,
                location: &shot.location,
                mood: &shot.mood,
            },
        )
        .unwrap()
    }
    fn pointer(&self) -> std::path::PathBuf {
        self.layout.meta().join("edits/scenes.json")
    }
    fn corrupt_revision(&self, id: &str) -> (std::path::PathBuf, String) {
        let path = self.revision_path(id);
        let bytes = std::fs::read_to_string(&path)
            .unwrap()
            .replace(EDITED_ACTION, "Tampered scene direction.");
        std::fs::write(&path, &bytes).unwrap();
        (path, bytes)
    }
    fn revision_path(&self, id: &str) -> std::path::PathBuf {
        self.layout
            .meta()
            .join("edits/scene-revisions")
            .join(format!("{id}.json"))
    }
    fn remove_generated_cache(&self) {
        std::fs::remove_file(self.generated_cache()).unwrap();
    }
    fn generated_cache(&self) -> std::path::PathBuf {
        self.layout
            .meta()
            .join("cache")
            .join(format!("storyboard-{}.json", self.generated_key()))
    }
    fn generated_key(&self) -> String {
        let state = retold_engine::store::read_state(&self.layout).unwrap();
        state.completed.get("storyboard").unwrap().key.clone()
    }
}

#[test]
fn absent_scene_inspection_does_not_create_editing_state_or_change_legacy_digest() {
    let fixture = Fixture::new();
    let before =
        retold_engine::fsx::read_bounded(&fixture.layout.state_file(), STATE_LIMIT).unwrap();
    let expected = hash(&Option::<Vec<u8>>::None).unwrap();
    assert_eq!(editing::digest(&fixture.layout).unwrap(), expected);
    assert!(scenes::snapshot(&fixture.layout).unwrap().is_none());
    assert!(!fixture.layout.meta().join("edits").exists());
    assert_eq!(
        retold_engine::fsx::read_bounded(&fixture.layout.state_file(), STATE_LIMIT).unwrap(),
        before
    );
}

#[tokio::test]
async fn saved_scene_changes_authoritative_direction_without_rewriting_generated_originals() {
    let fixture = Fixture::generated().await;
    let original = fixture.snapshot();
    let path = fixture
        .layout
        .stage(Stage::Storyboard)
        .join("storyboard.json");
    let generated = std::fs::read(&path).unwrap();
    let digest = editing::digest(&fixture.layout).unwrap();
    let changed = fixture.change(&original);
    assert_eq!(changed.board.direction.shots[0].action, EDITED_ACTION);
    assert_eq!(
        hash(&changed.board.units).unwrap(),
        hash(&original.board.units).unwrap()
    );
    assert_eq!(
        hash(&changed.board.plan).unwrap(),
        hash(&original.board.plan).unwrap()
    );
    let untouched = &changed.board.direction.shots[1..];
    let previous = &original.board.direction.shots[1..];
    assert_eq!(hash(&untouched).unwrap(), hash(&previous).unwrap());
    assert_eq!(std::fs::read(path).unwrap(), generated);
    assert_ne!(editing::digest(&fixture.layout).unwrap(), digest);
    assert!(!changed.approved && changed.editable && !changed.source_changed);
}

#[tokio::test]
async fn approval_is_required_before_any_provider_call_and_effective_review_uses_manual_content() {
    let fixture = Fixture::generated().await;
    let changed = fixture.change(&fixture.snapshot());
    let calls = fixture.text.calls.load(Ordering::SeqCst);
    assert!(matches!(
        editing::ready(&fixture.layout),
        Err(EditError::SceneUnapproved)
    ));
    assert!(fixture.run().await.is_err());
    assert_eq!(fixture.text.calls.load(Ordering::SeqCst), calls);
    let approved = scenes::approve(&fixture.layout, &changed.version).unwrap();
    assert!(approved.approved);
    fixture.run().await.unwrap();
    assert_eq!(fixture.text.calls.load(Ordering::SeqCst), calls);
    assert!(
        std::fs::read_to_string(fixture.layout.review_page())
            .unwrap()
            .contains(EDITED_ACTION)
    );
}

#[tokio::test]
async fn stale_edits_approvals_and_generation_locks_keep_the_saved_pointer() {
    let fixture = Fixture::generated().await;
    let original = fixture.snapshot();
    let changed = fixture.change(&original);
    let bytes = std::fs::read(fixture.pointer()).unwrap();
    assert!(matches!(
        scenes::approve(&fixture.layout, &original.version),
        Err(EditError::SceneChanged)
    ));
    let shot = &original.board.direction.shots[0];
    let stale = ShotEdit {
        expected_version: &original.version,
        unit: shot.unit,
        action: "A different action",
        location: &shot.location,
        mood: &shot.mood,
    };
    assert!(matches!(
        scenes::save_shot(&fixture.layout, stale),
        Err(EditError::SceneChanged)
    ));
    let owner = Store::open(&fixture.layout).unwrap();
    assert!(scenes::approve(&fixture.layout, &changed.version).is_err());
    drop(owner);
    assert_eq!(std::fs::read(fixture.pointer()).unwrap(), bytes);
}

#[tokio::test]
async fn undo_and_redo_restore_actual_direction_and_revoke_approval() {
    let fixture = Fixture::generated().await;
    let original = fixture.snapshot();
    let changed = fixture.change(&original);
    let approved = scenes::approve(&fixture.layout, &changed.version).unwrap();
    let undone = scenes::undo(&fixture.layout, &approved.version).unwrap();
    assert_eq!(undone.content_hash, original.content_hash);
    assert!(!undone.approved && undone.history.redo);
    let redone = scenes::redo(&fixture.layout, &undone.version).unwrap();
    assert_eq!(redone.content_hash, changed.content_hash);
    assert!(!redone.approved && redone.history.undo);
    assert!(matches!(
        editing::ready(&fixture.layout),
        Err(EditError::SceneUnapproved)
    ));
    let history = scenes::history(&fixture.layout, Option::default()).unwrap();
    assert_eq!(history.entries.len(), 1);
}

#[tokio::test]
async fn source_changes_block_edits_and_dispatch_but_still_allow_unwinding_manual_direction() {
    let fixture = Fixture::generated().await;
    let changed = fixture.change(&fixture.snapshot());
    scenes::approve(&fixture.layout, &changed.version).unwrap();
    std::fs::write(
        fixture
            .layout
            .stage(Stage::Research)
            .join("source-file.txt"),
        "Changed source text.",
    )
    .unwrap();
    let observed = fixture.snapshot();
    assert!(observed.source_changed);
    let calls = fixture.text.calls.load(Ordering::SeqCst);
    assert!(matches!(
        scenes::approve(&fixture.layout, &observed.version),
        Err(EditError::SceneSourceChanged)
    ));
    assert!(fixture.run().await.is_err());
    assert_eq!(fixture.text.calls.load(Ordering::SeqCst), calls);
    let undone = scenes::undo(&fixture.layout, &observed.version).unwrap();
    assert!(!undone.history.undo);
    assert!(scenes::ready(&fixture.layout).is_ok());
}

#[tokio::test]
async fn changed_revision_bytes_fail_closed_before_dispatch() {
    let fixture = Fixture::generated().await;
    let changed = fixture.change(&fixture.snapshot());
    let approved = scenes::approve(&fixture.layout, &changed.version).unwrap();
    let (path, bytes) = fixture.corrupt_revision(&approved.revision.unwrap());
    let calls = fixture.text.calls.load(Ordering::SeqCst);
    assert!(matches!(
        scenes::snapshot(&fixture.layout),
        Err(EditError::Integrity(_))
    ));
    assert!(fixture.run().await.is_err());
    assert_eq!(fixture.text.calls.load(Ordering::SeqCst), calls);
    assert_eq!(std::fs::read_to_string(path).unwrap(), bytes);
}

#[tokio::test]
async fn impact_review_is_read_only_and_keeps_narration_and_research_out_of_rebuild_scope() {
    let fixture = Fixture::generated().await;
    let snapshot = fixture.snapshot();
    let before = std::fs::read(fixture.pointer()).unwrap();
    let state = std::fs::read(fixture.layout.state_file()).unwrap();
    let shot = &snapshot.board.direction.shots[0];
    let impact = scenes::preview(
        &fixture.layout,
        ShotEdit {
            expected_version: &snapshot.version,
            unit: shot.unit,
            action: EDITED_ACTION,
            location: &shot.location,
            mood: &shot.mood,
        },
    )
    .unwrap();
    assert_eq!(impact.changed_units, vec![shot.unit]);
    assert!(impact.affected_stages.contains(&"images".into()));
    assert!(impact.affected_stages.contains(&"deliver".into()));
    assert!(!impact.affected_stages.iter().any(|stage| matches!(
        stage.as_str(),
        "voice" | "captions" | "script" | "research" | "characters"
    )));
    assert_eq!(std::fs::read(fixture.pointer()).unwrap(), before);
    assert_eq!(std::fs::read(fixture.layout.state_file()).unwrap(), state);
}

#[tokio::test]
async fn legacy_generated_scenes_stay_read_only_until_the_engine_establishes_a_trusted_baseline() {
    let fixture = Fixture::generated().await;
    std::fs::remove_file(fixture.pointer()).unwrap();
    let snapshot = fixture.snapshot();
    assert!(!snapshot.editable);
    let shot = &snapshot.board.direction.shots[0];
    let edit = ShotEdit {
        expected_version: &snapshot.version,
        unit: shot.unit,
        action: EDITED_ACTION,
        location: &shot.location,
        mood: &shot.mood,
    };
    assert!(scenes::preview(&fixture.layout, edit).is_err());
    assert!(scenes::save_shot(&fixture.layout, edit).is_err());
    assert!(!fixture.pointer().exists());
    fixture.run().await.unwrap();
    assert!(fixture.snapshot().editable);
}

#[tokio::test]
async fn missing_generated_envelope_blocks_an_approved_manual_head_without_new_provider_calls() {
    let fixture = Fixture::generated().await;
    let changed = fixture.change(&fixture.snapshot());
    scenes::approve(&fixture.layout, &changed.version).unwrap();
    fixture.remove_generated_cache();
    let calls = fixture.text.calls.load(Ordering::SeqCst);
    assert!(matches!(
        scenes::ready(&fixture.layout),
        Err(EditError::Integrity(_))
    ));
    assert!(fixture.run().await.is_err());
    assert_eq!(fixture.text.calls.load(Ordering::SeqCst), calls);
}

#[tokio::test]
async fn scene_history_is_bounded_and_cursors_are_invalidated_by_a_new_head() {
    let fixture = Fixture::generated().await;
    let mut current = fixture.snapshot();
    for index in 0..=editing::HISTORY_PAGE_SIZE {
        current = fixture.save_action(&current, &format!("{EDITED_ACTION} Revision {index}."));
    }
    let before = std::fs::read(fixture.pointer()).unwrap();
    let first = scenes::history(&fixture.layout, Option::default()).unwrap();
    assert_eq!(first.entries.len(), editing::HISTORY_PAGE_SIZE);
    let cursor = first.next.unwrap();
    let last = scenes::history(&fixture.layout, Some(&cursor)).unwrap();
    assert_eq!(last.entries.len(), 1);
    assert_eq!(std::fs::read(fixture.pointer()).unwrap(), before);
    fixture.save_action(&current, "An original figure waits quietly.");
    assert!(matches!(
        scenes::history(&fixture.layout, Some(&cursor)),
        Err(EditError::SceneChanged)
    ));
}

#[test]
fn read_only_revision_inspection_keeps_exact_scope_boundaries_and_saturates() {
    let mut state = retold_engine::store::State {
        revisions: std::collections::BTreeMap::from([("image-1".into(), 2), ("images".into(), 1)]),
        ..Default::default()
    };
    assert_eq!(state.revision("image-1-0"), 2 + 1);
    assert_eq!(state.revision("image-10-0"), 1);
    state.revisions.insert("images".into(), u64::MAX);
    assert_eq!(state.revision("image-1-0"), u64::MAX);
}

#[tokio::test]
async fn image_redo_changes_edit_versions_without_revoking_approved_direction() {
    let fixture = Fixture::generated().await;
    let changed = fixture.change(&fixture.snapshot());
    let original = scenes::approve(&fixture.layout, &changed.version).unwrap();
    let pointer = std::fs::read(fixture.pointer()).unwrap();
    let mut writer = Store::open(&fixture.layout).unwrap();
    writer.bump_revision("image-1").unwrap();
    drop(writer);
    let latest = fixture.snapshot();
    assert_ne!(latest.version, original.version);
    assert_eq!(latest.content_hash, original.content_hash);
    assert!(latest.approved && !latest.source_changed);
    scenes::ready(&fixture.layout).unwrap();
    let shot = &original.board.direction.shots[0];
    let edit = ShotEdit {
        expected_version: &original.version,
        unit: shot.unit,
        action: "An original figure waits by the door.",
        location: &shot.location,
        mood: &shot.mood,
    };
    assert!(matches!(
        scenes::save_shot(&fixture.layout, edit),
        Err(EditError::SceneChanged)
    ));
    assert_eq!(std::fs::read(fixture.pointer()).unwrap(), pointer);
}

fn rejected_casts() -> Vec<Vec<retold_engine::story::CastPose>> {
    let pose = |name: &str| retold_engine::story::CastPose {
        name: name.into(),
        expression: "calm".into(),
    };
    vec![
        vec![pose("Nobody From This Story")],
        vec![pose("Ada"), pose("Ada")],
        (0..=scenes::MAX_CAST)
            .map(|index| pose(&format!("Extra {index}")))
            .collect(),
        vec![pose(" ")],
    ]
}

const fn cast_edit<'a>(
    version: &'a str,
    cast: &'a [retold_engine::story::CastPose],
) -> scenes::CastEdit<'a> {
    scenes::CastEdit {
        expected_version: version,
        unit: 0,
        cast,
    }
}

#[tokio::test]
async fn cast_edits_accept_only_saved_profiles_and_leave_the_journal_untouched_when_rejected() {
    let fixture = Fixture::generated().await;
    let snapshot = fixture.snapshot();
    let before = std::fs::read(fixture.pointer()).unwrap();
    for cast in &rejected_casts() {
        let edit = cast_edit(&snapshot.version, cast);
        let preview = scenes::preview_cast(&fixture.layout, edit);
        assert!(matches!(preview, Err(EditError::Invalid(_))));
        let saved = scenes::save_cast(&fixture.layout, edit);
        assert!(matches!(saved, Err(EditError::Invalid(_))));
    }
    let unchanged = scenes::save_cast(&fixture.layout, cast_edit(&snapshot.version, &[])).unwrap();
    assert_eq!(unchanged.version, snapshot.version);
    let stale = scenes::save_cast(&fixture.layout, cast_edit("stale", &[]));
    assert!(matches!(stale, Err(EditError::SceneChanged)));
    assert_eq!(std::fs::read(fixture.pointer()).unwrap(), before);
}
