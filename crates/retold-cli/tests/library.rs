//! Real filesystem/closed-stdin query contracts, including cursor and cache corruption boundaries.
#![expect(
    clippy::unwrap_used,
    reason = "fixture helpers must fail on invalid test setup or protocol"
)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const EARLIER: &str = "2026-10-18T00:00:00Z";
const LATER: &str = "2026-10-18T00:00:00.1Z";

#[path = "library/paging.rs"]
mod paging_tests;

fn saved(root: &Path, reference: &str, identity: (&str, &str)) -> PathBuf {
    let (channel, folder) = reference.split_once('/').unwrap();
    let metadata = metadata_directory(root, reference);
    let manifest = json!({"schema":1,"id":identity.0,"created":identity.1,"title":folder,
        "guide":{"id":"fixture","version":"1","hash":"fixture"},"channel":channel,"inputs":{},
        "variants":["en:en"],"quality":"standard","budget":0,"skip_reviews":true,
        "folder":folder.split('/').collect::<Vec<_>>(),"cast_levels":0});
    serde_json::from_value::<retold_engine::RunMeta>(manifest.clone()).unwrap();
    save_json(metadata.join("run.json"), &manifest);
    metadata
}

fn metadata_directory(root: &Path, reference: &str) -> PathBuf {
    let metadata = root.join(reference).join(".retold");
    std::fs::create_dir_all(&metadata).unwrap();
    metadata
}

fn cache_directory(metadata: &Path) -> PathBuf {
    let directory = metadata.join("cache");
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn save_json(path: PathBuf, contents: &Value) {
    std::fs::write(path, contents.to_string()).unwrap();
}

fn query(root: &Path, args: &[&str]) -> Value {
    let mut command = Command::cargo_bin("retold").unwrap();
    // Plans read installation settings and the account; never the developer's own.
    let installation = root.with_extension("installation");
    command
        .env("HOME", &installation)
        .env("RETOLD_CONFIG_DIR", installation.join("config"))
        .env_remove("XDG_DATA_HOME");
    command.arg("--json").arg("--library").arg(root);
    let assertion = command.args(args).assert();
    let output = assertion.get_output().clone();
    assert_eq!(output.stderr, Vec::<u8>::new());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    let envelope: Value = serde_json::from_str(&stdout).unwrap();
    let code = envelope["payload"]["code"]
        .as_str()
        .and_then(retold_app::protocol::ErrorCode::parse);
    let expected = code.map_or(0, retold_app::protocol::ErrorCode::exit_code);
    assert_eq!(output.status.code(), Some(i32::from(expected)));
    envelope
}

fn cache(metadata: &Path) -> PathBuf {
    let contents = json!({"text":"fixture script"});
    let key = retold_engine::store::hash(&"inputs").unwrap();
    let output_hash = retold_engine::store::hash(&contents).unwrap();
    let record = json!({"key":key,"output_hash":output_hash,"receipt":null});
    let envelope = json!({"schema":1,"step":"script","record":record,"value":contents});
    let state =
        json!({"schema":1,"active":null,"error":"SECRET-RAW-ERROR","completed":{"script":record}});
    let directory = cache_directory(metadata);
    save_json(metadata.join("state.json"), &state);
    let path = directory.join(format!("script-{key}.json"));
    save_json(path.clone(), &envelope);
    path
}

fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut contents = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            contents.extend(files(&entry.path()));
        } else {
            contents.push((entry.path(), std::fs::read(entry.path()).unwrap()));
        }
    }
    contents.sort();
    contents
}

#[test]
fn pagination_is_ordered_by_real_time_and_bound_to_filters_and_metadata() {
    let root = tempfile::tempdir().unwrap();
    saved(root.path(), "channel/older", ("vid-a", EARLIER));
    saved(root.path(), "channel/newer", ("vid-b", LATER));
    let first = query(root.path(), &["video", "list", "--limit", "1"]);
    assert_eq!(first["payload"]["data"]["items"][0]["id"], "vid-b");
    let cursor = first["payload"]["data"]["next_cursor"].as_str().unwrap();
    let second = query(
        root.path(),
        &["video", "list", "--limit", "1", "--cursor", cursor],
    );
    assert_eq!(second["payload"]["data"]["items"][0]["id"], "vid-a");
    let changed = query(
        root.path(),
        &["video", "list", "--channel", "channel", "--cursor", cursor],
    );
    assert_eq!(changed["payload"]["code"], "RG-CURSOR-INVALID");
    saved(root.path(), "channel/third", ("vid-c", LATER));
    assert_eq!(
        query(root.path(), &["video", "list", "--cursor", cursor])["payload"]["code"],
        "RG-CURSOR-INVALID"
    );
}

