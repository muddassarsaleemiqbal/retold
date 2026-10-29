//! Version-bound comments separate old pixels, reject stale publication and recover exact retries.
#![expect(clippy::unwrap_used, reason = "isolated fixture assertions")]
use retold_app::{
    notes::{self, Append, Target},
    protocol::ErrorCode,
};
use std::path::PathBuf;

const REFERENCE: &str = "studio/film";
const COMMENT: &str = "हिंदी اردو English. Keep this exact frame.";

struct Fixture {
    root: tempfile::TempDir,
    video: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let video = create_video(root.path());
        Self { root, video }
    }
    fn frame(&self, bytes: &[u8]) -> Target {
        let directory = self.video.join("work/05-images");
        let path = write_pixels(&directory, bytes);
        let asset = serde_json::json!({"path":path,"content_hash":blake3::hash(bytes).to_hex().to_string(),"provider":"fixture","model":"fixture","usage":{"generation":null,"cost":0},"quote":{"ceiling":0,"provider_tag":null},"request_id":"fixture"});
        let records = serde_json::json!({"cover":asset,"style":asset,"sheets":{},"frames":[{"unit":0,"image":asset,"qa":{"style_score":10,"character_score":10,"text_present":false,"anatomy_errors":[],"problems":[],"focus":[0.5,0.4]},"regenerated":false}]});
        std::fs::write(
            directory.join("images.json"),
            serde_json::to_vec(&records).unwrap(),
        )
        .unwrap();
        Target::frame(0, bytes).unwrap()
    }
    fn read(&self, target: &Target) -> notes::ContextualDocument {
        notes::contextual_document(
            self.root.path(),
            REFERENCE,
            notes::ContextQuery { target, offset: 0 },
        )
        .unwrap()
    }
    fn append(
        &self,
        target: &Target,
        version: &str,
        id: &str,
    ) -> Result<notes::Saved, retold_app::protocol::Diagnostic> {
        notes::append_contextual(
            self.root.path(),
            REFERENCE,
            notes::ContextAppend {
                target,
                request: Append {
                    expected_version: version,
                    id,
                    item: &target.key().unwrap(),
                    text: COMMENT,
                },
            },
        )
    }
    fn journal(&self) -> PathBuf {
        self.video.join(".retold/review-notes.json")
    }
}

fn write_pixels(directory: &std::path::Path, bytes: &[u8]) -> PathBuf {
    std::fs::create_dir_all(directory).unwrap();
    let path = directory.join("frame.png");
    std::fs::write(&path, bytes).unwrap();
    path
}

fn create_video(root: &std::path::Path) -> PathBuf {
    let video = root.join(REFERENCE);
    std::fs::create_dir_all(video.join(".retold")).unwrap();
    save_manifest(&video);
    video
}

