//! Manual revisions change effective content, preserve immutable history and cannot grant execution.
#![expect(clippy::unwrap_used, reason = "isolated fixture setup and assertions")]
#[expect(
    dead_code,
    reason = "each suite uses only part of the shared fake providers"
)]
mod support {
    include!("support/mod.rs");
}

use std::sync::{Arc, atomic::Ordering};

use retold_core::Usd;
use retold_engine::editing::{self, EditError, SectionEdit};
use retold_engine::pipeline::{Pipeline, Until};
use retold_engine::store::{Store, hash};
use retold_engine::{RunLayout, RunMeta, Stage, fsx, story::Script};
use retold_guide::Guide;
use retold_providers::Router;
use tokio_util::sync::CancellationToken;

struct Fixture {
    _sandbox: tempfile::TempDir,
    layout: RunLayout,
    meta: RunMeta,
    guide: Guide,
    router: Arc<Router>,
    text: Arc<support::TextFake>,
}

impl Fixture {
    fn new() -> Self {
        let (sandbox, context, text, _) = support::setup(&["en"], Usd::ZERO);
        Self {
            _sandbox: sandbox,
            layout: context.layout.clone(),
            meta: context.meta.clone(),
            guide: context.guide.clone(),
            router: Arc::clone(&context.text),
            text,
        }
    }

    fn seeded() -> Self {
        let fixture = Self::new();
        let script: Script =
            serde_json::from_str(include_str!("fixtures/full/script.json")).unwrap();
        let mut store = Store::open(&fixture.layout).unwrap();
        store
            .commit("script", &hash(&script).unwrap(), &script, None)
            .unwrap();
        fsx::write_json_atomic(
            &fixture.layout.stage(Stage::Script).join("script.json"),
            &script,
        )
        .unwrap();
        drop(store);
        fixture
    }

    fn snapshot(&self) -> editing::ScriptSnapshot {
        editing::script(&self.layout).unwrap().unwrap()
    }

    fn change_hook(&self) -> editing::ScriptSnapshot {
        let snapshot = self.snapshot();
        let section = &snapshot.script.sections[0];
        let text = section.text.replace("vampire", "stranger");
        editing::save_section(
            &self.layout,
            SectionEdit {
                expected_version: &snapshot.version,
                section: &section.section,
                text: &text,
            },
        )
        .unwrap()
    }

    fn corrupt_revision(&self, id: &str) -> (std::path::PathBuf, String) {
        let path = revision_path(&self.layout, id);
        let bytes = std::fs::read_to_string(&path)
            .unwrap()
            .replace("stranger", "intruder");
        std::fs::write(&path, &bytes).unwrap();
        (path, bytes)
    }

    async fn run_script(&self) -> Result<(), retold_engine::pipeline::PipelineError> {
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
        .run(Until::Script)
        .await
    }
}

fn revision_path(layout: &RunLayout, id: &str) -> std::path::PathBuf {
    layout
        .meta()
        .join("edits/revisions")
        .join(format!("{id}.json"))
}

#[test]
fn reading_a_script_creates_no_editing_state() {
    let fixture = Fixture::seeded();
    let before = std::fs::read(fixture.layout.state_file()).unwrap();
    let script = fixture.snapshot();
    assert!(script.editable);
    assert!(script.revision.is_none());
    assert!(!fixture.layout.meta().join("edits").exists());
    assert_eq!(std::fs::read(fixture.layout.state_file()).unwrap(), before);
}

#[test]
fn a_revision_persists_real_content_without_rewriting_the_generated_mirror() {
    let fixture = Fixture::seeded();
    let original = fixture.snapshot();
    let generated = fixture.layout.stage(Stage::Script).join("script.json");
    let bytes = std::fs::read(&generated).unwrap();
    let changed = fixture.change_hook();
    assert!(changed.script.sections[0].text.contains("stranger"));
    assert_eq!(
        changed.script.sections[1].text,
        original.script.sections[1].text
    );
    assert!(!changed.approved);
    assert!(changed.revision.is_some());
    assert_eq!(std::fs::read(generated).unwrap(), bytes);
    assert_eq!(fixture.snapshot().content_hash, changed.content_hash);
    assert!(matches!(
        editing::ready(&fixture.layout),
        Err(EditError::Unapproved)
    ));
}

