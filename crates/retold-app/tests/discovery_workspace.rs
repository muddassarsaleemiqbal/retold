//! Workspace-owned studio state is not a malformed video manifest.

#[test]
fn studio_state_at_library_root_does_not_make_the_catalog_incomplete() {
    let temporary = tempfile::tempdir().unwrap();
    let library = temporary.path().join("library");
    let studio = library.join(".retold/studio");
    std::fs::create_dir_all(&studio).unwrap();
    let catalog =
        retold_app::library::page(&library, &retold_app::library::ListOptions::default()).unwrap();
    assert!(catalog.items.is_empty());
    assert!(catalog.issues.is_empty());
}
