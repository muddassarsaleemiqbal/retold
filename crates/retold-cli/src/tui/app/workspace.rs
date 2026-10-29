//! Workspace transitions wait for tracked work and replace every root-scoped UI state.
use super::super::{form::Form, page::Page};
use super::{App, Tone};
use std::path::{Path, PathBuf};

impl App {
    pub fn is_switching_workspace(&self) -> bool {
        let pending = self.switch_to.is_some();
        pending
            || self.pages.iter().flatten().any(|page| {
                matches!(
                    page, Page::Outcome(outcome) if self.running.contains(&outcome.id)
                        && outcome.form.action.command() == "workspace switch"
                )
            })
    }

    pub fn workspace_ready(&self) -> Option<&Path> {
        let idle = self.running.is_empty() && !self.quit;
        self.switch_to.as_deref().filter(|_| idle)
    }

    pub fn enter_workspace(&mut self, root: PathBuf) {
        let mut next = Self::new(self.theme);
        next.workspace = Some(root);
        next.now = self.now;
        next.area = self.area;
        next.load_defaults();
        next.toast(
            "Workspace changed; unsaved TUI drafts and selections cleared.".into(),
            Tone::Success,
        );
        *self = next;
    }

    pub(super) fn workspace_wait(&self, form: &Form) -> Option<&'static str> {
        let changing = self.is_switching_workspace();
        if changing {
            return Some("Wait for the workspace switch before starting another action.");
        }
        let switching = form.action.command() == "workspace switch";
        let busy = !self.running.is_empty();
        (switching && busy).then_some("Wait for the current action before switching workspaces.")
    }
}
