//! Save a copy of a saved file to a chosen folder; an existing file needs explicit replacement.
use std::path::PathBuf;

use gpui_kit::{Context, PathPromptOptions};
use retold_app::desktop::destinations::{self, Checked, Overwrite};
use retold_app::desktop::media::Item;

use super::Shell;

#[derive(Default)]
pub(super) struct DestinationPanel {
    pub replace: Option<(Item, PathBuf)>,
    pub history: Option<Vec<Checked>>,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl Shell {
    fn media_scope(&self) -> Option<(PathBuf, String)> {
        let panel = self.saved_media.as_ref()?;
        Some((panel.library.clone(), panel.reference.clone()))
    }

    pub(super) fn choose_copy_folder(&mut self, item: Item, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let chooser = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Save a copy here".into()),
        });
        cx.spawn(async move |this, cx| {
            let chosen = chooser
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            let _ = this.update(cx, |this, cx| {
                if let Some(folder) = chosen {
                    let (_, exists) = destinations::target(&item, &folder);
                    if exists {
                        this.copies.replace = Some((item, folder));
                        cx.notify();
                    } else {
                        this.copy_delivery(item, folder, Overwrite::Refuse, cx);
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn confirm_replace(&mut self, cx: &mut Context<Self>) {
        if let Some((item, folder)) = self.copies.replace.take() {
            self.copy_delivery(item, folder, Overwrite::Replace, cx);
        }
    }

    fn copy_delivery(
        &mut self,
        item: Item,
        folder: PathBuf,
        overwrite: Overwrite,
        cx: &mut Context<Self>,
    ) {
        let Some((library, reference)) = self.media_scope() else {
            return;
        };
        self.busy = true;
        self.copies.error = None;
        let work = cx.background_executor().spawn(async move {
            destinations::deliver(&library, &reference, (&item, &folder, overwrite))?;
            destinations::history(&library, &reference)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(history) => {
                        this.copies.message = Some("Copy saved and verified.".into());
                        this.copies.history = Some(history);
                    }
                    Err(error) => this.copies.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn load_delivery_history(&mut self, cx: &mut Context<Self>) {
        let Some((library, reference)) = self.media_scope() else {
            return;
        };
        let work = cx
            .background_executor()
            .spawn(async move { destinations::history(&library, &reference) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(history) => this.copies.history = Some(history),
                    Err(error) => this.copies.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
