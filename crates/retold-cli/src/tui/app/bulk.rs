//! Selection is explicit, and batch archive/trash reviews every reference before dispatch.
use super::super::{
    collection,
    form::Form,
    landing::Tab,
    page::{Batch, Modal},
};
use super::App;
use ratatui::crossterm::event::KeyCode;

#[derive(Clone, Copy)]
pub(super) enum BulkAction {
    Select,
    Archive,
    Trash,
    Tag,
}
impl BulkAction {
    const fn path(self) -> Option<&'static str> {
        match self {
            Self::Select | Self::Tag => None,
            Self::Archive => Some("video archive"),
            Self::Trash => Some("video delete"),
        }
    }
}

impl App {
    pub(super) fn bulk_command(&self, code: KeyCode) -> Option<BulkAction> {
        let videos = self.tab() == Tab::Collection(&collection::VIDEOS);
        if !videos {
            return None;
        }
        match code {
            KeyCode::Char(' ') => Some(BulkAction::Select),
            KeyCode::Char('A') => Some(BulkAction::Archive),
            KeyCode::Char('D') => Some(BulkAction::Trash),
            KeyCode::Char('t') => Some(BulkAction::Tag),
            _ => None,
        }
    }
    pub(super) fn bulk_key(&mut self, action: BulkAction) {
        if matches!(action, BulkAction::Tag) {
            self.modal = Some(Modal::BulkTag(super::super::input::Input::default()));
            return;
        }
        let Some(command) = action.path() else {
            self.toggle_video_selection();
            return;
        };
        let empty = self.selected_videos.is_empty();
        if empty {
            self.toast(
                "Select videos with Space first.".into(),
                super::Tone::Notice,
            );
            return;
        }
        self.review_bulk(command);
    }
    fn review_bulk(&mut self, command: &str) {
        let Some(action) = self
            .catalog
            .iter()
            .find(|action| action.command() == command)
        else {
            return;
        };
        let forms = self
            .selected_videos
            .iter()
            .map(|reference| Form::new(action.clone()).with("reference", reference))
            .collect();
        self.modal = Some(Modal::Batch(Batch { forms, scroll: 0 }));
    }
    fn toggle_video_selection(&mut self) {
        if let Some(item) = self.selected_item() {
            let selected = self.selected_videos.remove(&item.key);
            if !selected {
                self.selected_videos.insert(item.key);
            }
        }
    }
    pub(super) fn batch_key(&mut self, mut batch: Batch, code: KeyCode) {
        match code {
            KeyCode::Esc => {}
            KeyCode::Enter => {
                self.selected_videos.clear();
                for form in batch.forms {
                    self.run(form);
                }
            }
            _ => {
                batch.scroll = super::keys::scrolled(batch.scroll, code);
                self.modal = Some(Modal::Batch(batch));
            }
        }
    }
}

impl App {
    pub(super) fn bulk_tag_key(
        &mut self,
        mut input: super::super::input::Input,
        key: ratatui::crossterm::event::KeyEvent,
    ) {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter if !input.text().trim().is_empty() => {
                self.review_tags(input.text().trim());
            }
            _ => {
                input.edit(key);
                self.modal = Some(Modal::BulkTag(input));
            }
        }
    }
    fn review_tags(&mut self, tag: &str) {
        let Some(action) = self
            .catalog
            .iter()
            .find(|action| action.command() == "video tag")
        else {
            return;
        };
        let forms = self
            .selected_videos
            .iter()
            .map(|reference| {
                Form::new(action.clone())
                    .with("reference", reference)
                    .with("add", tag)
            })
            .collect::<Vec<_>>();
        if forms.is_empty() {
            self.toast(
                "Select videos with Space first.".into(),
                super::Tone::Notice,
            );
            return;
        }
        self.modal = Some(Modal::Batch(Batch { forms, scroll: 0 }));
    }
}
