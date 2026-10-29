//! The assessment path reads saved evidence without locking for writes or generating media.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]
use retold_core::Usd;
use retold_engine::{
    fsx,
    guide_assessment::assess,
    run::{self, NewRun},
    store::Store,
};
use retold_guide::{Guide, Origin};
use retold_parallax::{
    dibr::Audit,
    render::{Fallback, Rendered},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn fixture(root: &Path, title: &str, fallbacks: &[Fallback]) -> PathBuf {
    let mut files = retold_guide::catalog::bundled()
        .unwrap()
        .remove(0)
        .files()
        .clone();
    files.get_mut("guide.toml").unwrap().push_str(
        "\n[visuals.parallax]\nenabled=true\nmax_share=1.0\nstrength=0.55\nhero_fill=false\n",
    );
    let guide = Guide::from_files(Origin::Inline, files).unwrap();
    let channel =
        retold_guide::channel::init_from_template("test-studio", &root.join("config")).unwrap();
    let raw = [
        ("title".into(), title.into()),
        ("kind".into(), "film".into()),
    ]
    .into_iter()
    .collect();
    let (layout, _) = run::create(
        &root.join("library"),
        NewRun {
            guide: &guide,
            channel: &channel,
            inputs: retold_guide::inputs::resolve(guide.manifest(), &raw).unwrap(),
            variants: vec!["en".parse().unwrap()],
            quality: retold_engine::Quality::default(),
            budget: Usd::ZERO,
            skip_reviews: true,
            source_file: None,
        },
    )
    .unwrap();
    let mut store = Store::open(&layout).unwrap();
    for (i, fallback) in fallbacks.iter().enumerate() {
        let value = Rendered {
            key: format!("shot-{i}"),
            path: layout.work().join("unused.mov"),
            clip_hash: "not-needed-for-audit".into(),
            frame_hashes: vec![],
            fallback: *fallback,
            applied_strength: match fallback {
                Fallback::Requested => 0.55,
                Fallback::Gentler => 0.275,
                Fallback::Flat => 0.0,
            },
            audit: Audit {
                framing_scale: 1.01,
                max_stretch: 1.0,
                ..Audit::default()
            },
            rejected: vec![],
        };
        let step = format!("shots-{i}-en");
        store
            .commit(
                &step,
                &retold_engine::store::hash(&i).unwrap(),
                &value,
                None,
            )
            .unwrap();
        fsx::write_json_atomic(
            &layout
                .work()
                .join("parallax-shots")
                .join(format!("{step}.audit.json")),
            &value,
        )
        .unwrap();
    }
    store.finish().unwrap();
    drop(store);
    layout.video_dir().to_owned()
}
fn hashes(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(root).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            out.extend(hashes(&e.path()));
        } else {
            out.insert(
                e.path(),
                blake3::hash(&std::fs::read(e.path()).unwrap())
                    .to_hex()
                    .to_string(),
            );
        }
    }
    out
}
const RECOVERY: &[Fallback] = &[Fallback::Requested, Fallback::Gentler, Fallback::Flat];
#[test]
fn a_small_scene_reports_evidence_and_unknowns_without_mutating_any_file() {
    let root = tempfile::tempdir().unwrap();
    let run = fixture(root.path(), "one", RECOVERY);
    let checks = run.join("work/02-script/checks-0.json");
    fsx::write_json_atomic(
        &checks,
        &serde_json::json!({"gates":[{"gate":"starts","message":"Repeated sentence opening"}]}),
    )
    .unwrap();
    let before = hashes(root.path());
    let report = assess(std::slice::from_ref(&run)).unwrap();
    assert!(report.proposals.is_empty());
    assert_eq!(report.runs[0].observations.len(), 4);
    assert!(
        report.runs[0]
            .observations
            .iter()
            .filter(|o| o.kind == "parallax")
            .all(|o| o.status == "verified")
    );
    assert!(
        report.runs[0]
            .unavailable
            .iter()
            .any(|s| s.contains("timing is unknown"))
    );
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        serde_json::to_value(assess(&[run]).unwrap()).unwrap()
    );
    assert_eq!(before, hashes(root.path()));
}
#[test]
fn recurring_independent_evidence_yields_a_validated_candidate_while_mixed_settings_fail() {
    let root = tempfile::tempdir().unwrap();
    let mut runs = vec![];
    for i in 0..3 {
        runs.push(fixture(
            &root.path().join(i.to_string()),
            &format!("film {i}"),
            RECOVERY,
        ));
    }
    let report = assess(&runs).unwrap();
    assert_eq!(report.proposals.len(), 1);
    let proposal = &report.proposals[0];
    assert_eq!(proposal.independent_runs, 3);
    assert_eq!(proposal.after, 0.275);
    assert!(proposal.diff.contains("-strength=0.55"));
    assert!(proposal.diff.contains("+strength = 0.275"));
    let path = runs[0].join(".retold/channel.toml");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.push(b'\n');
    std::fs::write(&path, bytes).unwrap();
    assert!(assess(&runs).unwrap().proposals.is_empty());
}
#[test]
fn repeated_inputs_and_duplicate_run_copies_never_inflate_the_sample() {
    let root = tempfile::tempdir().unwrap();
    let runs: Vec<_> = (0..3)
        .map(|i| fixture(&root.path().join(i.to_string()), "same input", RECOVERY))
        .collect();
    assert!(assess(&runs).unwrap().proposals.is_empty());
    assert!(assess(&[runs[0].clone(), runs[0].clone()]).is_err());
    let a = runs[0].join(".retold/run.json");
    let b = runs[1].join(".retold/run.json");
    let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&std::fs::read(a).unwrap()).unwrap();
    value["id"] = original["id"].clone();
    fsx::write_json_atomic(&b, &value).unwrap();
    assert!(assess(&runs).is_err());
}
#[test]
fn stale_audits_and_altered_snapshots_cannot_justify_changes() {
    let root = tempfile::tempdir().unwrap();
    let run = fixture(root.path(), "one", RECOVERY);
    let path = run.join("work/parallax-shots/shots-1-en.audit.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["key"] = "stale".into();
    fsx::write_json_atomic(&path, &value).unwrap();
    let report = assess(std::slice::from_ref(&run)).unwrap();
    assert!(
        report.runs[0]
            .observations
            .iter()
            .any(|o| o.status == "unverified_or_legacy")
    );
    let path = run.join(".retold/guide/guide.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push('\n');
    std::fs::write(path, text).unwrap();
    assert!(assess(&[run]).is_err());
}
#[test]
fn busy_or_unfinished_runs_fail_without_recovery_writes() {
    let root = tempfile::tempdir().unwrap();
    let run = fixture(root.path(), "one", RECOVERY);
    let lock = std::fs::File::open(run.join(".retold/lock")).unwrap();
    lock.try_lock().unwrap();
    assert!(assess(std::slice::from_ref(&run)).is_err());
    drop(lock);
    let state = run.join(".retold/state.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
    value["active"] = "shots-0-en".into();
    fsx::write_json_atomic(&state, &value).unwrap();
    let before = hashes(root.path());
    assert!(assess(&[run]).is_err());
    assert_eq!(before, hashes(root.path()));
}

#[test]
fn timing_is_language_specific_and_stays_an_observation_not_a_target_change() {
    use retold_engine::{
        languages::{Edition, Packaging},
        media_runtime::Asset,
    };
    use retold_providers::media::{MediaUsage, Quote};
    let root = tempfile::tempdir().unwrap();
    let run = fixture(root.path(), "timing", &[]);
    let guide = Guide::from_dir(&run.join(".retold/guide")).unwrap();
    let m = guide.manifest();
    let lang: retold_core::LanguageTag = "en".parse().unwrap();
    let range = m
        .length
        .get(&lang)
        .and_then(|r| r.seconds)
        .unwrap_or(m.format.target_seconds);
    let duration = f64::midpoint(range[0], range[1]) - m.edit.ending.seconds;
    let edition = Edition {
        variant: "en".parse().unwrap(),
        units: vec![],
        audio: Asset {
            path: run.join("unused.wav"),
            content_hash: "fixture".into(),
            provider: "fixture".into(),
            model: "fixture".into(),
            usage: MediaUsage {
                generation: None,
                cost: Some(Usd::ZERO),
            },
            quote: Quote {
                ceiling: Usd::ZERO,
                provider_tag: None,
            },
            request_id: "fixture".into(),
        },
        alignment: retold_lang::Alignment {
            duration,
            units: vec![],
            matched: 1.0,
        },
        captions: vec![],
        sidecars: vec![],
        packaging: Packaging { posts: vec![] },
    };
    fsx::write_json_atomic(&run.join("work/07-captions/editions.json"), &vec![edition]).unwrap();
    let report = assess(&[run]).unwrap();
    let row = report.runs[0]
        .observations
        .iter()
        .find(|o| o.kind == "timing")
        .unwrap();
    assert_eq!(row.subject, "en:en");
    assert_eq!(row.status, "review_only");
    assert_eq!(row.detail["within_target"], true);
    assert!(report.proposals.is_empty());
}

#[test]
fn clean_runs_are_considered_instead_of_cherry_picking_recovery_failures() {
    let root = tempfile::tempdir().unwrap();
    let runs: Vec<_> = (0..7)
        .map(|i| {
            fixture(
                &root.path().join(i.to_string()),
                &format!("film {i}"),
                if i < 3 {
                    RECOVERY
                } else {
                    &[Fallback::Requested, Fallback::Requested]
                },
            )
        })
        .collect();
    assert!(assess(&runs).unwrap().proposals.is_empty());
}