fn save_manifest(video: &std::path::Path) {
    let manifest = serde_json::json!({"schema":1,"id":"context-fixture", "created":"2026-10-26T00:00:00Z", "title":"Context notes", "guide":{"id":"fixture","version":"1","hash":"fixture"}, "channel":"studio","inputs":{},"variants":["en:en"],"quality":"standard","budget":0,"skip_reviews":false,"folder":["film"],"cast_levels":0});
    std::fs::write(
        video.join(".retold/run.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn replacement_separates_comments_and_old_retry_acknowledges_without_writing() {
    let fixture = Fixture::new();
    let target = fixture.frame(b"original pixels");
    let original = fixture.read(&target);
    assert_eq!(original.current, Some(target.clone()));
    assert!(!fixture.journal().exists());
    let id = notes::draft_id().unwrap();
    let saved = fixture
        .append(&target, &original.document.version, &id)
        .unwrap();
    let journal = std::fs::read(fixture.journal()).unwrap();
    let next = fixture.frame(b"replacement pixels");
    let historical = fixture.read(&target);
    assert_eq!(historical.document.total, 1);
    assert_eq!(historical.current, Some(next.clone()));
    assert!(historical.problem.is_some());
    assert_stale_and_retry(
        &fixture,
        (&target, &original.document.version, &id),
        (&saved.note, &journal),
    );
    assert_separate_versions(&fixture, &target, &next);
}

fn assert_separate_versions(fixture: &Fixture, target: &Target, next: &Target) {
    let current = fixture.read(next);
    assert_eq!(current.document.total, 0);
    fixture
        .append(next, &current.document.version, &notes::draft_id().unwrap())
        .unwrap();
    assert_eq!(fixture.read(target).document.total, 1);
    assert_eq!(fixture.read(next).document.total, 1);
    assert!(!fixture.root.path().join(".retold/studio").exists());
}

fn assert_stale_and_retry(
    fixture: &Fixture,
    original: (&Target, &str, &str),
    saved: (&notes::ReviewNote, &[u8]),
) {
    let (target, version, id) = original;
    let retry = fixture.append(target, version, id).unwrap();
    assert_eq!(&retry.note, saved.0);
    let version = fixture.read(target).document.version;
    let rejected = fixture
        .append(target, &version, &notes::draft_id().unwrap())
        .unwrap_err();
    assert_eq!(rejected.code, ErrorCode::PlanChanged);
    assert_eq!(std::fs::read(fixture.journal()).unwrap(), saved.1);
}

#[test]
fn missing_or_tampered_frame_retains_history_but_cannot_receive_a_new_comment() {
    let fixture = Fixture::new();
    let target = fixture.frame(b"verified");
    let document = fixture.read(&target).document;
    fixture
        .append(&target, &document.version, &notes::draft_id().unwrap())
        .unwrap();
    let journal = std::fs::read(fixture.journal()).unwrap();
    std::fs::write(fixture.video.join("work/05-images/frame.png"), b"tampered").unwrap();
    let observed = fixture.read(&target);
    assert!(observed.current.is_none());
    assert_eq!(observed.document.total, 1);
    assert!(
        fixture
            .append(
                &target,
                &observed.document.version,
                &notes::draft_id().unwrap()
            )
            .is_err()
    );
    std::fs::remove_file(fixture.video.join("work/05-images/images.json")).unwrap();
    assert_eq!(fixture.read(&target).document.total, 1);
    assert_eq!(std::fs::read(fixture.journal()).unwrap(), journal);
}

#[test]
fn generation_writer_and_reserved_context_keys_cannot_be_bypassed() {
    let fixture = Fixture::new();
    let target = fixture.frame(b"pixels");
    let document = fixture.read(&target).document;
    let id = notes::draft_id().unwrap();
    let request = Append {
        expected_version: &document.version,
        id: &id,
        item: &target.key().unwrap(),
        text: COMMENT,
    };
    assert!(notes::append(fixture.root.path(), REFERENCE, request).is_err());
    let layout =
        retold_engine::RunLayout::from_video_dir(fixture.root.path(), "studio", &fixture.video, 2)
            .unwrap();
    let _owner = retold_engine::store::Store::open(&layout).unwrap();
    assert!(fixture.append(&target, &document.version, &id).is_err());
    assert!(!fixture.journal().exists());
}

#[test]
fn typed_subjects_have_distinct_bounded_keys_and_validate_deserialized_context() {
    let section = retold_engine::story::SectionText {
        section: "हिंदी / opening".into(),
        text: "one".into(),
    };
    let original = Target::script(&section).unwrap();
    let changed = Target::script(&retold_engine::story::SectionText {
        text: "two".into(),
        ..section
    })
    .unwrap();
    assert_ne!(original.key().unwrap(), changed.key().unwrap());
    let frame = Target::frame(0, b"pixels").unwrap();
    let character = Target::character("original / character", frame.content_hash()).unwrap();
    assert_ne!(frame.key().unwrap(), character.key().unwrap());
    let invalid: Target = serde_json::from_value(
        serde_json::json!({"subject":{"kind":"frame","unit":0},"content_hash":"forged"}),
    )
    .unwrap();
    assert!(invalid.key().is_err());
}
