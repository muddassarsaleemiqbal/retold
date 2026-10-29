//! Draft publication must never expose partial runs or overwrite existing user files.
#![expect(
    clippy::unwrap_used,
    reason = "fixture and worker failures must fail the test"
)]
use std::path::{Path, PathBuf};
use std::sync::Barrier;

use indexmap::IndexMap;
use retold_core::Usd;
use retold_engine::{EngineError, NewRun, Quality, RunLayout, run};
use retold_guide::{Channel, Guide, Origin, catalog::bundled, channel::init_from_template};

struct Fixture {
    root: tempfile::TempDir,
    library: PathBuf,
    guide: Guide,
    channel: Channel,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let channel = init_from_template("test-studio", &root.path().join("cfg/studio")).unwrap();
        let guide = fixture_guide();
        Self {
            library: root.path().join("library"),
            root,
            guide,
            channel,
        }
    }

    fn request(&self) -> NewRun<'_> {
        let inputs: IndexMap<String, String> = [
            ("title".into(), "The Story".into()),
            ("scope".into(), "Season 1".into()),
        ]
        .into();
        NewRun {
            guide: &self.guide,
            channel: &self.channel,
            inputs: retold_guide::inputs::resolve(self.guide.manifest(), &inputs).unwrap(),
            variants: vec!["en".parse().unwrap()],
            quality: Quality::Standard,
            budget: Usd::ZERO,
            skip_reviews: false,
            source_file: Option::default(),
        }
    }

    fn layout(&self) -> RunLayout {
        let request = self.request();
        RunLayout::resolve(
            &self.library,
            request.channel,
            request.guide,
            &request.inputs,
        )
        .unwrap()
    }

    fn create(&self) -> Result<(RunLayout, run::RunMeta), EngineError> {
        run::create(&self.library, self.request())
    }
}

fn fixture_guide() -> Guide {
    bundled()
        .unwrap()
        .into_iter()
        .find(|guide| guide.id() == "narrative-fixture")
        .unwrap()
}

#[test]
fn failed_snapshot_leaves_no_visible_run_or_staging_tree() {
    let mut fixture = Fixture::new();
    let mut files = fixture.guide.files().clone();
    files.insert(
        "voice.md/child.txt".into(),
        "file and parent cannot coexist".into(),
    );
    fixture.guide = Guide::from_files(Origin::Inline, files).unwrap();
    let destination = fixture.layout().video_dir().to_owned();
    let failure = fixture.create().unwrap_err();
    assert!(!matches!(failure, EngineError::RunExists { .. }));
    assert!(!destination.exists());
    assert!(run::find_all(&fixture.library).is_empty());
    assert!(
        std::fs::read_dir(destination.parent().unwrap())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn unowned_directories_and_files_are_preserved() {
    let fixture = Fixture::new();
    let destination = fixture.layout().video_dir().to_owned();
    std::fs::create_dir_all(&destination).unwrap();
    let original = destination.join("notes.txt");
    std::fs::write(&original, "user owned").unwrap();
    assert!(matches!(
        fixture.create(),
        Err(EngineError::RunExists { .. })
    ));
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "user owned");
    assert!(!destination.join(".retold").exists());
    std::fs::remove_dir_all(&destination).unwrap();
    std::fs::write(&destination, "another user file").unwrap();
    assert!(matches!(
        fixture.create(),
        Err(EngineError::RunExists { .. })
    ));
    assert_eq!(
        std::fs::read_to_string(destination).unwrap(),
        "another user file"
    );
}

#[test]
fn concurrent_creators_publish_exactly_one_complete_draft() {
    const CREATORS: usize = 8;
    let fixture = Fixture::new();
    let barrier = Barrier::new(CREATORS);
    let results = concurrent_results(&fixture, &barrier);
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(EngineError::RunExists { .. })))
            .count(),
        CREATORS - 1
    );
    let found = run::find_all(&fixture.library);
    assert_eq!(found.len(), 1);
    let snapshot = Guide::from_dir(&found[0].layout.guide_snapshot()).unwrap();
    assert_eq!(snapshot.content_hash(), found[0].meta.guide.hash);
    let destination = found[0].layout.video_dir();
    assert_no_staging(destination.parent().unwrap());
}

fn concurrent_results(
    fixture: &Fixture,
    barrier: &Barrier,
) -> Vec<Result<(RunLayout, run::RunMeta), EngineError>> {
    const CREATORS: usize = 8;
    std::thread::scope(|scope| {
        let workers = (0..CREATORS)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    fixture.create()
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    })
}

fn assert_no_staging(parent: &Path) {
    assert!(std::fs::read_dir(parent).unwrap().all(|entry| {
        let name = entry.unwrap().file_name();
        !name.to_string_lossy().starts_with(".retold-create-")
    }));
}