#[test]
fn references_are_explicit_and_ambiguity_is_actionable() {
    let root = tempfile::tempdir().unwrap();
    saved(root.path(), "one/shared", ("vid-a", EARLIER));
    saved(root.path(), "two/shared", ("vid-b", LATER));
    assert_eq!(
        query(root.path(), &["video", "show", "shared"])["payload"]["code"],
        "RG-REF-AMBIGUOUS"
    );
    let shown = query(root.path(), &["video", "show", "vid-a"]);
    assert_eq!(shown["payload"]["data"]["video"]["ref"], "one/shared");
    assert_eq!(
        query(root.path(), &["video", "show", "two/sh"])["payload"]["data"]["video"]["id"],
        "vid-b"
    );
    assert_eq!(
        query(root.path(), &["video", "show", "missing"])["payload"]["code"],
        "RG-REF-NOT-FOUND"
    );
}

#[test]
fn plans_verify_envelopes_without_granting_reuse_authority_or_writing_files() {
    let root = tempfile::tempdir().unwrap();
    let metadata = saved(root.path(), "channel/example", ("vid-a", EARLIER));
    let cached = cache(&metadata);
    let before = files(root.path());
    let plan = query(root.path(), &["video", "plan", "vid-a"]);
    let contents = &plan["payload"]["data"];
    assert_eq!(
        contents["snapshot"]["recorded"]["steps"][0]["integrity"],
        "envelope_verified"
    );
    assert_eq!(contents["can_execute"], false);
    assert_eq!(contents["blockers"][0]["code"], "RG-ACCOUNT-REQUIRED");
    assert_eq!(contents["export_authorized"], false);
    assert!(contents["estimates"]["usd_micros"].is_null());
    assert!(!plan.to_string().contains("SECRET-RAW-ERROR"));
    assert_eq!(files(root.path()), before);
    std::fs::write(cached, "{}").unwrap();
    let corrupt = query(root.path(), &["video", "plan", "vid-a"]);
    assert_eq!(
        corrupt["payload"]["data"]["snapshot"]["recorded"]["steps"][0]["integrity"],
        "invalid"
    );
}

#[test]
fn corrupt_manifests_are_visible_and_prevent_silent_reference_selection() {
    let root = tempfile::tempdir().unwrap();
    saved(root.path(), "channel/valid", ("vid-a", EARLIER));
    let bad = saved(root.path(), "channel/invalid", ("vid-b", LATER));
    std::fs::write(bad.join("run.json"), "bad manifest").unwrap();
    let listing = query(root.path(), &["video", "list"]);
    assert_eq!(listing["payload"]["data"]["complete"], false);
    assert_eq!(
        listing["payload"]["data"]["issues"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        query(root.path(), &["video", "show", "vid-a"])["payload"]["code"],
        "RG-LIBRARY-INCOMPLETE"
    );
}

#[test]
fn corrupt_state_is_not_repaired_or_hidden_by_a_backup() {
    let root = tempfile::tempdir().unwrap();
    let metadata = saved(root.path(), "channel/example", ("vid-a", EARLIER));
    std::fs::write(metadata.join("state.json"), "broken").unwrap();
    let before = files(root.path());
    assert_eq!(
        query(root.path(), &["video", "show", "vid-a"])["payload"]["code"],
        "RG-DATA-INVALID"
    );
    assert_eq!(files(root.path()), before);
}

#[test]
fn empty_library_queries_leave_the_root_absent() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("not-created");
    let result = query(&missing, &["video", "list"]);
    assert_eq!(result["payload"]["data"]["total"], 0);
    assert!(!missing.exists());
}

#[test]
fn missing_envelopes_remain_missing_without_index_repair() {
    let root = tempfile::tempdir().unwrap();
    let metadata = saved(root.path(), "channel/example", ("vid-a", EARLIER));
    let cached = cache(&metadata);
    std::fs::remove_file(cached).unwrap();
    let before = files(root.path());
    let result = query(root.path(), &["video", "plan", "vid-a"]);
    assert_eq!(
        result["payload"]["data"]["snapshot"]["recorded"]["steps"][0]["integrity"],
        "missing"
    );
    assert_eq!(files(root.path()), before);
}

