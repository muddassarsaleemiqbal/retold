//! Background success requires real engine artifacts; plans, authority and replay survive owners.
#![expect(
    clippy::unwrap_used,
    reason = "isolated subprocess fixtures and assertions"
)]
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use retold_app::plan::PlanOptions;
use retold_app::queue::ListOptions;
use retold_app::queue::{ExecutionIntent, JobSpec};
use retold_app::queue::{JobStatus, QueueStore};
use serde_json::Value;
use std::process::Command;

const WAIT_LIMIT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const NO_PROVIDERS: &str = "schema_version = 1\n[providers]\n[routing]\n";

#[cfg(unix)]
#[path = "studio_execution/control.rs"]
mod control;

#[cfg(unix)]
#[path = "studio_execution/foreground.rs"]
mod foreground;

struct Installation {
    sandbox: tempfile::TempDir,
    root: PathBuf,
    video_id: String,
    run: PathBuf,
}

impl Installation {
    fn new() -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        let root = sandbox.path().join("workspace");
        let mut fixture = Self {
            sandbox,
            root,
            video_id: String::new(),
            run: PathBuf::new(),
        };
        fixture.succeed(&["workspace", "init", fixture.root.to_str().unwrap()]);
        fixture.install_resources();
        fixture.succeed(&["account", "init", "--timezone", "UTC"]);
        fixture.create_video();
        fixture
    }

    fn create_video(&mut self) {
        let source = self.sandbox.path().join("facts.txt");
        fs::write(&source, "Clouds consist of suspended water droplets.").unwrap();
        let created = self.succeed(&[
            "new",
            "explain",
            "--topic",
            "Clouds",
            "--source-file",
            source.to_str().unwrap(),
        ]);
        self.video_id = created["payload"]["data"]["created"]["video_id"]
            .as_str()
            .unwrap()
            .into();
        self.run = PathBuf::from(
            created["payload"]["data"]["created"]["directory"]
                .as_str()
                .unwrap(),
        );
    }

    fn install_resources(&self) {
        self.succeed(&["guides", "create", "explain", "--language", "en"]);
        self.succeed(&["channels", "create", "science", "--language", "en"]);
        fs::write(self.root.join("config/providers.toml"), NO_PROVIDERS).unwrap();
    }

    fn state(&self) -> PathBuf {
        self.sandbox.path().join("studio")
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(assert_cmd::cargo::cargo_bin("retold"));
        command.env("HOME", self.sandbox.path().join("home"));
        command.env(
            "RETOLD_CONFIG_DIR",
            self.sandbox.path().join("installation"),
        );
        command.env("RETOLD_STATE", self.state()).env("PATH", "");
        command = self.provider_environment(command);
        command
            .env_remove("RETOLD_WORKSPACE")
            .env_remove("RETOLD_LIBRARY")
            .env_remove("XDG_DATA_HOME");
        command
            .args(["--json", "--workspace"])
            .arg(&self.root)
            .args(args);
        command
    }

    fn invoke(&self, args: &[&str]) -> (Value, i32) {
        let output = self.command(args).output().unwrap();
        (
            serde_json::from_slice(&output.stdout).unwrap(),
            output.status.code().unwrap(),
        )
    }

    fn provider_environment(&self, mut command: Command) -> Command {
        for (key, filename) in [
            ("RETOLD_TEST_PROVIDER_PID", "provider.pid"),
            ("RETOLD_TEST_PROVIDER_STARTED", "provider.started"),
            ("RETOLD_TEST_PROVIDER_FINISHED", "provider.finished"),
        ] {
            command.env(key, self.sandbox.path().join(filename));
        }
        command
    }

    fn succeed(&self, args: &[&str]) -> Value {
        let (result, code) = self.invoke(args);
        assert_eq!(code, 0, "{args:?}: {result}");
        result
    }

    fn submit(&self, key: &str) -> String {
        let result = self.succeed(&[
            "video",
            "continue",
            &self.video_id,
            "--until",
            "research",
            "--yes",
            "--background",
            "--idempotency-key",
            key,
        ]);
        result["payload"]["data"]["job"]["id"]
            .as_str()
            .unwrap()
            .into()
    }

    fn wait(&self, id: &str) -> retold_app::queue::JobRecord {
        let queue = QueueStore::open_read_only(&self.state().join("queue.sqlite")).unwrap();
        let started = Instant::now();
        loop {
            let job = queue.find(id).unwrap().unwrap();
            let terminal = job.status.is_terminal();
            if terminal {
                return job;
            }
            assert!(
                started.elapsed() < WAIT_LIMIT,
                "job did not finish: {job:?}"
            );
            thread::sleep(POLL_INTERVAL);
        }
    }
}

