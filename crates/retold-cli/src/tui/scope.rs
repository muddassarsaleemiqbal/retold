//! Pin session paths once without changing process environment or the shared account root.
use super::files::{self, Task};
use retold_app::{
    protocol::{Diagnostic, ErrorCode, Payload},
    workspace::context::Context,
};
use std::{path::PathBuf, sync::OnceLock};

pub(super) struct Scope {
    root: Option<PathBuf>,
    selected: OnceLock<Context>,
}

impl Scope {
    pub fn new(root: Option<PathBuf>) -> Self {
        let selected = OnceLock::new();
        if let Ok(context) = Context::resolve(root.as_deref()) {
            selected.set(context).ok();
        }
        Self { root, selected }
    }

    pub fn prepare(&self) {
        self.context()
            .and_then(retold_app::setup::ensure_config)
            .ok();
    }

    pub fn request(&self, words: Vec<String>) -> Payload {
        crate::request_using(words.into_iter().map(Into::into).collect(), |matches| {
            if let Some(payload) = crate::session_dispatch::independent(matches) {
                return payload;
            }
            self.context()
                .map_or_else(Diagnostic::into_payload, |context| {
                    crate::session_dispatch::scoped(context, matches)
                })
        })
    }

    pub fn local(&self, task: &Task) -> Payload {
        match task {
            Task::LoadDefaults(_) | Task::SaveDefaults(..) | Task::Resource { .. } => self
                .context()
                .map_or_else(Diagnostic::into_payload, |context| {
                    files::run_task(&scoped_task(task, context))
                }),
            _ => files::run_task(task),
        }
    }

    fn context(&self) -> Result<&Context, Diagnostic> {
        let unselected = self.selected.get().is_none();
        if unselected {
            let context = Context::resolve(self.root.as_deref())?;
            self.selected.set(context).ok();
        }
        let context = self.selected.get().ok_or_else(layout_changed)?;
        let current = Context::resolve(Some(&context.root))?;
        let changed = current.managed != context.managed
            || (context.managed
                && (current.config != context.config || current.library != context.library));
        if changed {
            return Err(layout_changed());
        }
        Ok(context)
    }
}

fn layout_changed() -> Diagnostic {
    Diagnostic::new(
        ErrorCode::DataInvalid,
        "Workspace layout changed. Explicitly switch to the workspace again before continuing.",
    )
}

fn scoped_task(task: &Task, context: &Context) -> Task {
    let root = Some(context.config.clone());
    match task {
        Task::LoadDefaults(_) => Task::LoadDefaults(root),
        Task::SaveDefaults(_, text) => Task::SaveDefaults(root, text.clone()),
        Task::Resource { kind, id, .. } => Task::Resource {
            root,
            kind: kind.clone(),
            id: id.clone(),
        },
        _ => task.clone(),
    }
}

#[cfg(test)]
#[path = "tests/scope.rs"]
mod tests;
