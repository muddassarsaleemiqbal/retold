//! Series parts retain their saved selectors and open existing video runs directly.
use super::super::page::Page;
use super::App;
use ratatui::crossterm::event::KeyCode;
use serde_json::Value;

impl App {
    pub fn series_parts(&self) -> Vec<Value> {
        let Some(Page::Reading(reading)) = self.page() else {
            return Vec::new();
        };
        let series = reading.series_id().is_some();
        if !series {
            return Vec::new();
        }
        self.store
            .contents(&reading.words)
            .and_then(|contents| contents["series"]["parts"].as_array())
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn series_claims(&self, code: KeyCode) -> bool {
        let series =
            matches!(self.page(), Some(Page::Reading(reading)) if reading.series_id().is_some());
        series
            && matches!(
                code,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Char('j' | 'k' | 'p' | 'r' | 'a' | 'l' | 'n')
                    | KeyCode::Enter
            )
    }

    pub(super) fn series_key(&mut self, code: KeyCode) {
        let Some(Page::Reading(reading)) = self.page() else {
            return;
        };
        let Some(id) = reading.series_id().map(str::to_owned) else {
            return;
        };
        let parts = self.series_parts();
        let selected = usize::from(reading.scroll);
        let part = parts.get(selected);
        match code {
            KeyCode::Enter => {
                let reference = part.and_then(|part| part["video_ref"].as_str());
                if let Some(reference) = reference {
                    self.open_video(reference);
                }
            }
            KeyCode::Char('p' | 'r') => self.series_action(code, &id, part),
            KeyCode::Char('l' | 'n') => self.series_draft_action(code, &id, part),
            KeyCode::Char('a') => {
                self.open_path("series update", &vec![("series".into(), id)]);
            }
            _ => self.select_series_part(super::super::landing::moved(selected, parts.len(), code)),
        }
    }

    fn series_action(&mut self, code: KeyCode, id: &str, part: Option<&Value>) {
        let Some(selector) = part.and_then(|part| part["id"].as_str()) else {
            return;
        };
        let command = if code == KeyCode::Char('p') {
            "series plan"
        } else {
            "series run"
        };
        self.open_path(
            command,
            &vec![
                ("series".into(), id.into()),
                ("parts".into(), selector.into()),
            ],
        );
    }

    fn series_draft_action(&mut self, code: KeyCode, id: &str, part: Option<&Value>) {
        let Some(part) = part else {
            return;
        };
        if code == KeyCode::Char('l') {
            let fill = vec![
                ("series".into(), id.into()),
                ("part".into(), crate::human::text(part, "id")),
            ];
            self.open_path("series update", &fill);
            return;
        }
        let Some(Page::Reading(reading)) = self.page() else {
            return;
        };
        let Some(contents) = self.store.contents(&reading.words) else {
            return;
        };
        let series = &contents["series"];
        let fill = vec![
            ("guide".into(), crate::human::text(series, "guide")),
            ("channel".into(), crate::human::text(series, "channel")),
            ("work".into(), crate::human::text(part, "title")),
        ];
        self.open_path("new", &fill);
    }

    fn select_series_part(&mut self, selected: usize) {
        let here = self.here();
        let page = self.pages[here].last_mut();
        if let Some(Page::Reading(reading)) = page {
            reading.scroll = u16::try_from(selected).unwrap_or(u16::MAX);
        }
    }
}