impl Drop for Installation {
    fn drop(&mut self) {
        let _ = retold_app::studio::stop_studio(&self.state());
    }
}

#[test]
fn background_research_saves_real_artifacts_and_replays_after_restart() {
    let fixture = Installation::new();
    fixture.succeed(&["studio", "start"]);
    let id = fixture.submit("research");
    let finished = fixture.wait(&id);
    assert_eq!(finished.status, JobStatus::Completed, "{finished:?}");
    assert_source(&fixture.run);
    let queue = QueueStore::open_read_only(&fixture.state().join("queue.sqlite")).unwrap();
    let events = queue.events(&id, 0).unwrap();
    assert!(events.iter().any(|event| event.stage == "research"));
    assert!(events.last().unwrap().is_terminal);
    assert!(queue.result(&id).unwrap().is_some());
    assert_saved_details(&fixture, &id);
    fixture.succeed(&["studio", "stop"]);
    fixture.succeed(&["studio", "start"]);
    let replay = fixture.succeed(&["queue", "attach", &id, "--once"]);
    assert_eq!(
        replay["payload"]["data"]["events"]
            .as_array()
            .unwrap()
            .len(),
        events.len()
    );
    assert_eq!(replay["payload"]["data"]["is_terminal"], true);
}

fn assert_saved_details(fixture: &Installation, id: &str) {
    let shown = fixture.succeed(&["queue", "show", id]);
    let details = &shown["payload"]["data"];
    assert_eq!(details["job"]["status"], "completed");
    assert_eq!(details["result"]["type"], "result");
    assert_eq!(details["result"]["payload"]["outcome"], "completed");
    assert_eq!(details["execution"]["plan"]["until"], "research");
    assert_eq!(
        details["result"]["payload"]["data"]["continued"]["job_id"],
        id
    );
}

#[test]
fn quiet_live_attachment_returns_the_actual_background_engine_outcome() {
    let fixture = Installation::new();
    fixture.succeed(&["studio", "start"]);
    let id = fixture.submit("live-attachment");
    let finished = fixture.succeed(&["--quiet", "queue", "attach", &id]);
    assert_eq!(finished["payload"]["outcome"], "completed");
    let outcome = &finished["payload"]["data"];
    assert_eq!(outcome["continued"]["job_id"], id);
    assert_eq!(outcome["continued"]["until"], "research");
    assert_eq!(outcome["attachment"]["status"], "completed");
    assert_source(&fixture.run);
}

fn assert_source(run: &Path) {
    let source: Value =
        serde_json::from_slice(&fs::read(run.join("work/01-research/source.json")).unwrap())
            .unwrap();
    assert!(
        source["text"]
            .as_str()
            .unwrap()
            .contains("suspended water droplets")
    );
    let state: Value =
        serde_json::from_slice(&fs::read(run.join(".retold/state.json")).unwrap()).unwrap();
    assert!(state["completed"]["research"].is_object());
    assert!(run.join(".retold/events.log").is_file());
    assert!(!run.join("deliverables/en/video.mp4").exists());
}

#[test]
fn submission_requires_a_studio_and_stale_confirmation_cannot_enqueue() {
    let fixture = Installation::new();
    let (offline, code) = fixture.invoke(&[
        "video",
        "continue",
        &fixture.video_id,
        "--until",
        "research",
        "--yes",
        "--background",
    ]);
    assert_eq!(code, 3, "{offline}");
    assert_eq!(offline["payload"]["code"], "RG-STUDIO-REQUIRED");
    assert!(!fixture.state().join("queue.sqlite").exists());
    fixture.succeed(&["studio", "start"]);
    let (stale, code) = fixture.invoke(&[
        "video",
        "continue",
        &fixture.video_id,
        "--until",
        "research",
        "--plan-hash",
        "stale",
        "--background",
    ]);
    assert_eq!(code, 3, "{stale}");
    assert_eq!(stale["payload"]["code"], "RG-PLAN-CHANGED");
    assert_empty_queue(&fixture);
}

