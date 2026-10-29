//! Feedback stays beside its video context, with retained text and explicit saving.
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    input::Textarea,
};
use gpui_kit::{Context, Div, div, prelude::*, px};

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{Shell, notes::NoteAction};

impl Shell {
    pub(super) fn note_page(&self, cx: &mut Context<Self>) -> Div {
        let Some(panel) = &self.notes else {
            return div();
        };
        let p = self.palette();
        let text = panel.text.read(cx).value();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .max_w(px(860.))
            .child(
                Button::new("desktop-notes-back")
                    .self_start()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .ml(gpui_kit::px(-10.))
                    .label("Back to video")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_notes = false;
                        cx.notify();
                    })),
            )
            .child(super::page_header(
                if panel.target.is_some() {
                    "Notes"
                } else {
                    "Video notes"
                },
                "Comments stay with the version you reviewed. Back keeps your unsaved text.",
                [
                    super::refresh_button("desktop-notes-refresh", "Refresh comments", self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.note_action(NoteAction::Read(0), window, cx)
                        }))
                        .into_any_element(),
                ],
                p,
            ))
            .when_some(panel.document.as_ref(), |body, document| {
                body.child(heading_small(&document.title))
            })
            .child(self.checkpoint_status(panel.checkpoint.as_deref(), panel.dirty(cx)))
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(super::decision(
                    error,
                    [Button::new("desktop-notes-refresh-fix")
                        .label("Refresh comments")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.note_action(NoteAction::Read(0), window, cx)
                        }))],
                    p,
                ))
            });
        body = body.child(self.note_context(cx));
        let composing = panel.composing
            || panel.dirty(cx)
            || panel
                .document
                .as_ref()
                .is_none_or(|document| document.total == 0);
        if composing {
            body = body
                .child(heading_small("Write a note"))
                .child(
                    Textarea::new(&panel.text)
                        .aria_label("Review note")
                        .h(px(150.))
                        .disabled(self.busy || self.recovery.closing || self.recovery.quitting),
                )
                .when(text.len() > 8192, |body| {
                    body.child(error_notice(
                        "This note is too long. Shorten it before saving.",
                        p,
                    ))
                })
                .child(
                    Button::new("desktop-notes-save")
                        .self_start()
                        .primary()
                        .label("Save note")
                        .disabled(
                            self.busy
                                || panel.document.is_none()
                                || panel.id.is_none()
                                || text.trim().is_empty()
                                || text.len() > 8192,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.note_action(NoteAction::Save, window, cx)
                        })),
                );
        } else {
            body = body.child(
                Button::new("desktop-notes-write")
                    .self_start()
                    .primary()
                    .label("Write a note")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(panel) = &mut this.notes {
                            panel.composing = true;
                            panel.text.update(cx, |state, cx| state.focus(window, cx));
                        }
                        cx.notify();
                    })),
            );
        }
        let Some(document) = &panel.document else {
            return body.child(muted(
                if self.busy {
                    "Loading comments…"
                } else {
                    "Refresh comments to try again. Your text stays here."
                },
                p,
            ));
        };
        body = body.child(muted(&format!("{} comments", document.total), p));
        if document.notes.is_empty() {
            body = body.child(muted("No comments on this version yet.", p));
        }
        body.children(document.notes.iter().map(|note| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_4()
                .bg(p.surface)
                .rounded_lg()
                .border_1()
                .border_color(p.border)
                .child(muted(&super::when(&note.created_at.to_string()), p))
                .child(note.text.clone())
        }))
        .when(
            document.offset > 0 || document.next_offset.is_some(),
            |body| {
                body.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("desktop-notes-previous")
                                .self_start()
                                .label("Previous comments")
                                .disabled(self.busy || document.offset == 0)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(document) = this
                                        .notes
                                        .as_ref()
                                        .and_then(|panel| panel.document.as_ref())
                                    {
                                        this.note_action(
                                            NoteAction::Read(
                                                document
                                                    .offset
                                                    .saturating_sub(retold_app::notes::PAGE_SIZE),
                                            ),
                                            window,
                                            cx,
                                        );
                                    }
                                })),
                        )
                        .child(
                            Button::new("desktop-notes-next")
                                .self_start()
                                .label("Next comments")
                                .disabled(self.busy || document.next_offset.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(offset) = this
                                        .notes
                                        .as_ref()
                                        .and_then(|panel| panel.document.as_ref())
                                        .and_then(|document| document.next_offset)
                                    {
                                        this.note_action(NoteAction::Read(offset), window, cx);
                                    }
                                })),
                        ),
                )
            },
        )
    }

    pub(super) fn retained_note_notice(&self, cx: &mut Context<Self>) -> Div {
        let visible = self.show_notes
            && self.destination == crate::shell::Destination::Videos
            && self
                .notes
                .as_ref()
                .is_some_and(|panel| self.selected_video.as_deref() == Some(&panel.reference));
        if visible || self.notes.as_ref().is_none_or(|panel| !panel.dirty(cx)) {
            return div();
        }
        let p = self.palette();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .mb_4()
            .child(muted("An unsaved review note is kept in this window.", p))
            .child(
                Button::new("desktop-notes-return")
                    .self_start()
                    .label("Return to note")
                    .on_click(cx.listener(|this, _, _, cx| this.return_to_notes(cx))),
            )
    }
}

impl Shell {
    fn note_context(&self, cx: &mut Context<Self>) -> Div {
        let Some(panel) = &self.notes else {
            return div();
        };
        let Some(target) = &panel.target else {
            return div();
        };
        let p = self.palette();
        let changed = panel
            .current
            .as_ref()
            .is_some_and(|current| current != target);
        div().flex().flex_col().gap_2()
            .child(heading_small(&target.label()))
            .when_some(panel.target_problem.as_ref(), |body, problem| body.child(notice(problem, p)))
            .when(changed, |body| body.child(Button::new("desktop-notes-compare-version").self_start().label("Compare current version").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| { if let Some(panel) = &mut this.notes { panel.rebind = true; } cx.notify(); }))))
            .when(panel.rebind, |body| {
                body.child(muted("Moving your note keeps its text and starts a new thread; earlier comments stay where they were made.", p))
                    .child(div().flex().flex_wrap().gap_2()
                        .child(Button::new("desktop-notes-keep-version").label("Keep original version").on_click(cx.listener(|this, _, _, cx| { if let Some(panel) = &mut this.notes { panel.rebind = false; } cx.notify(); })))
                        .child(Button::new("desktop-notes-rebind").primary().label("Move my note to the latest version").disabled(self.busy || self.recovery_pending()).on_click(cx.listener(|this, _, window, cx| this.rebind_note(window, cx)))))
                    .when(self.recovery_pending(), |body| body.child(muted("Finish restoring your saved draft first; then you can move the note.", p)))
            })
    }
}