#[test]
fn stale_editor_and_approval_requests_preserve_the_saved_revision() {
    let fixture = Fixture::seeded();
    let old = fixture.snapshot();
    let saved = fixture.change_hook();
    let retry = SectionEdit {
        expected_version: &old.version,
        section: "hook",
        text: "An older editor's text",
    };
    assert!(matches!(
        editing::save_section(&fixture.layout, retry),
        Err(EditError::Changed)
    ));
    assert!(matches!(
        editing::approve(&fixture.layout, &old.version),
        Err(EditError::Changed)
    ));
    assert_eq!(fixture.snapshot().version, saved.version);
}

#[test]
fn approval_checks_quality_and_exact_content() {
    let fixture = Fixture::seeded();
    let original = fixture.snapshot();
    let draft = editing::save_section(
        &fixture.layout,
        SectionEdit {
            expected_version: &original.version,
            section: "hook",
            text: "",
        },
    )
    .unwrap();
    assert_ne!(draft.problems, Vec::<String>::new());
    assert!(editing::approve(&fixture.layout, &draft.version).is_err());
    assert!(!fixture.snapshot().approved);
    let undo = editing::undo(&fixture.layout, &draft.version).unwrap();
    let approved = editing::approve(&fixture.layout, &undo.version).unwrap();
    assert!(approved.approved);
    assert!(approved.approved_at.is_some());
    assert!(editing::ready(&fixture.layout).is_ok());
}

#[test]
fn unicode_edits_survive_reload_and_undo_redo_requires_fresh_approval() {
    let fixture = Fixture::seeded();
    let original = fixture.snapshot();
    let text = "English draft — हिन्दी १२३ — اردو 123";
    let saved = editing::save_section(
        &fixture.layout,
        SectionEdit {
            expected_version: &original.version,
            section: "hook",
            text,
        },
    )
    .unwrap();
    assert_eq!(fixture.snapshot().script.sections[0].text, text);
    let undone = editing::undo(&fixture.layout, &saved.version).unwrap();
    assert_eq!(undone.content_hash, original.content_hash);
    assert!(undone.history.redo);
    let redone = editing::redo(&fixture.layout, &undone.version).unwrap();
    assert_eq!(redone.script.sections[0].text, text);
    assert!(!redone.approved);
}

#[test]
fn generation_lock_prevents_cross_client_editing() {
    let fixture = Fixture::seeded();
    let snapshot = fixture.snapshot();
    let owner = Store::open(&fixture.layout).unwrap();
    let request = SectionEdit {
        expected_version: &snapshot.version,
        section: "hook",
        text: "conflicting writer",
    };
    assert!(editing::save_section(&fixture.layout, request).is_err());
    drop(owner);
    assert_eq!(fixture.snapshot().version, snapshot.version);
}

