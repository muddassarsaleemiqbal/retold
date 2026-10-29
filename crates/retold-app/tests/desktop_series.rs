//! Native series projections stay faithful to the shared registry and planner.
#![expect(clippy::unwrap_used, reason = "isolated fixture setup and assertions")]

use retold_app::desktop::series;
use retold_app::series::{SeriesSpec, new_series};

const PARTS: usize = 3;
const FORGED_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn created() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let spec = SeriesSpec {
        guide: "story-recap",
        work: "Three Lanterns",
        kind: "film",
        parts_count: PARTS,
        ..SeriesSpec::default()
    };
    new_series(root.path(), &spec).unwrap();
    root
}

#[test]
fn a_new_series_lists_and_shows_three_unlinked_parts() {
    let root = created();
    let listed = series::list(root.path()).unwrap();
    assert_eq!(listed.len(), 1);
    let matrix = series::matrix(root.path(), &listed[0].id).unwrap();
    assert_eq!(matrix.manifest.parts.len(), PARTS);
    assert!(matrix.progress.values().all(Option::is_none));
}

#[test]
fn unlinked_parts_block_the_plan_and_a_run_without_its_reviewed_hash_queues_nothing() {
    let root = created();
    let id = series::list(root.path()).unwrap().remove(0).id;
    let plan = series::plan(root.path(), &id, (Some("P01,P02"), "0")).unwrap();
    assert!(!plan.can_execute);
    assert_eq!(plan.blockers.len(), 2);
    assert_eq!(plan.usd_micros(), 0);
    let mut changed = plan.clone();
    changed.plan_hash = FORGED_HASH.into();
    assert!(series::run(root.path(), &id, &changed).is_err());
    assert!(series::run(root.path(), &id, &plan).is_err());
    let duplicate = series::plan(root.path(), &id, (Some("P01,P01"), "0"));
    assert!(duplicate.is_err());
    assert!(!root.path().join(".retold/studio").exists());
}

#[test]
fn native_creation_registers_unlinked_parts_and_rejects_empty_or_excessive_requests() {
    let root = tempfile::tempdir().unwrap();
    let new = series::NewSeries {
        work: "Night Ferry".into(),
        guide: "story-recap".into(),
        channel: "default".into(),
        parts: PARTS,
    };
    let matrix = series::create(root.path(), &new).unwrap();
    assert_eq!(matrix.manifest.parts.len(), PARTS);
    let empty = series::NewSeries {
        work: " ".into(),
        ..new.clone()
    };
    assert!(series::create(root.path(), &empty).is_err());
    let excessive = series::NewSeries {
        parts: series::MAX_NEW_PARTS + 1,
        ..new
    };
    assert!(series::create(root.path(), &excessive).is_err());
}

#[test]
fn adding_parts_is_previewed_and_refused_after_another_change() {
    let root = created();
    let id = series::list(root.path()).unwrap().remove(0).id;
    let reviewed = series::review_add_parts(root.path(), &id, 2).unwrap();
    assert_eq!(reviewed.new_parts, vec!["P04".to_owned(), "P05".to_owned()]);
    assert_eq!(
        series::matrix(root.path(), &id)
            .unwrap()
            .manifest
            .parts
            .len(),
        PARTS
    );
    let updated = series::add_parts(root.path(), &id, &reviewed).unwrap();
    assert_eq!(updated.manifest.parts.len(), PARTS + 2);
    assert!(series::add_parts(root.path(), &id, &reviewed).is_err());
}
