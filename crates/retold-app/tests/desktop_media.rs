//! Saved-media pages disclose incomplete scans, filter editions and never initialize work.
#![expect(clippy::unwrap_used, reason = "isolated fixture assertions")]
use std::path::{Path, PathBuf};

use retold_app::desktop::media::{self, Kind};

const VOICE_LINES: usize = 60;
const OVER_SCAN_LIMIT: usize = 4100;
const OVERSIZED_TEXT: u64 = 131_073;

fn production(root: &Path) -> PathBuf {
    let directory = root.join("studio/film");
    std::fs::create_dir_all(directory.join(".retold")).unwrap();
    save_manifest(&directory);
    directory
}

fn save_manifest(directory: &Path) {
    let manifest = serde_json::json!({"schema":1,"id":"media-fixture", "created":"2026-10-26T00:00:00Z", "title":"Saved media", "guide":{"id":"fixture","version":"1","hash":"fixture"}, "channel":"studio","inputs":{},"variants":["en:en", "hi:hi"],"quality":"standard","budget":0,"skip_reviews":false,"folder":["film"],"cast_levels":0});
    std::fs::write(
        directory.join(".retold/run.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}

fn write(directory: &Path, relative: &str) {
    let path = directory.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"saved fixture").unwrap();
}

fn english_page(root: &Path, offset: usize) -> media::Page {
    let query = media::Query {
        edition: Some("en:en".into()),
        offset,
    };
    media::page(root, "studio/film", &query).unwrap()
}

fn populate(video: &Path) {
    for path in [
        "deliverables/en/story.mp4",
        "deliverables/hi/story.mp4",
        "work/scenes/scene.png",
    ] {
        write(video, path);
    }
    for index in 0..VOICE_LINES {
        write(video, &format!("work/voice/en/line-{index:03}.wav"));
    }
}

#[test]
fn edition_filter_keeps_shared_scenes_and_pages_without_queue_writes() {
    let root = tempfile::tempdir().unwrap();
    populate(&production(root.path()));
    let first = english_page(root.path(), 0);
    assert_eq!(first.total, VOICE_LINES + 2);
    assert_eq!(first.items.len(), media::PAGE_SIZE);
    assert!(!first.partial);
    let second = english_page(root.path(), first.next_offset.unwrap());
    assert_eq!(second.items.len(), VOICE_LINES + 2 - media::PAGE_SIZE);
    assert!(second.next_offset.is_none());
    assert!(
        first
            .items
            .iter()
            .any(|item| item.kind == Kind::Image && item.edition.is_none())
    );
    assert!(
        !first
            .items
            .iter()
            .chain(&second.items)
            .any(|item| item.relative.contains("/hi/"))
    );
    assert!(!root.path().join(".retold/studio").exists());
}

#[test]
fn unsupported_edition_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    production(root.path());
    let query = media::Query {
        edition: Some("fr".into()),
        offset: 0,
    };
    assert!(media::page(root.path(), "studio/film", &query).is_err());
}

#[test]
fn scan_and_preview_limits_disclose_partial_and_reject_oversized_files() {
    let root = tempfile::tempdir().unwrap();
    let video = production(root.path());
    write(&video, "work/caption.srt");
    let mut page = media::page(root.path(), "studio/film", &media::Query::default()).unwrap();
    let caption = page.items.pop().unwrap();
    oversized_preview_is_rejected(&caption);
    for index in 0..OVER_SCAN_LIMIT {
        write(&video, &format!("large/{index:04}.png"));
    }
    let page = media::page(root.path(), "studio/film", &media::Query::default()).unwrap();
    assert!(page.partial);
    assert!(page.total < OVER_SCAN_LIMIT + 1);
}

fn oversized_preview_is_rejected(caption: &media::Item) {
    assert_eq!(caption.preview_bytes().unwrap(), b"saved fixture");
    std::fs::File::create(&caption.path)
        .unwrap()
        .set_len(OVERSIZED_TEXT)
        .unwrap();
    assert!(caption.preview_bytes().is_err());
}

fn frame_asset(path: &Path) -> serde_json::Value {
    let bytes = std::fs::read(path).unwrap();
    serde_json::json!({"path":path,"content_hash":blake3::hash(&bytes).to_hex().to_string(),"provider":"fixture","model":"fixture","usage":{"generation":null,"cost":0},"quote":{"ceiling":0,"provider_tag":null},"request_id":"fixture"})
}

fn save_frame_records(video: &Path, frames: Vec<(usize, serde_json::Value)>) {
    let first = frames[0].1.clone();
    let frames: Vec<_> = frames.into_iter().map(|(unit, image)| serde_json::json!({"unit":unit,"image":image,"qa":{"style_score":10,"character_score":10,"text_present":false,"anatomy_errors":[],"problems":[],"focus":[0.5,0.4]},"regenerated":false})).collect();
    let record = serde_json::json!({"cover":first,"frames":frames,"style":first,"sheets":{}});
    std::fs::write(
        video.join("work/05-images/images.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
}

#[test]
fn scene_frame_selection_uses_recorded_unit_paths_and_content_hashes() {
    let root = tempfile::tempdir().unwrap();
    let video = production(root.path());
    write(&video, "work/05-images/unit-0.png");
    write(&video, "work/05-images/unit-1-panel.png");
    let original = video.join("work/05-images/unit-0.png");
    let panel = video.join("work/05-images/unit-1-panel.png");
    save_frame_records(
        &video,
        vec![(0, frame_asset(&original)), (1, frame_asset(&panel))],
    );
    let frame = media::scene_frame(root.path(), "studio/film", 1)
        .unwrap()
        .unwrap();
    assert!(frame.relative.ends_with("unit-1-panel.png"));
    assert_eq!(frame.preview_bytes().unwrap(), b"saved fixture");
    assert!(
        media::scene_frame(root.path(), "studio/film", VOICE_LINES)
            .unwrap()
            .is_none()
    );
    assert!(!root.path().join(".retold/studio").exists());
}

#[test]
fn frame_hash_rejects_same_size_changes_even_when_file_identity_and_time_are_preserved() {
    let root = tempfile::tempdir().unwrap();
    let video = production(root.path());
    write(&video, "work/05-images/unit-0.png");
    let path = video.join("work/05-images/unit-0.png");
    save_frame_records(&video, vec![(0, frame_asset(&path))]);
    let frame = media::scene_frame(root.path(), "studio/film", 0)
        .unwrap()
        .unwrap();
    change_bytes_preserving_time(&path);
    frame.validated_path().unwrap();
    assert!(frame.preview_bytes().is_err());
}

fn change_bytes_preserving_time(path: &Path) {
    let modified = std::fs::metadata(path).unwrap().modified().unwrap();
    std::fs::write(path, b"other fixture").unwrap();
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    let file = options.open(path).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
}

#[test]
fn scene_records_cannot_open_a_frame_from_another_production() {
    let root = tempfile::tempdir().unwrap();
    let video = production(root.path());
    write(&video, "work/05-images/unit-0.png");
    let outside = root.path().join("other-video.png");
    std::fs::write(&outside, b"saved fixture").unwrap();
    save_frame_records(&video, vec![(0, frame_asset(&outside))]);
    assert!(media::scene_frame(root.path(), "studio/film", 0).is_err());
}

#[test]
#[cfg(unix)]
fn external_symlinks_and_cycles_are_omitted_and_disclosed() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let video = production(root.path());
    write(external.path(), "outside.png");
    std::os::unix::fs::symlink(external.path(), video.join("outside")).unwrap();
    std::os::unix::fs::symlink(&video, video.join("cycle")).unwrap();
    let page = media::page(root.path(), "studio/film", &media::Query::default()).unwrap();
    assert!(page.partial);
    assert!(page.items.is_empty());
}

#[test]
fn a_selected_file_cannot_be_opened_after_replacement() {
    let root = tempfile::tempdir().unwrap();
    let video = production(root.path());
    write(&video, "deliverables/en/story.mp4");
    let item = media::page(root.path(), "studio/film", &media::Query::default())
        .unwrap()
        .items
        .remove(0);
    assert_eq!(item.validated_path().unwrap(), item.path);
    std::fs::write(&item.path, b"replaced").unwrap();
    assert!(item.validated_path().is_err());
}

#[test]
#[cfg(unix)]
fn replacing_the_inode_is_detected_even_with_preserved_size_and_time() {
    let root = tempfile::tempdir().unwrap();
    let video = production(root.path());
    write(&video, "work/scene.png");
    let item = media::page(root.path(), "studio/film", &media::Query::default())
        .unwrap()
        .items
        .remove(0);
    let metadata = std::fs::metadata(&item.path).unwrap();
    let modified = metadata.modified().unwrap();
    let retained = item.path.with_extension("old");
    std::fs::rename(&item.path, retained).unwrap();
    std::fs::write(&item.path, b"saved fixture").unwrap();
    let times = std::fs::FileTimes::new().set_modified(modified);
    let replacement = std::fs::File::options()
        .write(true)
        .open(&item.path)
        .unwrap();
    replacement.set_times(times).unwrap();
    assert!(item.validated_path().is_err());
}
