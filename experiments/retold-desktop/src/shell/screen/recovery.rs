//! Quiet retained-draft recovery stays separate from production and run controls.
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};
use retold_app::desktop::recovery::Content;

use super::{error_notice, heading_small, muted};
use crate::shell::Shell;

impl Shell {
    pub(super) fn checkpoint_status(&self, id: Option<&str>, dirty: bool) -> Div {
        if !dirty || !self.recovery_enabled() {
            return div();
        }
        let message = if !self.recovery.blocked.is_empty() || self.recovery.error.is_some() {
            "Draft retention failed. Keep this window open and resolve the recovery error above."
        } else if self.recovery_pending() {
            "Keeping your draft on this Mac…"
        } else if id.is_some_and(|id| self.recovery.records.contains_key(id)) {
            "Editing text retained on this device. It has not been saved to the video."
        } else {
            "Editing text has not yet been retained. Keep this window open."
        };
        muted(message, self.palette())
    }
    pub(super) fn recovery_notice(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let mut body = div().flex().flex_col().gap_2();
        for error in self.recovery.blocked.values() {
            body = body.child(error_notice(error, p));
        }
        if let Some(error) = &self.recovery.error {
            body = body
                .child(error_notice(
                    &format!("Draft recovery needs attention: {error}"),
                    p,
                ))
                .child(
                    Button::new("desktop-recovery-retry")
                        .self_start()
                        .label("Retry retaining drafts")
                        .disabled(self.recovery.saving || self.recovery.loading)
                        .on_click(cx.listener(|this, _, _, cx| this.flush_checkpoints(cx))),
                );
            body = body.child(
                Button::new("desktop-recovery-refresh")
                    .self_start()
                    .label("Refresh list and retain editor text separately")
                    .disabled(self.recovery.saving || self.recovery.loading)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_retained_drafts(cx))),
            );
        }
        if self.recovery.closing {
            body = body.child(muted(
                "Discarding the retained editing drafts before closing…",
                p,
            ));
        }
        let entries: Vec<_> = self.retained_checkpoints().collect();
        if entries.is_empty() {
            return body;
        }
        body = body.mb_4().child(
            Button::new("desktop-recovery-toggle")
                .self_start()
                .label(format!(
                    "{} · {}",
                    if entries.len() == 1 {
                        "1 saved editing draft".to_owned()
                    } else {
                        format!("{} saved editing drafts", entries.len())
                    },
                    if self.recovery.expanded {
                        "Hide"
                    } else {
                        "Review"
                    }
                ))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.recovery.expanded = !this.recovery.expanded;
                    cx.notify();
                })),
        );
        if !self.recovery.expanded {
            return body;
        }
        body = body.child(heading_small("Retained editing drafts"));
        for entry in entries {
            let label = match &entry.draft.content {
                Content::Script { section, .. } => {
                    format!("{} · Script · {section}", entry.draft.title)
                }
                Content::Note { .. } => format!("{} · Review note", entry.draft.title),
                Content::Scene { draft, .. } => format!(
                    "{} · Scene {}",
                    entry.draft.title,
                    draft.unit.saturating_add(1)
                ),
                Content::Line { draft } => format!(
                    "{} · {} line {}",
                    entry.draft.title,
                    draft.language,
                    draft.unit.saturating_add(1)
                ),
            };
            let restore_id = entry.id.clone();
            let discard_id = entry.id.clone();
            let text = entry.draft.content.copy_text();
            body =
                body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_3()
                        .bg(p.surface)
                        .rounded_lg()
                        .border_1()
                        .border_color(p.border)
                        .child(label)
                        .child(muted(
                            &format!(
                                "{} · {}",
                                super::when(&entry.updated_at.to_string()),
                                super::home_relative(&entry.draft.library)
                            ),
                            p,
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    Button::new(gpui_kit::SharedString::from(format!(
                                        "desktop-recovery-restore-{}",
                                        entry.id
                                    )))
                                    .self_start()
                                    .label("Restore draft")
                                    .disabled(
                                        self.busy
                                            || self.has_unsaved_setup(cx)
                                            || self.recovery_pending(),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.restore_checkpoint(&restore_id, window, cx)
                                    })),
                                )
                                .child(
                                    Button::new(gpui_kit::SharedString::from(format!(
                                        "desktop-recovery-copy-{}",
                                        entry.id
                                    )))
                                    .self_start()
                                    .label("Copy text")
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                            text.clone(),
                                        ))
                                    })),
                                )
                                .child(
                                    Button::new(gpui_kit::SharedString::from(format!(
                                        "desktop-recovery-discard-{}",
                                        entry.id
                                    )))
                                    .self_start()
                                    .danger()
                                    .outline()
                                    .label("Discard draft")
                                    .disabled(self.recovery_pending())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.confirm_discard_checkpoint(
                                            discard_id.clone(),
                                            window,
                                            cx,
                                        );
                                    })),
                                ),
                        ),
                );
        }
        body
    }
}
