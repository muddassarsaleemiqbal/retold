//! Isolated history fixtures and observer processes; the fake owner never starts a worker.
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use retold_app::protocol::{Outcome, Payload};
use retold_app::queue::{JobSpec, JobStatus, QueueStore, TransitionSpec};
use retold_app::studio::{ProgressEvent, StudioServer};
use serde_json::Value;

pub const WAIT_LIMIT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(10);
const NOW: &str = "2026-10-20T00:00:00Z";

#[derive(Debug)]
pub struct Fixture {
    root: tempfile::TempDir,
    owner: Option<StudioServer>,
    pub queue: QueueStore,
    pub id: String,
}

impl Fixture {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let owner = StudioServer::bind(root.path()).unwrap();
        let queue = owner.queue().clone();
        let id = running_job(&queue);
        Self {
            root,
            owner: Some(owner),
            queue,
            id,
        }
    }

    pub fn progress(&self, count: usize) {
        for index in 0..count {
            self.queue
                .record_event(progress(&self.id, &format!("Saved {index}")))
                .unwrap();
        }
    }

    pub fn large_progress(&self, count: usize) {
        const MESSAGE_BYTES: usize = 1000;
        for _ in 0..count {
            self.queue
                .record_event(progress(&self.id, &"x".repeat(MESSAGE_BYTES)))
                .unwrap();
        }
    }

    pub fn completed(self) -> Self {
        self.queue.finish(&self.id, &Payload::Result { outcome: Outcome::Completed,
            contents: serde_json::json!({"text": "Stored outcome fixture", "proof": "durable-fixture"}) },
            progress(&self.id, "Saved outcome")).unwrap();
        self
    }

    pub fn large_outcome(self) -> Self {
        const OUTCOME_BYTES: usize = 200_000;
        self.queue
            .finish(
                &self.id,
                &Payload::Result {
                    outcome: Outcome::Completed,
                    contents: serde_json::json!({"proof": "x".repeat(OUTCOME_BYTES)}),
                },
                progress(&self.id, "Saved large outcome"),
            )
            .unwrap();
        self
    }

    pub fn offline(mut self) -> Self {
        self.owner.take();
        self
    }

    pub fn command(&self, extra: &[&str]) -> Command {
        let mut command = self.isolated_command();
        command.args(extra).args(["queue", "attach", &self.id]);
        command
    }

    fn isolated_command(&self) -> Command {
        let command = Command::new(assert_cmd::cargo::cargo_bin("retold"));
        isolated_environment(self.root.path(), command)
    }

    pub fn spawn(&self) -> Observer {
        let mut command = self.command(&["--json"]);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = command.spawn().unwrap();
        Observer { child }
    }

    pub fn status(&self) -> JobStatus {
        let found = self.queue.find(&self.id).unwrap();
        let job = found.unwrap();
        job.status
    }
}

fn isolated_environment(root: &std::path::Path, mut command: Command) -> Command {
    let folders = [
        ("HOME", "home"),
        ("RETOLD_CONFIG_DIR", "config"),
        ("RETOLD_LIBRARY", "library"),
    ];
    command.envs(folders.map(|(key, folder)| (key, root.join(folder))));
    command.env("RETOLD_STATE", root).env("PATH", "");
    command
        .env_remove("RETOLD_WORKSPACE")
        .env_remove("XDG_DATA_HOME");
    command
}

fn running_job(queue: &QueueStore) -> String {
    let job = queue
        .enqueue(
            JobSpec {
                idempotency_key: "observed",
                video_ref: "video",
                plan_hash: "hash",
            },
            NOW,
        )
        .unwrap();
    queue
        .transition(
            TransitionSpec {
                id: &job.id,
                target: JobStatus::Running,
                error: Option::default(),
            },
            NOW,
        )
        .unwrap();
    job.id
}

fn progress(id: &str, message: &str) -> ProgressEvent {
    ProgressEvent {
        sequence: 0,
        job_id: id.into(),
        timestamp: NOW.into(),
        stage: "research".into(),
        step: Option::default(),
        message: message.into(),
        is_terminal: false,
    }
}

#[derive(Debug)]
pub struct Observer {
    child: Child,
}

impl Observer {
    pub fn first_line(mut self) -> (Self, Pipe) {
        let stdout = self.child.stdout.take().unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let _ = send.send(Pipe { reader, first });
        });
        let pipe = receive.recv_timeout(WAIT_LIMIT).unwrap();
        assert!(!pipe.first.is_empty(), "observer exited before progress");
        (self, pipe)
    }

    pub fn interrupt(&self) {
        let status = Command::new("/bin/kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .unwrap();
        assert!(status.success());
    }

    pub fn close_reader(mut self) -> Self {
        self.child.stdout.take();
        self
    }

    pub fn wait(mut self) -> ExitStatus {
        let started = Instant::now();
        loop {
            let status = self.child.try_wait().unwrap();
            if let Some(status) = status {
                return status;
            }
            let timely = started.elapsed() < WAIT_LIMIT;
            assert!(timely, "observer did not detach promptly");
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug)]
pub struct Pipe {
    reader: BufReader<ChildStdout>,
    pub first: String,
}

impl Pipe {
    pub fn remaining(mut self) -> Vec<Value> {
        let mut rest = String::new();
        self.reader.read_to_string(&mut rest).unwrap();
        parse_lines(&format!("{}{rest}", self.first))
    }
}

pub fn parse_lines(text: &str) -> Vec<Value> {
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
