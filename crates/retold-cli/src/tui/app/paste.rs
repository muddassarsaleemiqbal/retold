//! Paste is text for one focused owner, never a sequence of terminal key presses.
use super::super::{
    form::Entry,
    input::Input,
    page::{Modal, Page},
};
use super::{App, Tone};

impl App {
    pub(super) fn paste(&mut self, text: &str) {
        let problem = self
            .paste_input()
            .and_then(|input| input.paste_problem(text));
        if let Some(problem) = problem {
            self.toast(problem.into(), Tone::Failure);
            return;
        }
        let modal = self.modal.take();
        if let Some(modal) = modal {
            let modal = pasted_modal(modal, text);
            if let Modal::Browser(browser) = &modal {
                self.request_preview(browser);
            }
            self.modal = Some(modal);
        } else {
            self.paste_page(text);
        }
    }

    fn paste_input(&self) -> Option<&Input> {
        if let Some(modal) = &self.modal {
            return modal_input(modal);
        }
        match self.page() {
            Some(Page::Form(form)) => match form.entries.get(form.focus) {
                Some(Entry::Text(input)) => Some(input),
                _ => None,
            },
            Some(Page::Reading(reading)) => reading
                .stream
                .as_ref()
                .filter(|stream| stream.filtering)
                .map(|stream| &stream.filter),
            None if self.landing().filtering => Some(&self.landing().cursor().filter),
            _ => None,
        }
    }

    fn paste_page(&mut self, text: &str) {
        let here = self.here();
        let filtering = self.landing().filtering;
        match self.pages[here].last_mut() {
            Some(Page::Form(form)) => {
                let focused = form.entries.get_mut(form.focus);
                if let Some(Entry::Text(input)) = focused {
                    input.paste(text);
                    form.mark_focused_edited();
                    form.problem = None;
                }
            }
            Some(Page::Reading(reading)) => {
                if let Some(stream) = &mut reading.stream
                    && stream.filtering
                {
                    stream.filter.paste(text);
                }
            }
            None if filtering => self.landings[here].paste_filter(text),
            _ => {}
        }
    }
}

const fn modal_input(modal: &Modal) -> Option<&Input> {
    match modal {
        Modal::Palette(palette) | Modal::Values(palette) => Some(&palette.query),
        Modal::Browser(browser) => Some(&browser.query),
        Modal::Document(editor) => Some(&editor.input),
        Modal::Route(editor) => editor.adding.as_ref(),
        Modal::BulkTag(input) | Modal::Destroy { typed: input, .. } => Some(input),
        _ => None,
    }
}

fn pasted_modal(mut modal: Modal, text: &str) -> Modal {
    match &mut modal {
        Modal::Palette(palette) | Modal::Values(palette) => {
            palette.query.paste(text);
            palette.selected = 0;
        }
        Modal::Browser(browser) => {
            browser.query.paste(text);
            browser.selected = 0;
            browser.preview = None;
        }
        Modal::Document(editor) => {
            editor.input.paste(text);
            editor.problem = None;
        }
        Modal::Route(editor) => {
            if let Some(input) = &mut editor.adding {
                input.paste(text);
                editor.problem = None;
            }
        }
        Modal::BulkTag(input) | Modal::Destroy { typed: input, .. } => input.paste(text),
        _ => {}
    }
    modal
}
