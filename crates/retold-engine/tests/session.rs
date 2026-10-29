//! Sessions run saved videos only within the reviewed snapshot and never past `ready`.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared support module"
)]
mod support {
    include!("support/mod.rs");
}

use std::collections::BTreeSet;
use std::sync::Arc;

use retold_core::Usd;
use retold_engine::gate::{GateError, PaidCall, PaidEvidence, PaidGate};
use retold_engine::pipeline::Until;
use retold_engine::session::{self, Expected, Providers, Selection, SessionError, StopKind};
use retold_engine::store::{hash, inspection};
use retold_engine::{Paths, RunLayout};

const NO_PROVIDERS: &str = "schema_version = 1\n[providers]\n[routing]\n";
const MISSING_EDITOR: &str = "/definitely/not/a/real/editor";

#[derive(Debug)]
struct FailedJournal;

impl retold_engine::events::EventObserver for FailedJournal {
    fn observe(&self, _: &retold_engine::events::Event) -> Result<(), retold_engine::EngineError> {
        Err(retold_engine::EngineError::Setup(
            "fixture journal is unavailable".into(),
        ))
    }
}

#[test]
fn cancelled_session_preserves_the_reviewed_snapshot_and_dispatches_nothing() {
    let (_sandbox, context, text, _) = support::setup(&["en"], Usd::ZERO);
    let layout = context.layout.clone();
    drop(context);
    let settings = installation(&layout);
    let before = current(&layout);
    let control = session::Control::default();
    control.cancel();
    let request = session::Request {
        control: Some(&control),
        until: Until::Research,
        ..request(&layout, &settings, &before)
    };
    assert_eq!(
        session::run_until(&request).unwrap_err().kind(),
        StopKind::Cancelled
    );
    assert_eq!(current(&layout).state_hash, before.state_hash);
    assert_eq!(text.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn journal_failure_stops_the_session_before_research_dispatch() {
    let (_sandbox, context, text, _) = support::setup(&["en"], Usd::ZERO);
    let layout = context.layout.clone();
    drop(context);
    let settings = installation(&layout);
    let expected = current(&layout);
    let control = session::Control::default().with_observer(Arc::new(FailedJournal));
    let request = session::Request {
        control: Some(&control),
        until: Until::Research,
        ..request(&layout, &settings, &expected)
    };
    let error = session::run_until(&request).unwrap_err();
    assert!(error.to_string().contains("fixture journal is unavailable"));
    assert_eq!(text.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(
        !layout
            .stage(retold_engine::Stage::Research)
            .join("source.json")
            .exists()
    );
}

/// Refuses everything; these sessions must never reach a paid call.
#[derive(Debug)]
struct Closed;

fn closed() -> GateError {
    GateError::Blocked("closed fixture gate".into())
}

impl PaidGate for Closed {
    fn reserve(&self, _: &PaidCall<'_>) -> Result<(), GateError> {
        Err(closed())
    }
    fn dispatch(&self, _: &str) -> Result<(), GateError> {
        Err(closed())
    }
    fn withdraw(&self, _: &str) -> Result<(), GateError> {
        Err(closed())
    }
    fn settle(&self, _: &str, _: &PaidEvidence<'_>) -> Result<(), GateError> {
        Err(closed())
    }
    fn reconcile(&self, _: &str, _: Usd) -> Result<(), GateError> {
        Err(closed())
    }
}

fn stages(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|&name| name.to_owned()).collect()
}

#[test]
fn stops_declare_exactly_their_stages_and_prerequisites() {
    assert!(session::stages_through(Until::Created).is_empty());
    let script = session::stages_through(Until::Script);
    assert_eq!(script, stages(&["research", "script"]));
    let voice = session::stages_through(Until::Voice);
    assert!(voice.contains("characters") && !voice.contains("captions"));
    let ready = session::stages_through(Until::Ready);
    assert_eq!(
        ready.len(),
        retold_engine::graph::Graph::stages().dependencies.len()
    );
}

fn current(layout: &RunLayout) -> Expected {
    let manifest = std::fs::read(layout.run_file()).unwrap();
    Expected {
        manifest_hash: hash(&manifest).unwrap(),
        state_hash: inspection::inspect(layout).unwrap().state_hash,
        source_hash: retold_engine::research::saved_source_hash(layout).unwrap(),
        editing_hash: retold_engine::editing::digest(layout).unwrap(),
    }
}

/// Installation settings with no provider; nothing can be dispatched.
fn installation(layout: &RunLayout) -> (Paths, Providers) {
    let config = layout.video_dir().join("installation");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("providers.toml"), NO_PROVIDERS).unwrap();
    let paths = Paths {
        config,
        library: layout.video_dir().into(),
    };
    let providers = Providers::load(&paths).unwrap();
    (paths, providers)
}

fn request<'a>(
    layout: &'a RunLayout,
    settings: &'a (Paths, Providers),
    expected: &'a Expected,
) -> session::Request<'a> {
    session::Request {
        paths: &settings.0,
        layout,
        providers: &settings.1,
        gate: Arc::new(Closed),
        allow_paid: false,
        until: Until::Ready,
        expected,
        editor: Some(MISSING_EDITOR.into()),
        control: None,
    }
}

