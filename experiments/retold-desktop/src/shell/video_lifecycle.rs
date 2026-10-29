//! Archive, restore, copy and trash for the selected video, with a two-step trash confirmation.
use gpui_kit::Context;
use retold_app::desktop::lifecycle::{self, Change};

use super::Shell;

impl Shell {
    pub(super) fn change_video(&mut self, change: Change, cx: &mut Context<Self>) {
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        if self.busy {
            return;
        }
        let (library, reference) = (snapshot.context.library.clone(), reference.clone());
        self.busy = true;
        self.confirm_trash = false;
        let trashing = change == Change::Trash;
        let work = cx
            .background_executor()
            .spawn(async move { lifecycle::apply(&library, &reference, &change) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(outcome) => {
                        this.status = outcome.text;
                        if trashing {
                            this.selected_video = None;
                        }
                        this.refresh(cx);
                    }
                    Err(error) => this.status = error.message,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Shell {
    /// Apply one change to every selected video in order, stopping at the first refusal.
    pub(super) fn change_selected(&mut self, change: Change, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        if self.busy || self.selection.is_empty() {
            return;
        }
        let library = snapshot.context.library.clone();
        let references: Vec<String> = self.selection.iter().cloned().collect();
        self.busy = true;
        let work = cx.background_executor().spawn(async move {
            let mut done = 0_usize;
            for reference in &references {
                if let Err(error) = lifecycle::apply(&library, reference, &change) {
                    return (done, Some(format!("{reference}: {}", error.message)));
                }
                done += 1;
            }
            (done, None)
        });
        cx.spawn(async move |this, cx| {
            let (done, failure) = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.selection.clear();
                this.status = match failure {
                    Some(error) => format!("Changed {done} video(s), then stopped. {error}"),
                    None => format!("Changed {done} video(s)."),
                };
                this.refresh(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
