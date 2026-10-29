//! Automatic previews must not disclose credentials or open special files.
use crate::tui::files::{Task, run_task};
use retold_app::protocol::Payload;
use serde_json::Value;
use std::path::Path;

const SYNTHETIC_SECRET: &str = "AUDIT-SYNTHETIC-CREDENTIAL-CANARY";
const LONG_TEXT_CHARACTERS: usize = 6_000;
const PREVIEW_WAIT: std::time::Duration = std::time::Duration::from_secs(1);
const BROWSER_LIMIT: usize = 2_000;

fn listing(path: &Path) -> Value {
    let payload = run_task(&Task::Browse(path.to_path_buf()));
    let Payload::Result { contents, .. } = payload else {
        panic!("listing failed: {payload:?}");
    };
    contents
}

#[test]
fn large_directories_report_when_the_listing_is_partial() {
    let root = tempfile::tempdir().unwrap();
    for index in 0..BROWSER_LIMIT {
        std::fs::write(root.path().join(format!("file-{index:04}.txt")), "public").unwrap();
    }
    assert_eq!(listing(root.path())["truncated"], false);
    std::fs::write(root.path().join("one-more.txt"), "public").unwrap();
    let contents = listing(root.path());
    assert_eq!(contents["entries"].as_array().unwrap().len(), BROWSER_LIMIT);
    assert_eq!(contents["truncated"], true);
}

#[test]
fn partial_browser_lists_explain_the_filter_scope_and_unavailable_entries() {
    let mut app = super::fresh();
    let mut browser = crate::tui::files::Browser::new(1, "/public".into(), Option::default());
    browser.listing = Some(super::result(serde_json::json!({
        "entries": [], "limit": 2000, "truncated": true, "unavailable": 1
    })));
    app.modal = Some(super::Modal::Browser(browser));
    let screen = super::screen(&app, super::NORMAL.0, super::NORMAL.1);
    assert!(screen.contains("partial"), "{screen}");
    assert!(screen.contains("2000"), "{screen}");
    assert!(screen.contains("1 unavailable"), "{screen}");
}

#[cfg(target_os = "linux")]
#[test]
fn unsupported_filenames_preserve_other_browser_entries() {
    use std::os::unix::ffi::OsStrExt;
    let root = tempfile::tempdir().unwrap();
    let unsupported = root
        .path()
        .join(std::ffi::OsStr::from_bytes(b"bad-\xff.txt"));
    std::fs::write(&unsupported, "public").unwrap();
    std::fs::write(root.path().join("safe.txt"), "public").unwrap();
    let contents = listing(root.path());
    assert_eq!(contents["entries"].as_array().unwrap().len(), 1);
    assert_eq!(contents["entries"][0]["name"], "safe.txt");
    assert_eq!(contents["unavailable"], 1);
    assert_eq!(std::fs::read_to_string(unsupported).unwrap(), "public");
}

#[cfg(target_os = "linux")]
#[test]
fn an_unsupported_directory_path_returns_an_error_without_panicking() {
    use std::os::unix::ffi::OsStrExt;
    let root = tempfile::tempdir().unwrap();
    let unsupported = root.path().join(std::ffi::OsStr::from_bytes(b"bad-\xff"));
    std::fs::create_dir(&unsupported).unwrap();
    let payload = run_task(&Task::Browse(unsupported));
    assert!(matches!(payload, Payload::Error { .. }), "{payload:?}");
}

fn preview(path: &Path) -> Value {
    let payload = run_task(&Task::Preview(path.to_path_buf()));
    let Payload::Result { contents, .. } = payload else {
        panic!("preview failed: {payload:?}");
    };
    contents
}

#[cfg(unix)]
#[test]
fn an_innocent_symlink_name_does_not_expose_a_sensitive_target() {
    let root = tempfile::tempdir().unwrap();
    let secret = root.path().join("auth.json");
    std::fs::write(&secret, SYNTHETIC_SECRET).unwrap();
    let alias = root.path().join("notes.txt");
    std::os::unix::fs::symlink(secret, &alias).unwrap();
    let contents = preview(&alias);
    assert!(!contents.to_string().contains(SYNTHETIC_SECRET));
    assert!(
        contents["text"]
            .as_str()
            .unwrap()
            .contains("preview hidden")
    );
}

#[test]
fn files_inside_credential_profiles_are_not_automatically_previewed() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join(".codex_personal");
    std::fs::create_dir(&profile).unwrap();
    let settings = profile.join("settings.toml");
    std::fs::write(&settings, SYNTHETIC_SECRET).unwrap();
    let contents = preview(&settings);
    assert!(!contents.to_string().contains(SYNTHETIC_SECRET));
    assert!(
        contents["text"]
            .as_str()
            .unwrap()
            .contains("preview hidden")
    );
}

#[test]
fn an_image_extension_does_not_make_a_directory_an_image() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("folder.png");
    std::fs::create_dir(&directory).unwrap();
    assert!(
        preview(&directory)["text"]
            .as_str()
            .unwrap()
            .contains("Directory")
    );
}

#[cfg(unix)]
#[test]
fn an_image_named_pipe_is_rejected_without_opening_it() {
    let root = tempfile::tempdir().unwrap();
    let pipe = root.path().join("pipe.png");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .unwrap()
            .success()
    );
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || sender.send(preview(&pipe)).unwrap());
    let contents = receiver.recv_timeout(PREVIEW_WAIT).unwrap();
    assert!(contents["text"].as_str().unwrap().contains("Special file"));
    worker.join().unwrap();
}

#[test]
fn a_truncated_unicode_text_file_keeps_its_valid_prefix() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("notes.txt");
    std::fs::write(&path, "€".repeat(LONG_TEXT_CHARACTERS)).unwrap();
    let contents = preview(&path);
    assert_eq!(contents["truncated"], true);
    assert!(contents["text"].as_str().unwrap().starts_with('€'));
    assert!(!contents.to_string().contains("Media or binary file"));
}

#[test]
fn ordinary_public_text_remains_visible_and_is_not_modified() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("notes.txt");
    let text = "Public notes: اردو, हिन्दी and e\u{301}.";
    std::fs::write(&path, text).unwrap();
    let contents = preview(&path);
    assert_eq!(contents["text"], text);
    assert_eq!(contents["truncated"], false);
    assert_eq!(std::fs::read_to_string(path).unwrap(), text);
}
