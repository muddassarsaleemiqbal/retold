//! Read-only previews precede financial changes; applying always revalidates current state.
use super::form::Form;

#[derive(Debug, Clone)]
pub struct Preview {
    pub form: Form,
    pub words: Vec<String>,
    pub scroll: u16,
}

impl Preview {
    pub const fn scroll_by(&mut self, code: ratatui::crossterm::event::KeyCode) {
        const PAGE_ROWS: u16 = 10;
        use ratatui::crossterm::event::KeyCode;
        let change = match code {
            KeyCode::Up | KeyCode::Char('k') => -1,
            KeyCode::Down | KeyCode::Char('j') => 1,
            KeyCode::PageUp => -PAGE_ROWS.cast_signed(),
            KeyCode::PageDown => PAGE_ROWS.cast_signed(),
            _ => 0,
        };
        self.scroll = self.scroll.saturating_add_signed(change);
    }

    pub fn new(form: &Form) -> Option<Self> {
        let supported = matches!(
            form.action.command().as_str(),
            "account limit"
                | "account reconcile"
                | "account backup"
                | "guides edit"
                | "channels edit"
                | "storage backup"
                | "storage restore"
                | "doctor"
                | "storage activate"
                | "new"
                | "video new"
                | "video redo"
        );
        let dry = form.value("dry-run").as_deref() == Some("true");
        if !supported || dry {
            return None;
        }
        let preview = form.clone().with("plan-hash", "");
        Some(Self {
            form: form.clone(),
            words: preview.with("dry-run", "true").arguments(),
            scroll: 0,
        })
    }
}

impl Preview {
    pub fn executable(&self, contents: &serde_json::Value) -> Option<Form> {
        let activating = self.form.action.command() == "storage activate";
        if !activating {
            return Some(self.form.clone());
        }
        let hash = contents["activation_plan"]["plan_hash"]
            .as_str()
            .or_else(|| contents["reconciliation_plan"]["plan_hash"].as_str())?;
        let form = self.form.clone().with("dry-run", "false");
        let form = form.with("plan-hash", hash);
        form.validation_problem().is_none().then_some(form)
    }
}