#[test]
fn modified_output_with_unchanged_record_hash_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let metadata = saved(root.path(), "channel/example", ("vid-a", EARLIER));
    let cached = cache(&metadata);
    let mut envelope: Value = serde_json::from_slice(&std::fs::read(&cached).unwrap()).unwrap();
    envelope["value"]["text"] = json!("a different script");
    save_json(cached, &envelope);
    let result = query(root.path(), &["video", "plan", "vid-a"]);
    assert_eq!(
        result["payload"]["data"]["snapshot"]["recorded"]["steps"][0]["integrity"],
        "invalid"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_metadata_is_reported_without_following_the_target() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let metadata = saved(root.path(), "channel/example", ("vid-a", EARLIER));
    let target = external.path().join("metadata");
    std::fs::rename(&metadata, &target).unwrap();
    std::os::unix::fs::symlink(&target, &metadata).unwrap();
    let result = query(root.path(), &["video", "list"]);
    assert_eq!(result["payload"]["data"]["total"], 0);
    assert_eq!(result["payload"]["data"]["complete"], false);
}

#[cfg(unix)]
#[test]
fn symlinked_cache_is_rejected_before_inspection() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let metadata = saved(root.path(), "channel/example", ("vid-a", EARLIER));
    cache(&metadata);
    let target = external.path().join("cache");
    let source = metadata.join("cache");
    std::fs::rename(&source, &target).unwrap();
    std::os::unix::fs::symlink(target, source).unwrap();
    assert_eq!(
        query(root.path(), &["video", "plan", "vid-a"])["payload"]["code"],
        "RG-DATA-INVALID"
    );
}

