//! Two background lanes keep reads responsive while one action runs. Commands share the CLI's
//! in-process dispatcher; refreshing never creates additional threads.
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use retold_app::protocol::{Diagnostic, ErrorCode, Payload};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Job {
    Query(Vec<String>),
    Action { id: u64, words: Vec<String> },
    Local { id: u64, task: super::files::Task },
}

#[derive(Debug)]
pub struct Reply {
    pub job: Job,
    pub payload: Payload,
}

type Dispatch = Arc<dyn Executor>;

trait Executor: Send + Sync {
    fn request(&self, words: Vec<String>) -> Payload;
    fn local(&self, task: &super::files::Task) -> Payload {
        super::files::run_task(task)
    }
}

impl<F: Fn(Vec<String>) -> Payload + Send + Sync> Executor for F {
    fn request(&self, words: Vec<String>) -> Payload {
        self(words)
    }
}

impl Executor for super::scope::Scope {
    fn request(&self, words: Vec<String>) -> Payload {
        self.request(words)
    }
    fn local(&self, task: &super::files::Task) -> Payload {
        self.local(task)
    }
}

pub struct Worker {
    queries: Sender<Job>,
    actions: Sender<Job>,
    receiver: Receiver<Reply>,
}

impl Worker {
    #[must_use]
    pub fn new(prefix: Vec<String>, library: Option<PathBuf>) -> Self {
        let scope = Arc::new(super::scope::Scope::new(library));
        let dispatch = Arc::clone(&scope);
        Self::start(prefix, dispatch, move || scope.prepare())
    }

    fn start(
        prefix: Vec<String>,
        dispatch: Dispatch,
        prepare: impl FnOnce() + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (queries, reads) = mpsc::channel();
        let (actions, writes) = mpsc::channel();
        let reads_lane = Lane {
            replies: sender.clone(),
            prefix: prefix.clone(),
            dispatch: Arc::clone(&dispatch),
        };
        let writes_lane = Lane {
            replies: sender,
            prefix,
            dispatch,
        };
        std::thread::spawn(move || {
            prepare();
            reads_lane.work(reads);
        });
        std::thread::spawn(move || writes_lane.work(writes));
        Self {
            queries,
            actions,
            receiver,
        }
    }

    pub fn submit(&self, job: Job) {
        let sender = match job {
            Job::Query(_)
            | Job::Local {
                task:
                    super::files::Task::LoadDefaults(_)
                    | super::files::Task::Browse(_)
                    | super::files::Task::Preview(_)
                    | super::files::Task::Resource { .. },
                ..
            } => &self.queries,
            Job::Action { .. } | Job::Local { .. } => &self.actions,
        };
        sender.send(job).ok();
    }

    #[must_use]
    pub fn ready(&self) -> Vec<Reply> {
        self.receiver.try_iter().collect()
    }
}

struct Lane {
    replies: Sender<Reply>,
    prefix: Vec<String>,
    dispatch: Dispatch,
}

impl Lane {
    fn work(self, jobs: Receiver<Job>) {
        for job in jobs {
            let command = command(&self.prefix, &job);
            let result = super::panic_guard::catch(|| match &job {
                Job::Local { task, .. } => self.dispatch.local(task),
                _ => self.dispatch.request(command),
            });
            let payload = result.unwrap_or_else(|_| {
                Diagnostic::new(
                    ErrorCode::ExecutionFailed,
                    "The action failed unexpectedly. Inspect saved state before retrying.",
                )
                .into_payload()
            });
            let disconnected = self.replies.send(Reply { job, payload }).is_err();
            if disconnected {
                break;
            }
        }
    }
}

fn command(prefix: &[String], job: &Job) -> Vec<String> {
    let words = match job {
        Job::Query(words) | Job::Action { words, .. } => words,
        Job::Local { .. } => return Vec::new(),
    };
    std::iter::once("retold".to_owned())
        .chain(prefix.iter().cloned())
        .chain(words.iter().cloned())
        .collect()
}

#[cfg(test)]
#[path = "tests/runtime.rs"]
mod tests;
