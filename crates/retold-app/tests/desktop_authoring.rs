//! Native authoring previews before creating, and edits only the version that was opened.
use retold_app::desktop::authoring::{self, Preview, Resource};

#[test]
fn a_guide_is_previewed_created_and_edited_once_per_opened_version() {
    let root = tempfile::tempdir().unwrap();
    let subject = (Resource::Guide, "night-ferry", "en");
    authoring::create(root.path(), subject, Preview::Only).unwrap();
    assert!(authoring::open(root.path(), Resource::Guide, "night-ferry").is_err());
    authoring::create(root.path(), subject, Preview::Write).unwrap();
    let opened = authoring::open(root.path(), Resource::Guide, "night-ferry").unwrap();
    authoring::edit(
        root.path(),
        &opened,
        ("/name", "Night Ferry", Preview::Only),
    )
    .unwrap();
    authoring::edit(
        root.path(),
        &opened,
        ("/name", "Night Ferry", Preview::Write),
    )
    .unwrap();
    let stale = authoring::edit(root.path(), &opened, ("/name", "Other", Preview::Write));
    assert!(stale.is_err());
    let reopened = authoring::open(root.path(), Resource::Guide, "night-ferry").unwrap();
    assert_eq!(reopened.value["name"], "Night Ferry");
}

#[test]
fn a_channel_is_created_in_its_language() {
    let root = tempfile::tempdir().unwrap();
    authoring::create(
        root.path(),
        (Resource::Channel, "studio", "hi"),
        Preview::Write,
    )
    .unwrap();
    assert!(authoring::open(root.path(), Resource::Channel, "studio").is_ok());
}

#[test]
fn the_bundled_example_guide_is_previewed_then_copied_once() {
    let root = tempfile::tempdir().unwrap();
    authoring::copy_example(root.path(), Preview::Only).unwrap();
    assert!(authoring::open(root.path(), Resource::Guide, authoring::EXAMPLE_GUIDE).is_err());
    authoring::copy_example(root.path(), Preview::Write).unwrap();
    assert!(authoring::open(root.path(), Resource::Guide, authoring::EXAMPLE_GUIDE).is_ok());
    assert!(authoring::copy_example(root.path(), Preview::Write).is_err());
}

#[test]
fn readiness_reports_missing_resources_without_writing() {
    let root = tempfile::tempdir().unwrap();
    let items = retold_app::desktop::readiness::checklist(root.path(), &root.path().join("config"))
        .unwrap();
    assert!(items.iter().any(|item| !item.ready));
    assert!(items.iter().any(|item| item.label == "Provider routes"));
}
