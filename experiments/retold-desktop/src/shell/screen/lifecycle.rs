//! Library controls for one video; trash needs a second, explicit confirmation.
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};
use retold_app::desktop::lifecycle::Change;
use retold_app::library::Video;

use super::{heading_small, muted};
use crate::shell::Shell;

impl Shell {
    pub(super) fn video_lifecycle(&self, video: &Video, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let archive = if video.is_archived {
            Change::Unarchive
        } else {
            Change::Archive
        };
        let archive_label = if video.is_archived {
            "Restore from archive"
        } else {
            "Archive"
        };
        let copy = Change::Copy {
            title: format!("{} (copy)", video.title),
        };
        div().flex().flex_col().gap_2()
            .child(heading_small("Library"))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("desktop-video-archive").self_start().label(archive_label).disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| this.change_video(archive.clone(), cx))))
                .child(Button::new("desktop-video-copy").self_start().label("Copy as new video").disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| this.change_video(copy.clone(), cx))))
                .when(!self.confirm_trash, |row| row.child(Button::new("desktop-video-trash").self_start().label("Move to trash…").disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirm_trash = true;
                        cx.notify();
                    }))))
                .when(self.confirm_trash, |row| row
                    .child(Button::new("desktop-video-trash-confirm").self_start().primary().label(format!("Move “{}” to trash", video.title)).disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.change_video(Change::Trash, cx))))
                    .child(Button::new("desktop-video-trash-cancel").self_start().ghost().label("Keep video")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_trash = false;
                            cx.notify();
                        })))))
            .child(div().flex().flex_wrap().gap_2().items_center()
                .child(muted(&format!("Tags: {}", if video.tags.is_empty() { "none".to_owned() } else { video.tags.join(", ") }), p))
                .child(gpui_kit::component::input::Input::new(&self.views.tag).aria_label("Tag"))
                .child(Button::new("desktop-video-tag-add").self_start().label("Add tag").disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.tag_video(true, cx))))
                .child(Button::new("desktop-video-tag-remove").self_start().ghost().label("Remove tag").disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.tag_video(false, cx)))))
            .child(muted("Unavailable while the video is being generated. Trashed videos stay in the workspace's trash folder.", p))
    }
}

impl Shell {
    pub(super) fn bulk_actions(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let count = self.selection.len();
        if count == 0 {
            return div();
        }
        div().flex().flex_col().gap_2()
            .child(heading_small(&format!("{count} selected")))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("desktop-bulk-archive").self_start().label("Archive selected").disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.change_selected(Change::Archive, cx))))
                .child(Button::new("desktop-bulk-unarchive").self_start().label("Restore selected").disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.change_selected(Change::Unarchive, cx))))
                .child(Button::new("desktop-bulk-tag").self_start().label("Add tag to selected").disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let tag = this.views.tag.read(cx).value().trim().to_owned();
                        if !tag.is_empty() {
                            this.change_selected(Change::AddTag(tag), cx);
                        }
                    })))
                .child(gpui_kit::component::input::Input::new(&self.views.tag).aria_label("Tag for selected videos"))
                .child(Button::new("desktop-bulk-clear").self_start().ghost().label("Clear selection")
                    .on_click(cx.listener(|this, _, _, cx| { this.selection.clear(); cx.notify(); }))))
            .child(muted("Each video is changed in turn; work stops at the first video that is being generated.", p))
    }
}
