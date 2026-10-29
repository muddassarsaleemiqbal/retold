//! Setup navigation and common safe form defaults.
use super::super::{
    form::Form,
    onboarding,
    page::{Modal, Page},
};
use super::App;
use ratatui::crossterm::event::KeyCode;

/// Setup steps that start with a switch on: login checks status, routing starts from suggestions.
const STEP_PRESETS: [(&str, &str); 2] = [
    ("providers login", "status"),
    ("providers route", "suggest"),
];

impl App {
    pub fn open_setup(&mut self) {
        let selected = onboarding::first_pending(&self.store);
        self.modal = Some(Modal::Setup(selected));
    }

    pub(super) fn setup_key(&mut self, selected: usize, code: KeyCode) {
        match code {
            KeyCode::Esc => self.setup_step = Option::default(),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Enter => self.start_setup_step(selected),
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                if let Some(section) = super::super::section::Section::from_digit(digit) {
                    self.go(section);
                }
            }
            _ => {
                let next = super::super::landing::moved(selected, onboarding::STEPS.len(), code);
                self.modal = Some(Modal::Setup(next));
            }
        }
    }

    fn start_setup_step(&mut self, selected: usize) {
        let Some((_, command)) = onboarding::STEPS.get(selected) else {
            return;
        };
        self.setup_step = Some(selected);
        self.open_path(command, &Vec::new());
        let preset = STEP_PRESETS
            .iter()
            .find(|(path, _)| path == command)
            .map(|(_, id)| *id);
        let here = self.here();
        let top = self.pages[here].last_mut();
        if let (Some(Page::Form(form)), Some(id)) = (top, preset) {
            *form = form.clone().with(id, "true");
        }
    }

    pub(super) fn maybe_setup(&mut self) {
        let loading = self.store.busy();
        if self.onboarding_checked || loading {
            return;
        }
        let workspace = self
            .store
            .view(&super::super::store::words("workspace show"));
        let missing = match workspace {
            super::super::store::View::Loading
            | super::super::store::View::Ready {
                refreshing: true, ..
            } => return,
            super::super::store::View::Failed(_) => true,
            super::super::store::View::Ready { .. } => false,
        };
        self.onboarding_checked = true;
        let landing = self.page().is_none();
        let free = self.modal.is_none();
        if missing && landing && free {
            self.open_setup();
        }
    }

    pub(super) fn setup_finished(&mut self, command: &str) {
        let Some(step) = self.setup_step else {
            return;
        };
        let expected = onboarding::STEPS.get(step).map(|(_, command)| *command);
        if expected != Some(command) {
            return;
        }
        let next = step.saturating_add(1);
        let remaining = next < onboarding::STEPS.len();
        self.setup_step = remaining.then_some(next);
        if remaining {
            self.modal = Some(Modal::Setup(next));
        }
    }
}

pub fn defaults(mut form: Form) -> Form {
    if matches!(
        form.action.command().as_str(),
        "video continue" | "video export"
    ) {
        form = form.with("spend", "0").with("background", "true");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        form = form.with(
            "idempotency-key",
            &format!("tui-{}-{stamp}", std::process::id()),
        );
    }
    form
}
