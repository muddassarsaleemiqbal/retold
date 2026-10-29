//! Comments retain legacy evidence, reject stale appends and acknowledge retries exactly once.
#![expect(clippy::unwrap_used, reason = "isolated fixture construction")]
use std::path::PathBuf;

use retold_app::notes::{self, Append, Document, Query};
use retold_app::protocol::{ErrorCode, Payload};

const REFERENCE: &str = "studio/film";
const OVERSIZED_TEXT: usize = 8193;

struct Fixture {
    root: tempfile::TempDir,
    video: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let video = root.path().join(REFERENCE);
        std::fs::create_dir_all(video.join(".retold")).unwrap();
        save_manifest(&video);
        Self { root, video }
    }
    fn document(&self) -> Document {
        notes::document(self.root.path(), REFERENCE, &Query::default()).unwrap()
    }
    fn journal(&self) -> PathBuf {
        self.video.join(".retold/review-notes.json")
    }
    fn append(&self, document: &Document, id: &str) -> notes::Saved {
        notes::append(self.root.path(), REFERENCE, request(document, id)).unwrap()
    }
}

fn save_manifest(video: &std::path::Path) {
    let manifest = serde_json::json!({"schema":1,"id":"notes-fixture", "created":"2026-10-26T00:00:00Z", "title":"Review notes", "guide":{"id":"fixture","version":"1","hash":"fixture"}, "channel":"studio","inputs":{},"variants":["en:en"],"quality":"standard","budget":0,"skip_reviews":false,"folder":["film"],"cast_levels":0});
    std::fs::write(
        video.join(".retold/run.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}

fn request<'a>(document: &'a Document, id: &'a str) -> Append<'a> {
    Append {
        expected_version: &document.version,
        id,
        item: "video",
        text: "हिंदी اردو English\nKeep the narration clear.",
    }
}

#[test]
fn reading_comments_creates_no_journal_lock_or_queue() {
    let fixture = Fixture::new();
    assert_eq!(fixture.document().total, 0);
    assert!(!fixture.journal().exists());
    assert_eq!(
        std::fs::read_dir(fixture.video.join(".retold"))
            .unwrap()
            .count(),
        1
    );
    assert!(!fixture.root.path().join(".retold/studio").exists());
}

#[test]
fn an_empty_version_from_another_workspace_cannot_admit_a_comment() {
    let original = Fixture::new();
    let other = Fixture::new();
    let document = original.document();
    let id = notes::draft_id().unwrap();
    let rejected =
        notes::append(other.root.path(), REFERENCE, request(&document, &id)).unwrap_err();
    assert_eq!(rejected.code, ErrorCode::PlanChanged);
    assert!(!other.journal().exists());
}

#[test]
fn retry_is_idempotent_and_stale_or_conflicting_requests_preserve_bytes() {
    let fixture = Fixture::new();
    let original = fixture.document();
    let id = notes::draft_id().unwrap();
    let saved = fixture.append(&original, &id);
    let bytes = std::fs::read(fixture.journal()).unwrap();
    let retried = fixture.append(&original, &id);
    assert_eq!(retried, saved);
    conflicting_requests_are_rejected(&fixture, &original, &id);
    assert_eq!(std::fs::read(fixture.journal()).unwrap(), bytes);
    let next = notes::draft_id().unwrap();
    assert_eq!(fixture.append(&saved.document, &next).document.total, 2);
    assert!(!fixture.video.join(".retold/state.json").exists());
}

fn conflicting_requests_are_rejected(fixture: &Fixture, original: &Document, id: &str) {
    let mut conflict = request(original, id);
    conflict.text = "Different text for the same identifier";
    assert!(notes::append(fixture.root.path(), REFERENCE, conflict).is_err());
    let next = notes::draft_id().unwrap();
    let stale =
        notes::append(fixture.root.path(), REFERENCE, request(original, &next)).unwrap_err();
    assert_eq!(stale.code, ErrorCode::PlanChanged);
}

#[test]
fn cli_history_and_summary_include_new_comments_without_overwriting_legacy_evidence() {
    let fixture = Fixture::new();
    let legacy = fixture.video.join("meta/reviews.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    let bytes = br#"{"approvals":{},"history":[],"notes":{"video":[{"id":"note-1","item":"video","text":"Legacy comment","created_at":"2026-10-20T00:00:00Z"}]}}"#;
    std::fs::write(&legacy, bytes).unwrap();
    assert_eq!(fixture.document().total, 1);
    fixture.append(&fixture.document(), &notes::draft_id().unwrap());
    assert_eq!(std::fs::read(legacy).unwrap(), bytes);
    assert_history_count(&fixture);
    let summary = retold_app::library::query(
        fixture.root.path(),
        retold_app::library::Query::Review(REFERENCE, retold_app::review::ReviewFilter::default()),
    );
    let Payload::Result { contents, .. } = summary else {
        panic!("Review failed: {summary:?}")
    };
    assert_eq!(contents["notes_count"], 2);
}

fn assert_history_count(fixture: &Fixture) {
    let history = retold_app::library::query(
        fixture.root.path(),
        retold_app::library::Query::History(REFERENCE, Some("review")),
    );
    assert!(
        matches!(&history, Payload::Result { .. }),
        "History failed: {history:?}"
    );
    if let Payload::Result { contents, .. } = history {
        assert_eq!(contents["total"], 2);
    }
}

#[test]
fn malformed_journals_keep_their_original_bytes() {
    let fixture = Fixture::new();
    let original = fixture.document();
    std::fs::write(fixture.journal(), b"broken journal").unwrap();
    assert!(notes::document(fixture.root.path(), REFERENCE, &Query::default()).is_err());
    assert!(
        notes::append(
            fixture.root.path(),
            REFERENCE,
            request(&original, &notes::draft_id().unwrap())
        )
        .is_err()
    );
    assert_eq!(std::fs::read(fixture.journal()).unwrap(), b"broken journal");
}

#[test]
fn subsecond_comment_order_uses_timestamp_values() {
    let fixture = Fixture::new();
    let first = fixture.append(&fixture.document(), &notes::draft_id().unwrap());
    let second = fixture.append(&first.document, &notes::draft_id().unwrap());
    let mut journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.journal()).unwrap()).unwrap();
    journal["entries"][0]["created_at"] = "2026-10-26T10:00:00Z".into();
    journal["entries"][1]["created_at"] = "2026-10-26T10:00:00.1Z".into();
    std::fs::write(fixture.journal(), serde_json::to_vec(&journal).unwrap()).unwrap();
    assert_eq!(fixture.document().notes[0].id, second.note.id);
}

#[test]
fn duplicate_stored_identifiers_and_oversized_inputs_are_rejected() {
    let fixture = Fixture::new();
    let document = fixture.document();
    let id = notes::draft_id().unwrap();
    let mut oversized = request(&document, &id);
    let text = "x".repeat(OVERSIZED_TEXT);
    oversized.text = &text;
    assert!(notes::append(fixture.root.path(), REFERENCE, oversized).is_err());
    assert!(!fixture.journal().exists());
    fixture.append(&document, &id);
    duplicate_record_is_rejected(&fixture);
}

fn duplicate_record_is_rejected(fixture: &Fixture) {
    let mut journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.journal()).unwrap()).unwrap();
    let record = journal["entries"][0].clone();
    journal["entries"].as_array_mut().unwrap().push(record);
    let original = serde_json::to_vec(&journal).unwrap();
    std::fs::write(fixture.journal(), &original).unwrap();
    assert!(notes::document(fixture.root.path(), REFERENCE, &Query::default()).is_err());
    assert_eq!(std::fs::read(fixture.journal()).unwrap(), original);
}

#[test]
fn generation_and_inactive_roots_block_new_comments_but_keep_saved_receipts_readable() {
    let fixture = Fixture::new();
    let original = fixture.document();
    let id = notes::draft_id().unwrap();
    let saved = fixture.append(&original, &id);
    let layout =
        retold_engine::RunLayout::from_video_dir(fixture.root.path(), "studio", &fixture.video, 2)
            .unwrap();
    let owner = retold_engine::store::Store::open(&layout).unwrap();
    assert_eq!(fixture.append(&original, &id).note.id, saved.note.id);
    assert!(
        notes::append(
            fixture.root.path(),
            REFERENCE,
            request(&saved.document, &notes::draft_id().unwrap())
        )
        .is_err()
    );
    drop(owner);
    check_inactive_root(&fixture, &saved.document);
}

fn check_inactive_root(fixture: &Fixture, saved: &Document) {
    std::fs::write(
        fixture
            .root
            .path()
            .join(retold_engine::recovery::INACTIVE_MARKER),
        b"{}",
    )
    .unwrap();
    assert_eq!(fixture.document().total, 1);
    assert!(
        notes::append(
            fixture.root.path(),
            REFERENCE,
            request(saved, &notes::draft_id().unwrap())
        )
        .is_err()
    );
}
