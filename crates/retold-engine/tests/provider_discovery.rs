//! Offline discovery never executes discovered files or inspects login contents.
#![expect(clippy::unwrap_used, reason = "fixture assertions")]
use retold_engine::provider_discovery::scan;

#[test]
fn inventories_profiles_and_tools_without_executing_them() {
    let root = tempfile::tempdir().unwrap();
    let (bin, tool) = tool_fixture(root.path());
    for name in [".codex_third", ".claude", ".unrelated"] {
        std::fs::create_dir(root.path().join(name)).unwrap();
    }
    let inventory = scan(root.path(), bin.as_os_str()).unwrap();
    let codex = inventory
        .tools
        .iter()
        .find(|tool| tool.name == "codex")
        .unwrap();
    assert_eq!(codex.path.as_ref(), Some(&tool));
    assert_eq!(inventory.profiles.len(), 2);
    assert_eq!(inventory.authentication, "not_checked");
    assert_eq!(inventory.versions, "not_checked");
    assert_eq!(
        std::fs::read_to_string(tool).unwrap(),
        "not an executable program"
    );
}

fn tool_fixture(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let tool = bin.join("codex");
    std::fs::write(&tool, "not an executable program").unwrap();
    executable(&tool);
    (bin, tool)
}

#[cfg(unix)]
fn executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    const TOOL_MODE: u32 = 0o755;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(TOOL_MODE)).unwrap();
}
#[cfg(not(unix))]
fn executable(_: &std::path::Path) {}

#[test]
fn missing_home_and_excessive_search_paths_fail_explicitly() {
    const EXCESSIVE_ENTRIES: usize = 257;
    let root = tempfile::tempdir().unwrap();
    assert!(scan(&root.path().join("missing"), std::ffi::OsStr::new("")).is_err());
    let directories = vec![root.path(); EXCESSIVE_ENTRIES];
    let search_path = std::env::join_paths(directories).unwrap();
    assert!(scan(root.path(), &search_path).is_err());
}

#[cfg(unix)]
#[test]
fn nonexecutable_files_and_directories_are_not_tools() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("claude");
    std::fs::write(&file, "untrusted").unwrap();
    std::fs::create_dir(root.path().join("codex")).unwrap();
    let inventory = scan(root.path(), root.path().as_os_str()).unwrap();
    assert!(inventory.tools.iter().all(|tool| tool.path.is_none()));
}
