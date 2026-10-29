//! Native source selection grants only read access and an offline draft snapshot.
use super::{Shell, backend::Request};
use gpui_kit::{Context, PathPromptOptions, Window};

impl Shell {
    pub(super) fn preview_source(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.draft.as_ref().and_then(|form| form.source_path(cx)) {
            self.request(Request::PreviewSource(path), cx);
        }
    }

    pub(super) fn choose_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.prompting {
            return;
        }
        let Some(form) = &self.draft else { return };
        let input = form.source.clone();
        let before = input.read(cx).value();
        self.prompting = true;
        let chooser = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose story source text".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = chooser.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                let same = this.draft.as_ref().is_some_and(|form| form.source == input)
                    && input.read(cx).value() == before;
                if same {
                    match result {
                        Ok(Ok(Some(paths))) => if let Some(path) = paths.first() {
                            if let Some(path) = path.to_str() {
                                input.update(cx, |input, cx| input.set_value(path.to_owned(), window, cx));
                                if let Some(form) = &mut this.draft { form.preview = None; }
                            } else { this.status = "This source path cannot be represented as text. Choose a UTF-8 path.".into(); }
                        },
                        Ok(Ok(None)) => {},
                        _ => this.status = "The source chooser could not be opened. Paste a source path to continue.".into(),
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
