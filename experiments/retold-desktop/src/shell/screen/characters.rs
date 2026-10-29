//! Actual saved character sheets, explicit current-content approval and a fresh-run return.
use std::sync::Arc;

use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, ObjectFit, div, img, prelude::*, px};

use super::{heading_small, muted, notice, page_header, refresh_button};
use crate::shell::{Shell, characters::CharacterAction};

impl Shell {
    pub(super) fn character_page(&self, cx: &mut Context<Self>) -> Div {
        let Some(panel) = &self.characters else {
            return div();
        };
        let p = self.palette();
        let selected = panel
            .document
            .as_ref()
            .and_then(|document| document.selected.as_ref());
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                Button::new("desktop-characters-back")
                    .self_start()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .ml(gpui_kit::px(-10.))
                    .label("Back to video")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.characters = None;
                        this.review = None;
                        cx.notify();
                    })),
            )
            .child(page_header(
                "Characters",
                "Approve each character's look before scenes are drawn.",
                [
                    refresh_button("desktop-characters-refresh", "Reload sheets", self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.character_action(CharacterAction::Read(None), cx)
                        }))
                        .into_any_element(),
                ],
                p,
            ))
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(super::decision(
                    error,
                    [Button::new("desktop-characters-reload")
                        .primary()
                        .label("Load current sheet")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.character_action(CharacterAction::Read(None), cx)
                        }))],
                    p,
                ))
            });
        let Some(document) = &panel.document else {
            if self.busy {
                return body.child(super::skeleton(3, p));
            }
            return body.child(muted("No sheets loaded. Reload to try again.", p));
        };
        let approved = document
            .sheets
            .iter()
            .filter(|sheet| sheet.approved)
            .count();
        body = body.child(muted(
            &format!(
                "{} · {approved} of {} characters approved",
                document.title,
                document.sheets.len()
            ),
            p,
        ));
        if document.sheets.is_empty() {
            return body.child(notice("No character sheets saved yet. Review and confirm a run from the video to prepare them.", p));
        }
        body = body.child(div().flex().flex_wrap().gap_2().children(
            document.sheets.iter().enumerate().map(|(index, sheet)| {
                let name = sheet.name.clone();
                Button::new(("desktop-character", index))
                    .self_start()
                    .when(sheet.approved, |chip| {
                        chip.icon(gpui_kit::assets::IconName::Check)
                    })
                    .label(sheet.name.clone())
                    .accessibility_label(format!(
                        "{}, {}",
                        sheet.name,
                        if sheet.approved {
                            "approved"
                        } else {
                            "needs review"
                        }
                    ))
                    .selected(selected.is_some_and(|selected| selected.sheet.name == sheet.name))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.character_action(CharacterAction::Read(Some(name.clone())), cx)
                    }))
            }),
        ));
        let more_to_review = selected.is_some_and(|selected| {
            document
                .sheets
                .iter()
                .any(|sheet| !sheet.approved && sheet.name != selected.sheet.name)
        });
        if let Some(selected) = selected {
            body = body.child(heading_small(&selected.sheet.name)).when(
                panel.error.is_none(),
                |body| {
                    body.child(
                        Button::new("desktop-character-comment")
                            .self_start()
                            .label("Comment on this sheet")
                            .disabled(self.busy || panel.frame.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.comment_character(window, cx)
                            })),
                    )
                },
            );
            if let Some(frame) = &panel.frame {
                body = body.child(
                    div()
                        .h(px(360.))
                        .w_full()
                        .bg(p.surface)
                        .rounded_lg()
                        .overflow_hidden()
                        .child(
                            img(Arc::clone(frame))
                                .size_full()
                                .object_fit(ObjectFit::Contain),
                        ),
                );
                body = body
                    .child(if selected.sheet.approved {
                        super::success_notice("Approved. A changed sheet needs another review.", p)
                    } else {
                        muted(
                            &format!(
                                "Approve to lock {}'s look for every scene.",
                                selected.sheet.name
                            ),
                            p,
                        )
                    })
                    .child(
                        Button::new("desktop-character-approve")
                            .self_start()
                            .primary()
                            .label(if more_to_review {
                                "Approve and next"
                            } else {
                                "Approve"
                            })
                            .accessibility_label("Approve sheet")
                            .tooltip("Approve this look (⌘↩)")
                            .disabled(self.busy || selected.sheet.approved || panel.error.is_some())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.character_action(CharacterAction::Approve, cx)
                            })),
                    );
            } else {
                body = if self.busy {
                    body.child(super::skeleton(1, p))
                } else if panel.error.is_some() {
                    body.child(muted(
                        &format!("Load the current sheet to review {}.", selected.sheet.name),
                        p,
                    ))
                } else {
                    let name = selected.sheet.name.clone();
                    body.child(super::decision(
                        "This image couldn't load. Reload it to review and approve.",
                        [Button::new("desktop-character-reload-image")
                            .label("Reload image")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.character_action(CharacterAction::Read(Some(name.clone())), cx)
                            }))],
                        p,
                    ))
                };
            }
        }
        body
    }
}