#[test]
fn recreating_a_path_never_reuses_its_accounting_identity() {
    const GENERATIONS: usize = 16;
    let fixture = Fixture::new();
    let mut identities = std::collections::HashSet::new();
    for _ in 0..GENERATIONS {
        let (layout, meta) = fixture.create().unwrap();
        assert!(meta.id.starts_with("video-"));
        assert_eq!(meta.id.len(), "video-".len() + 32);
        assert!(identities.insert(meta.id));
        assert_eq!(meta.folder, ["series", "the-story", "season-1"]);
        std::fs::remove_dir_all(layout.video_dir()).unwrap();
    }
    assert_eq!(identities.len(), GENERATIONS);
}

#[test]
fn invalid_source_and_editions_are_rejected_before_creating_library() {
    let fixture = Fixture::new();
    let source = fixture.root.path().join("source.txt");
    for bytes in [vec![], vec![b'x'; 2_000_001], vec![0xff]] {
        std::fs::write(&source, bytes).unwrap();
        let mut request = fixture.request();
        request.source_file = Some(&source);
        assert!(run::create(&fixture.library, request).is_err());
        assert!(!fixture.library.exists());
    }
    let mut request = fixture.request();
    request.variants.push(request.variants[0].clone());
    assert!(run::create(&fixture.library, request).is_err());
    assert!(!fixture.library.exists());
    let mut request = fixture.request();
    request.budget = Usd::from_micros(-1);
    assert!(run::create(&fixture.library, request).is_err());
    assert!(!fixture.library.exists());
}

#[test]
fn distinct_caption_editions_may_share_one_narration_language() {
    let fixture = Fixture::new();
    let mut request = fixture.request();
    request.variants = vec!["ur:ur".parse().unwrap(), "ur:hi".parse().unwrap()];
    let (_, meta) = run::create(&fixture.library, request).unwrap();
    assert_eq!(meta.variants.len(), 2);
    assert_eq!(meta.variants[0].narration, meta.variants[1].narration);
    assert_ne!(meta.variants[0].captions, meta.variants[1].captions);
}

#[test]
fn source_and_guide_are_frozen_before_publication() {
    let fixture = Fixture::new();
    let source = fixture.root.path().join("source.txt");
    std::fs::write(&source, "User supplied source\n").unwrap();
    let mut request = fixture.request();
    request.source_file = Some(&source);
    let (layout, meta) = run::create(&fixture.library, request).unwrap();
    std::fs::write(&source, "changed").unwrap();
    assert!(meta.source_file);
    let saved = retold_engine::research::read_saved_source(&layout).unwrap();
    assert_eq!(saved.as_deref(), Some("User supplied source\n"));
    assert_eq!(
        Guide::from_dir(&layout.guide_snapshot())
            .unwrap()
            .content_hash(),
        meta.guide.hash
    );
    let saved_meta: run::RunMeta =
        serde_json::from_slice(&std::fs::read(layout.run_file()).unwrap()).unwrap();
    assert_eq!(saved_meta.id, meta.id);
}

#[test]
fn omitted_folder_levels_preserve_cast_ownership_when_reopened() {
    let mut fixture = Fixture::new();
    fixture.guide = guide_with_omitted_level(&fixture.guide);
    let (layout, meta) = fixture.create().unwrap();
    assert_eq!(meta.cast_levels, 1);
    let reopened = run::find_all(&fixture.library);
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened[0].layout.cast_dir(), layout.cast_dir());
}

fn guide_with_omitted_level(guide: &Guide) -> Guide {
    let mut files = guide.files().clone();
    files.insert(
        "guide.toml".into(),
        omitted_level_manifest(&files["guide.toml"]),
    );
    Guide::from_files(Origin::Inline, files).unwrap()
}

fn omitted_level_manifest(source: &str) -> String {
    const FOLDERS: [&str; 3] = ["", "{{ inputs.title }}", "{{ inputs.scope }}"];
    let mut manifest = source.parse::<toml::Table>().unwrap();
    let rules = manifest.get_mut("run").unwrap();
    let rules = rules.as_table_mut().unwrap();
    rules.insert("folder".into(), toml::Value::try_from(FOLDERS).unwrap());
    toml::to_string(&manifest).unwrap()
}

#[cfg(unix)]
#[test]
fn symlinked_parent_cannot_write_outside_library() {
    let fixture = Fixture::new();
    let outside = fixture.root.path().join("outside");
    std::fs::create_dir(&fixture.library).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, fixture.library.join("test-studio")).unwrap();
    assert!(fixture.create().is_err());
    assert!(std::fs::read_dir(outside).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn dangling_destination_symlink_is_preserved() {
    let fixture = Fixture::new();
    let destination = fixture.layout().video_dir().to_owned();
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    let original = fixture.root.path().join("missing-target");
    std::os::unix::fs::symlink(&original, &destination).unwrap();
    assert!(matches!(
        fixture.create(),
        Err(EngineError::RunExists { .. })
    ));
    assert_eq!(std::fs::read_link(destination).unwrap(), original);
}
