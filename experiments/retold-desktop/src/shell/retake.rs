//! One pending retake review at a time, shared by the Scenes and Narration workspaces.
use gpui_kit::Context;
use retold_app::retake::{self, Retake, ReviewedRetake};

use super::Shell;

impl Shell {
    pub(super) fn review_retake(&mut self, target: Retake, cx: &mut Context<Self>) {
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        if self.busy {
            return;
        }
        let (library, reference) = (snapshot.context.library.clone(), reference.clone());
        let reviewed = reference.clone();
        self.busy = true;
        self.retake = None;
        let work = cx
            .background_executor()
            .spawn(async move { retake::review(&library, &reference, target) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let still_open = this.selected_video.as_deref() == Some(reviewed.as_str());
                match result {
                    Ok(review) if still_open => {
                        this.status = "Review the retake. Nothing was changed.".into();
                        this.retake = Some(review);
                        this.retake_video = Some(reviewed);
                    }
                    Ok(_) => {}
                    Err(error) => this.status = error.message,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn confirm_retake(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.retake.clone() else {
            return;
        };
        if self.retake_video != self.selected_video {
            self.retake = None;
            self.retake_video = None;
            return;
        }
        if self.busy {
            return;
        }
        self.busy = true;
        let work = cx
            .background_executor()
            .spawn(async move { retake::request(&review) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.retake = None;
                this.status = match result {
                    Ok(_) => "Retake requested. Review a run to make the new version.".into(),
                    Err(error) => error.message,
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn retake_matches(&self, target: &Retake) -> Option<&ReviewedRetake> {
        let same_video = self.retake_video.is_some() && self.retake_video == self.selected_video;
        self.retake
            .as_ref()
            .filter(|review| same_video && &review.target == target)
    }
}