#[test]
fn plain_list_preserves_scripts_and_removes_terminal_controls() {
    let root = tempfile::tempdir().unwrap();
    let metadata = saved(root.path(), "channel/example", ("vid-a", EARLIER));
    let path = metadata.join("run.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    manifest["title"] = json!("हिंदी اردو\u{1b}[31m\nTitle");
    save_json(path, &manifest);
    let mut command = Command::cargo_bin("retold").unwrap();
    command.arg("--library").arg(root.path());
    let assertion = command
        .args(["--plain", "video", "list"])
        .assert()
        .success();
    let text = String::from_utf8(assertion.get_output().stdout.clone()).unwrap();
    assert!(text.contains("हिंदी اردو"));
    assert!(!text.contains('\u{1b}'));
    assert_eq!(text.lines().count(), 1);
}

#[test]
fn valid_channel_and_video_names_are_not_pruned_as_media_folders() {
    let root = tempfile::tempdir().unwrap();
    saved(root.path(), "cast/work", ("vid-a", EARLIER));
    let result = query(root.path(), &["video", "list"]);
    assert_eq!(result["payload"]["data"]["items"][0]["ref"], "cast/work");
}

#[test]
fn oversized_manifests_are_reported_without_parsing_them() {
    const OVERFLOW_BYTES: usize = 2 * 1024 * 1024 + 1;
    let root = tempfile::tempdir().unwrap();
    let metadata = saved(root.path(), "channel/example", ("vid-a", EARLIER));
    std::fs::write(metadata.join("run.json"), vec![b' '; OVERFLOW_BYTES]).unwrap();
    let result = query(root.path(), &["video", "list"]);
    assert_eq!(result["payload"]["data"]["complete"], false);
    assert_eq!(result["payload"]["data"]["total"], 0);
}

fn dummy_audio(dir: &Path, name: &str) -> PathBuf {
    let file_path = dir.join(name);
    std::fs::write(&file_path, b"RIFF....WAVEfmt ....data....").unwrap();
    file_path
}

#[test]
fn audio_music_import_and_list() {
    let root = tempfile::tempdir().unwrap();
    let file = dummy_audio(root.path(), "intro.wav");
    let imported = query(
        root.path(),
        &[
            "library",
            "music",
            "import",
            file.to_str().unwrap(),
            "--title",
            "Opening Theme",
            "--license",
            "cc-by",
        ],
    );
    assert_eq!(imported["type"], "result");
    assert_eq!(
        imported["payload"]["data"]["item"]["title"],
        "Opening Theme"
    );

    let listing = query(root.path(), &["library", "music", "list"]);
    let items = listing["payload"]["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Opening Theme");
}

#[test]
fn audio_music_tag_and_licence() {
    let root = tempfile::tempdir().unwrap();
    let file = dummy_audio(root.path(), "intro.wav");
    query(
        root.path(),
        &[
            "library",
            "music",
            "import",
            file.to_str().unwrap(),
            "--license",
            "cc-by",
        ],
    );
    let tagged = query(
        root.path(),
        &["library", "music", "tag", "intro", "chill", "outro"],
    );
    let tags = tagged["payload"]["data"]["item"]["tags"]
        .as_array()
        .unwrap();
    assert!(tags.iter().any(|item| item == "chill"));

    let inspected = query(root.path(), &["library", "music", "licence", "intro"]);
    assert_eq!(
        inspected["payload"]["data"]["item"]["license"]["license_type"],
        "cc-by"
    );
}

#[test]
fn audio_sfx_and_verification() {
    let root = tempfile::tempdir().unwrap();
    let file = dummy_audio(root.path(), "woosh.mp3");
    let imported = query(
        root.path(),
        &["library", "sfx", "import", file.to_str().unwrap()],
    );
    assert_eq!(imported["type"], "result");

    let checked = query(root.path(), &["library", "sfx", "check"]);
    assert_eq!(checked["payload"]["data"]["verified_count"], 1);
}

#[test]
fn video_copy_duplicates_manifest_with_new_id() {
    let root = tempfile::tempdir().unwrap();
    saved(root.path(), "channel/origin", ("vid-orig", EARLIER));
    let copied = query(
        root.path(),
        &[
            "video",
            "copy",
            "channel/origin",
            "--title",
            "Copied Origin",
        ],
    );
    assert_eq!(copied["type"], "result");
    assert_eq!(copied["payload"]["data"]["status"], "copied");
    assert_eq!(copied["payload"]["data"]["new_id"], "vid-orig-copy");
}

#[test]
fn video_archive_hides_from_normal_list() {
    let root = tempfile::tempdir().unwrap();
    saved(root.path(), "channel/toggle", ("vid-tog", EARLIER));
    let archived = query(root.path(), &["video", "archive", "channel/toggle"]);
    assert_eq!(archived["type"], "result");

    let normal_list = query(root.path(), &["video", "list"]);
    assert_eq!(
        normal_list["payload"]["data"]["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let archive_list = query(root.path(), &["video", "list", "--archive"]);
    assert_eq!(
        archive_list["payload"]["data"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn video_unarchive_restores_to_normal_list() {
    let root = tempfile::tempdir().unwrap();
    saved(root.path(), "channel/toggle", ("vid-tog", EARLIER));
    query(root.path(), &["video", "archive", "channel/toggle"]);

    let unarchived = query(root.path(), &["video", "unarchive", "channel/toggle"]);
    assert_eq!(unarchived["type"], "result");

    let restored_list = query(root.path(), &["video", "list"]);
    assert_eq!(
        restored_list["payload"]["data"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn video_delete_supports_trash_and_permanent() {
    let root = tempfile::tempdir().unwrap();
    saved(root.path(), "channel/trashed", ("vid-del1", EARLIER));
    saved(root.path(), "channel/purged", ("vid-del2", EARLIER));

    let trashed = query(root.path(), &["video", "delete", "channel/trashed"]);
    assert_eq!(trashed["payload"]["data"]["mode"], "trash");

    let purged = query(
        root.path(),
        &["video", "delete", "channel/purged", "--permanent"],
    );
    assert_eq!(purged["payload"]["data"]["mode"], "permanent");
}

#[test]
fn imported_wav_records_its_measured_duration_and_other_formats_stay_unknown() {
    let root = tempfile::tempdir().unwrap();
    let wav = root.path().join("silence.wav");
    std::fs::write(&wav, include_bytes!("fixtures/tui/silence.wav")).unwrap();
    let imported = query(
        root.path(),
        &["library", "music", "import", wav.to_str().unwrap()],
    );
    assert_eq!(imported["payload"]["data"]["item"]["duration_seconds"], 1.0);
    let other = root.path().join("theme.mp3");
    std::fs::write(&other, b"not decoded here").unwrap();
    let imported = query(
        root.path(),
        &["library", "music", "import", other.to_str().unwrap()],
    );
    assert_eq!(
        imported["payload"]["data"]["item"]["duration_seconds"],
        Value::Null
    );
}