fn assert_empty_queue(fixture: &Installation) {
    let queue = QueueStore::open_read_only(&fixture.state().join("queue.sqlite")).unwrap();
    let page = queue.list_page(&ListOptions::default()).unwrap();
    assert_eq!(page.total, 0);
}

#[test]
fn background_missing_provider_is_a_real_failure_and_preserves_research() {
    let fixture = Installation::new();
    fixture.succeed(&["studio", "start"]);
    let result = fixture.succeed(&[
        "video",
        "continue",
        &fixture.video_id,
        "--until",
        "script",
        "--yes",
        "--background",
    ]);
    let id = result["payload"]["data"]["job"]["id"].as_str().unwrap();
    let finished = fixture.wait(id);
    assert_eq!(finished.status, JobStatus::Failed, "{finished:?}");
    assert_failed_attachment(&fixture, id);
    assert_source(&fixture.run);
    assert!(!fixture.run.join("work/02-script/script.json").exists());
}

fn assert_failed_attachment(fixture: &Installation, id: &str) {
    let (reported, code) = fixture.invoke(&["--quiet", "queue", "attach", id]);
    assert_eq!(code, 4, "{reported}");
    assert_eq!(reported["type"], "error");
    assert_eq!(
        reported["payload"]["context"]["attachment"]["status"],
        "failed"
    );
    assert_eq!(reported["payload"]["context"]["attachment"]["job_id"], id);
}

#[test]
fn dequeue_rejects_a_source_changed_since_the_reviewed_plan() {
    let fixture = Installation::new();
    let job = reviewed_research_job(&fixture, "stale-source");
    fs::write(
        fixture.run.join("work/01-research/source-file.txt"),
        "A changed source.",
    )
    .unwrap();
    fixture.succeed(&["studio", "start"]);
    let finished = fixture.wait(&job.id);
    assert_eq!(finished.status, JobStatus::Blocked, "{finished:?}");
    assert_eq!(finished.error_code.as_deref(), Some("RG-PLAN-CHANGED"));
    assert!(!fixture.run.join("work/01-research/source.json").exists());
}

fn reviewed_research_job(fixture: &Installation, key: &str) -> retold_app::queue::JobRecord {
    let planned = fixture.succeed(&["video", "plan", &fixture.video_id, "--until", "research"]);
    let hash = planned["payload"]["data"]["plan_hash"].as_str().unwrap();
    let queue = QueueStore::open(&fixture.state().join("queue.sqlite")).unwrap();
    let intent = ExecutionIntent {
        library: fixture.root.join("library").canonicalize().unwrap(),
        plan: PlanOptions {
            until: Some("research".into()),
            ..PlanOptions::default()
        },
    };
    let spec = JobSpec {
        idempotency_key: key,
        video_ref: &fixture.video_id,
        plan_hash: hash,
    };
    queue
        .enqueue_executable(spec, &intent, "2026-10-20T00:00:00Z")
        .unwrap()
}

#[test]
fn idempotent_submission_returns_the_original_job_after_progress_and_refuses_changed_requests() {
    let fixture = Installation::new();
    fixture.succeed(&["studio", "start"]);
    let original = fixture.submit("stable-key");
    assert_eq!(fixture.wait(&original).status, JobStatus::Completed);
    let retried = fixture.submit("stable-key");
    assert_eq!(retried, original);
    let (changed, code) = fixture.invoke(&[
        "video",
        "continue",
        &fixture.video_id,
        "--until",
        "script",
        "--yes",
        "--background",
        "--idempotency-key",
        "stable-key",
    ]);
    assert_eq!(code, 2, "{changed}");
    let queue = QueueStore::open_read_only(&fixture.state().join("queue.sqlite")).unwrap();
    assert_eq!(
        queue.list_page(&ListOptions::default()).unwrap().jobs.len(),
        1
    );
    assert!(!fixture.run.join("work/02-script/script.json").exists());
}