fn attempt(layout: &RunLayout, until: Until, expected: &Expected) -> SessionError {
    let settings = installation(layout);
    let stop = session::Request {
        until,
        ..request(layout, &settings, expected)
    };
    session::run_until(&stop).unwrap_err()
}

#[test]
fn sessions_refuse_exports_and_changed_runs_before_running_anything() {
    let (_sandbox, context, text, _) = support::setup(&["en"], Usd::ZERO);
    let layout = &context.layout.clone();
    // Release the fixture's run lock; a session must take it itself.
    drop(context);
    let expected = current(layout);
    let beyond = attempt(layout, Until::Export, &expected);
    assert!(matches!(beyond, SessionError::BeyondReady));
    assert_eq!(beyond.kind(), StopKind::Authority);
    let stale = Expected {
        state_hash: hash(&"an older state").unwrap(),
        ..expected
    };
    let changed = attempt(layout, Until::Script, &stale);
    assert_eq!(changed.kind(), StopKind::Changed);
    assert_eq!(text.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn reviewed_sessions_reject_manual_edits_even_if_the_state_index_was_not_published() {
    let (_sandbox, context, text, _) = support::setup(&["en"], Usd::ZERO);
    let layout = context.layout.clone();
    drop(context);
    seed_script(&layout);
    let expected = current(&layout);
    let state = std::fs::read(layout.state_file()).unwrap();
    let snapshot = retold_engine::editing::script(&layout).unwrap().unwrap();
    let edit = retold_engine::editing::SectionEdit {
        expected_version: &snapshot.version,
        section: "hook",
        text: "A saved manual edit.",
    };
    retold_engine::editing::save_section(&layout, edit).unwrap();
    // Simulate a crash between authoritative pointer publication and derived index publication.
    std::fs::write(layout.state_file(), state).unwrap();
    assert_eq!(current(&layout).state_hash, expected.state_hash);
    assert_eq!(
        attempt(&layout, Until::Script, &expected).kind(),
        StopKind::Changed
    );
    assert_eq!(text.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

fn seed_script(layout: &RunLayout) {
    let script: retold_engine::story::Script =
        serde_json::from_str(include_str!("fixtures/full/script.json")).unwrap();
    let mut store = retold_engine::store::Store::open(layout).unwrap();
    store
        .commit(
            "script",
            &hash(&script).unwrap(),
            &script,
            Option::default(),
        )
        .unwrap();
    retold_engine::fsx::write_json_atomic(
        &layout
            .stage(retold_engine::Stage::Script)
            .join("script.json"),
        &script,
    )
    .unwrap();
}

#[test]
fn a_current_session_reports_manual_approval_as_review_before_dispatch() {
    let (_sandbox, context, text, _) = support::setup(&["en"], Usd::ZERO);
    let layout = context.layout.clone();
    drop(context);
    seed_script(&layout);
    let snapshot = retold_engine::editing::script(&layout).unwrap().unwrap();
    retold_engine::editing::save_section(
        &layout,
        retold_engine::editing::SectionEdit {
            expected_version: &snapshot.version,
            section: "hook",
            text: "A draft waiting for review.",
        },
    )
    .unwrap();
    let expected = current(&layout);
    assert_eq!(
        attempt(&layout, Until::Script, &expected).kind(),
        StopKind::Review
    );
    assert_eq!(text.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

/// Runs the fake workflow to ready and releases the run lock.
fn ready_video() -> (tempfile::TempDir, RunLayout) {
    let (sandbox, context, _, _) = support::setup(&["en"], Usd::ZERO);
    let options = retold_engine::workflow::Options {
        fonts: Some(Arc::new(support::EstimatedFonts)),
        ..retold_engine::workflow::Options::default()
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let work = retold_engine::workflow::run(Arc::clone(&context), Until::Ready, &options);
    runtime.block_on(work).unwrap();
    let layout = context.layout.clone();
    drop(context);
    (sandbox, layout)
}

fn selection(edition: &str, format: &str) -> Selection {
    Selection {
        editions: BTreeSet::from([edition.to_owned()]),
        formats: BTreeSet::from([format.to_owned()]),
        project_only: true,
    }
}

#[test]
fn exports_reach_the_editor_only_for_saved_ready_deliveries() {
    let (_sandbox, layout) = ready_video();
    let prepared = session::prepared_deliveries(&layout).unwrap();
    let first = prepared.first().unwrap().clone();
    let settings = installation(&layout);
    let expected = current(&layout);
    let export = request(&layout, &settings, &expected);
    let unknown = session::export(&export, &selection("zz", &first.format)).unwrap_err();
    assert_eq!(unknown.kind(), StopKind::NotReady);
    let missing = session::export(&export, &selection(&first.edition, "21x9")).unwrap_err();
    assert_eq!(missing.kind(), StopKind::NotReady);
    let expected = current(&layout);
    let export = request(&layout, &settings, &expected);
    let launched = session::export(&export, &selection(&first.edition, &first.format));
    let launched = launched.unwrap_err();
    assert_eq!(launched.kind(), StopKind::Failed, "{launched}");
    assert!(launched.to_string().contains("FilmCraft"), "{launched}");
}

#[test]
fn unfinished_videos_have_no_prepared_deliveries() {
    let (_sandbox, context, _, _) = support::setup(&["en"], Usd::ZERO);
    let prepared = session::prepared_deliveries(&context.layout).unwrap();
    assert_eq!(prepared, Vec::new());
}