#[test]
fn history_is_bounded_and_immutable_across_new_edits() {
    let fixture = Fixture::seeded();
    let first = fixture.change_hook();
    let id = first.revision.as_ref().unwrap();
    let path = fixture
        .layout
        .meta()
        .join("edits/revisions")
        .join(format!("{id}.json"));
    let original = std::fs::read(&path).unwrap();
    for index in 1..=editing::HISTORY_PAGE_SIZE {
        let snapshot = fixture.snapshot();
        editing::save_section(
            &fixture.layout,
            SectionEdit {
                expected_version: &snapshot.version,
                section: "hook",
                text: &format!("Saved draft {index}"),
            },
        )
        .unwrap();
    }
    let page = editing::history(&fixture.layout, Option::default()).unwrap();
    assert_eq!(page.entries.len(), editing::HISTORY_PAGE_SIZE);
    let next = editing::history(&fixture.layout, page.next.as_ref()).unwrap();
    assert_eq!(next.entries.len(), 1);
    assert!(next.next.is_none());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[tokio::test]
async fn approved_edits_are_consumed_by_the_real_pipeline_without_new_model_requests() {
    let fixture = Fixture::new();
    fixture.run_script().await.unwrap();
    let requests = fixture.text.calls.load(Ordering::SeqCst);
    let edited = fixture.change_hook();
    editing::approve(&fixture.layout, &edited.version).unwrap();
    fixture.run_script().await.unwrap();
    let bytes = std::fs::read(fixture.layout.stage(Stage::Script).join("script.json")).unwrap();
    let effective: Script = serde_json::from_slice(&bytes).unwrap();
    assert!(effective.sections[0].text.contains("stranger"));
    assert_eq!(fixture.text.calls.load(Ordering::SeqCst), requests);
}

#[tokio::test]
async fn changed_sources_block_old_edits_before_any_model_request() {
    let fixture = Fixture::new();
    fixture.run_script().await.unwrap();
    let edited = fixture.change_hook();
    editing::approve(&fixture.layout, &edited.version).unwrap();
    let requests = fixture.text.calls.load(Ordering::SeqCst);
    fsx::write_atomic(
        &fixture
            .layout
            .stage(Stage::Research)
            .join("source-file.txt"),
        b"Changed source facts.",
    )
    .unwrap();
    assert!(matches!(
        editing::ready(&fixture.layout),
        Err(EditError::SourceChanged)
    ));
    assert!(fixture.run_script().await.is_err());
    assert_eq!(fixture.text.calls.load(Ordering::SeqCst), requests);
    assert!(fixture.snapshot().source_changed);
}

#[test]
fn damaged_revision_is_reported_without_repairing_its_bytes() {
    let fixture = Fixture::seeded();
    let saved = fixture.change_hook();
    let (path, bytes) = fixture.corrupt_revision(saved.revision.as_deref().unwrap());
    assert!(matches!(
        editing::script(&fixture.layout),
        Err(EditError::Integrity(_))
    ));
    assert!(editing::ready(&fixture.layout).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), bytes);
}

#[test]
fn changing_the_head_invalidates_a_history_cursor_even_after_undo_to_baseline() {
    let fixture = Fixture::seeded();
    let saved = fixture.change_hook();
    let cursor = editing::HistoryCursor {
        head: saved.revision.clone().unwrap(),
        next: saved.revision.clone().unwrap(),
    };
    editing::undo(&fixture.layout, &saved.version).unwrap();
    assert!(matches!(
        editing::history(&fixture.layout, Some(&cursor)),
        Err(EditError::Changed)
    ));
}

#[test]
fn script_edits_invalidate_descendants_and_preserve_research_and_writer_cache() {
    let fixture = Fixture::seeded();
    {
        let mut store = Store::open(&fixture.layout).unwrap();
        for stage in ["source", "draft-0", "storyboard", "voice-en"] {
            store
                .commit(stage, &hash(&stage).unwrap(), &stage, Option::default())
                .unwrap();
        }
    }
    fixture.change_hook();
    let state = retold_engine::store::read_state(&fixture.layout).unwrap();
    assert!(state.completed.contains_key("source"));
    assert!(state.completed.contains_key("draft-0"));
    assert!(state.completed.contains_key("script"));
    assert!(!state.completed.contains_key("storyboard"));
    assert!(!state.completed.contains_key("voice-en"));
}

#[tokio::test]
async fn undoing_all_edits_then_running_restores_the_original_baseline_without_new_requests() {
    let fixture = Fixture::new();
    fixture.run_script().await.unwrap();
    let original = fixture.snapshot();
    let saved = fixture.change_hook();
    let approved = editing::approve(&fixture.layout, &saved.version).unwrap();
    fixture.run_script().await.unwrap();
    let requests = fixture.text.calls.load(Ordering::SeqCst);
    let current = fixture.snapshot();
    assert_eq!(current.content_hash, approved.content_hash);
    editing::undo(&fixture.layout, &current.version).unwrap();
    fixture.run_script().await.unwrap();
    assert_eq!(fixture.snapshot().content_hash, original.content_hash);
    assert_eq!(fixture.text.calls.load(Ordering::SeqCst), requests);
}
