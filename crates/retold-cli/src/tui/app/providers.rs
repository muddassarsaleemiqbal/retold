//! Provider status inspection is read-only; configuration and testing keep their existing gates.
use ratatui::crossterm::event::KeyCode;

use super::super::{
    collection,
    landing::Tab,
    page::{Page, Reading},
};
use super::App;

impl App {
    fn status_subject(&self) -> Option<String> {
        match self.page() {
            Some(Page::Reading(reading)) => reading.provider_id().map(str::to_owned),
            Some(_) => None,
            None => {
                let Tab::Collection(selected) = self.tab() else {
                    return None;
                };
                let providers = [
                    &collection::PROVIDERS,
                    &collection::HEALTH,
                    &collection::LOGINS,
                ];
                providers
                    .contains(&selected)
                    .then(|| self.selected_item())
                    .flatten()
                    .map(|item| item.key)
            }
        }
    }

    pub(super) fn provider_claims(&self, code: KeyCode) -> bool {
        let already_open = code == KeyCode::Char('l') && self.page().is_some();
        let shortcut = matches!(code, KeyCode::Char('l' | 'c' | 't'));
        shortcut && !already_open && self.status_subject().is_some()
    }

    pub(super) fn provider_command(&mut self, code: KeyCode) {
        let Some(id) = self.status_subject() else {
            return;
        };
        match code {
            KeyCode::Char('l') => self.push(Page::Reading(Reading::provider(&id))),
            KeyCode::Char('c') => self.open_path("providers login", &vec![("id".into(), id)]),
            KeyCode::Char('t') => self.open_path(
                "providers test",
                &vec![("id".into(), id), ("spend".into(), "0".into())],
            ),
            _ => {}
        }
    }
}
