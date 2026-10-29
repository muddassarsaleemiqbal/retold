//! Production-scoped media selection clears its decoder before changing filters or pages.
use gpui_kit::{AppContext as _, Context, Entity};
use retold_app::desktop::media::{self, Page};

use super::{Shell, player::Player};

pub(super) struct MediaPanel {
    pub library: std::path::PathBuf,
    pub reference: String,
    pub edition: Option<String>,
    pub page: Option<Page>,
    pub selected: Option<usize>,
    pub player: Entity<Player>,
    pub error: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) enum FileAction {
    Reveal,
    Editor,
}

impl Shell {
    pub(super) fn open_media_file(&mut self, action: FileAction, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(panel) = &self.saved_media else {
            return;
        };
        let Some(item) = panel
            .selected
            .and_then(|index| panel.page.as_ref()?.items.get(index))
            .cloned()
        else {
            return;
        };
        if matches!(action, FileAction::Editor) && item.kind != media::Kind::Project {
            return;
        }
        let scope = (
            panel.library.clone(),
            panel.reference.clone(),
            item.path.clone(),
        );
        self.busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { item.validated_path() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let Some(panel) = &mut this.saved_media else {
                    return;
                };
                let selected = panel
                    .selected
                    .and_then(|index| panel.page.as_ref()?.items.get(index));
                if panel.library != scope.0
                    || panel.reference != scope.1
                    || selected.is_none_or(|item| item.path != scope.2)
                {
                    return;
                }
                match result {
                    Ok(path) => match action {
                        FileAction::Reveal => cx.reveal_path(&path),
                        FileAction::Editor => cx.open_with_system(&path),
                    },
                    Err(error) => {
                        panel.error = Some(error.message);
                        panel.selected = None;
                        panel
                            .player
                            .update(cx, |player, cx| player.select(None, cx));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn begin_media(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.editor.as_ref().is_some_and(|editor| editor.dirty(cx)) {
            return;
        }
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        self.saved_media = Some(MediaPanel {
            library: snapshot.context.library.clone(),
            reference: reference.clone(),
            edition: None,
            page: None,
            selected: None,
            player: cx.new(|_| Player::new()),
            error: None,
        });
        self.show_script = false;
        self.characters = None;
        self.delivery = None;
        self.load_media(None, 0, cx);
    }

    pub(super) fn clear_media(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = &self.saved_media {
            panel
                .player
                .update(cx, |player, cx| player.select(None, cx));
        }
        self.saved_media = None;
    }

    pub(super) fn load_media(
        &mut self,
        edition: Option<String>,
        offset: usize,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(panel) = &mut self.saved_media else {
            return;
        };
        panel
            .player
            .update(cx, |player, cx| player.select(None, cx));
        panel.selected = None;
        panel.edition.clone_from(&edition);
        panel.error = None;
        let library = panel.library.clone();
        let reference = panel.reference.clone();
        let scope = (library.clone(), reference.clone());
        self.busy = true;
        let task = cx.background_executor().spawn(async move {
            media::page(&library, &reference, &media::Query { edition, offset })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if let Some(panel) = &mut this.saved_media
                    && panel.library == scope.0
                    && panel.reference == scope.1
                {
                    match result {
                        Ok(page) => {
                            panel.page = Some(page);
                        }
                        Err(error) => {
                            panel.page = None;
                            panel.error = Some(error.message);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn select_media(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(panel) = &mut self.saved_media else {
            return;
        };
        let item = panel
            .page
            .as_ref()
            .and_then(|page| page.items.get(index))
            .cloned();
        panel.selected = item.as_ref().map(|_| index);
        panel
            .player
            .update(cx, |player, cx| player.select(item, cx));
        cx.notify();
    }
}
