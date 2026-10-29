//! One edition's spoken lines with the generated original beside the saved text.
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::Textarea,
};
use gpui_kit::{Context, Div, div, prelude::*, px};

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{Shell, lines::LineAction};

impl Shell {
    pub(super) fn lines_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(panel) = &self.lines else {
            return div();
        };
        let dirty = panel.dirty(cx);
        let body = div().flex().flex_col().gap_4()
            .child(Button::new("desktop-lines-back").self_start().ghost().icon(gpui_kit::assets::IconName::ChevronLeft).ml(gpui_kit::px(-10.)).label("Back to video").disabled(self.busy || dirty)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.lines = None;
                    cx.notify();
                })))
            .child(super::page_header("Language versions", "Edit what each language version says. Approved edits rebuild its voice, captions and timeline; pictures stay as they are.", [], p))
            .when_some(panel.error.as_ref(), |body, error| body.child(error_notice(error, p)));
        let Some(document) = &panel.document else {
            return body.child(muted(
                if self.busy {
                    "Reading edition lines…"
                } else {
                    "No lines loaded."
                },
                p,
            ));
        };
        let tabs = document
            .languages
            .iter()
            .enumerate()
            .map(|(index, language)| {
                let chosen = language.clone();
                let selected = document
                    .content
                    .as_ref()
                    .is_some_and(|content| &content.language == language);
                Button::new(("desktop-lines-language", index))
                    .self_start()
                    .label(retold_app::desktop::edition_label(language))
                    .selected(selected)
                    .disabled(self.busy || dirty)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.line_action(LineAction::Read(Some(chosen.clone())), window, cx)
                    }))
            })
            .collect::<Vec<_>>();
        let body = body.child(div().flex().flex_wrap().gap_2().children(tabs));
        let Some(content) = &document.content else {
            return body
                .child(muted("Only the original narration exists. Edit it in Script; translations appear here once you record narration in another language.", p))
                .child(Button::new("desktop-lines-open-script").self_start().label("Edit narration in Script").disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.lines = None;
                        this.open_script(window, cx);
                    })));
        };
        let picker = content
            .lines
            .iter()
            .map(|line| {
                let unit = line.unit;
                let label = format!(
                    "{}{}",
                    unit.saturating_add(1),
                    if line.edited { " •" } else { "" }
                );
                Button::new(("desktop-lines-unit", unit))
                    .self_start()
                    .label(label)
                    .selected(unit == panel.unit)
                    .disabled(self.busy || dirty)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.select_line(unit, window, cx)),
                    )
            })
            .collect::<Vec<_>>();
        let body = body
            .when(!content.approved, |body| {
                body.child(notice(
                    "Edited lines are waiting for approval of their current text.",
                    p,
                ))
            })
            .child(div().flex().flex_wrap().gap_1().children(picker));
        let Some(line) = panel.line() else {
            return body;
        };
        body.child(heading_small(&format!(
            "Line {} · {}",
            line.unit.saturating_add(1),
            line.section
        )))
        .child(muted("Generated original", p))
        .child(line.generated.clone())
        .child(muted("Spoken text", p))
        .child(
            Textarea::new(&panel.text)
                .aria_label("Edition line")
                .h(px(120.))
                .disabled(self.busy),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("desktop-lines-review")
                        .self_start()
                        .label("Review line change")
                        .disabled(self.busy || !dirty)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.line_action(LineAction::Review, window, cx)
                        })),
                )
                .child(
                    Button::new("desktop-lines-discard")
                        .self_start()
                        .ghost()
                        .label("Discard")
                        .disabled(self.busy || !dirty)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(panel) = &this.lines {
                                panel.load_text(window, cx);
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("desktop-lines-approve")
                        .self_start()
                        .label("Approve edition lines")
                        .disabled(self.busy || dirty || content.approved)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.line_action(LineAction::Approve, window, cx)
                        })),
                ),
        )
        .when_some(panel.review.as_ref(), |body, review| {
            body.child(notice(
                &format!(
                    "{} Rebuild: {}. Saving clears this edition's approval.",
                    if review.impact.restores_generated {
                        "Restores the generated line."
                    } else {
                        "Replaces the generated line."
                    },
                    review.impact.affected_stages.join(", ")
                ),
                p,
            ))
            .child(
                Button::new("desktop-lines-confirm")
                    .self_start()
                    .primary()
                    .label("Confirm line change")
                    .disabled(self.busy || review.impact.changed_units.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.line_action(LineAction::Save, window, cx)
                    })),
            )
        })
    }
}
