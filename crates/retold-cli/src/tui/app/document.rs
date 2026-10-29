//! Editing a manifest field starts from its recorded document hash.
use super::super::{
    document,
    form::Form,
    page::{Modal, Page},
};
use super::App;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

impl App {
    pub(super) fn document_claims(&self, code: KeyCode) -> bool {
        let document = matches!(self.page(),Some(Page::Reading(reading)) if reading.document_subject().is_some());
        document
            && matches!(
                code,
                KeyCode::Enter
                    | KeyCode::Insert
                    | KeyCode::Delete
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Char('j' | 'k' | 'J' | 'K')
            )
    }
    pub(super) fn document_key(&mut self, code: KeyCode) {
        let Some(Page::Reading(reading)) = self.page() else {
            return;
        };
        let Some((kind, id)) = reading.document_subject() else {
            return;
        };
        let Some(contents) = self.store.contents(&reading.words) else {
            return;
        };
        let document = &contents["document"];
        self.document_selection(
            code,
            (reading.scroll, kind.to_owned(), id.to_owned()),
            &document.clone(),
        );
    }

    fn document_selection(
        &mut self,
        code: KeyCode,
        subject: (u16, String, String),
        document: &serde_json::Value,
    ) {
        let (scroll, kind, id) = subject;
        let fields = document::fields(document);
        let selected = usize::from(scroll);
        match code {
            KeyCode::Enter | KeyCode::Insert | KeyCode::Delete | KeyCode::Char('J' | 'K') => {
                self.document_edit(code, (kind, id), (document, fields.get(selected)));
            }
            _ => self.select_document_field(super::super::landing::moved(
                selected,
                fields.len(),
                code,
            )),
        }
    }
    fn document_edit(
        &mut self,
        code: KeyCode,
        subject: (String, String),
        selection: (&serde_json::Value, Option<&document::Field>),
    ) {
        let (document, field) = selection;
        let Some(field) = field else {
            return;
        };
        if code == KeyCode::Enter {
            self.edit_document_field(subject, document, field);
        } else {
            self.edit_list_item(subject, document, (field, code));
        }
    }
    fn edit_document_field(
        &mut self,
        subject: (String, String),
        document: &serde_json::Value,
        field: &document::Field,
    ) {
        let (kind, id) = subject;
        let command = format!("{kind} edit");
        let Some(action) = self
            .catalog
            .iter()
            .find(|action| action.command() == command)
        else {
            return;
        };
        let form = Form::new(action.clone())
            .with("id", &id)
            .with("field", &field.path)
            .with(
                "expected-hash",
                document["hash"].as_str().unwrap_or_default(),
            );
        let mut editor = document::Editor::new(form, &field.value);
        editor.missing = field.missing;
        self.modal = Some(Modal::Document(editor));
    }
    fn select_document_field(&mut self, selected: usize) {
        let here = self.here();
        let page = self.pages[here].last_mut();
        if let Some(Page::Reading(reading)) = page {
            reading.scroll = u16::try_from(selected).unwrap_or(u16::MAX);
        }
    }
    pub(super) fn document_edit_key(&mut self, mut editor: document::Editor, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter => match editor.reviewed() {
                Ok(form) => self.confirm(form),
                Err(problem) => {
                    editor.problem = Some(problem);
                    self.modal = Some(Modal::Document(editor));
                }
            },
            _ => {
                editor.type_key(key);
                self.modal = Some(Modal::Document(editor));
            }
        }
    }
}

impl App {
    fn edit_list_item(
        &mut self,
        subject: (String, String),
        document: &serde_json::Value,
        selection: (&document::Field, KeyCode),
    ) {
        let (field, code) = selection;
        let Some((path, items, index)) = document::list_context(document, &field.path) else {
            return;
        };
        let Some((value, items)) = list_replacement(items, index, (code, field)) else {
            return;
        };
        let target = document::Field {
            path,
            value,
            missing: false,
        };
        self.edit_document_field(subject, document, &target);
        if code == KeyCode::Insert
            && let Some(Modal::Document(editor)) = &mut self.modal
        {
            editor.append_to = Some(items);
        }
    }
}

/// The list a removal or move leaves, or the item itself for an edit or insertion; the
/// unchanged list is returned for an insertion to append to.
fn list_replacement(
    mut items: Vec<serde_json::Value>,
    index: usize,
    change: (KeyCode, &document::Field),
) -> Option<(serde_json::Value, Vec<serde_json::Value>)> {
    let (code, field) = change;
    let neighbour = match code {
        KeyCode::Delete => {
            items.remove(index);
            return Some((serde_json::Value::Array(items.clone()), items));
        }
        KeyCode::Char('J') => Some(index + 1).filter(|next| *next < items.len()),
        KeyCode::Char('K') => index.checked_sub(1),
        _ => return Some((field.value.clone(), items)),
    }?;
    items.swap(index, neighbour);
    Some((serde_json::Value::Array(items.clone()), items))
}
