//! Cast membership and expressions for one scene, reviewed before publication.
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};

use super::{heading_small, muted, notice};
use crate::shell::{Shell, scenes::SceneAction, scenes::cast::CastChange};

impl Shell {
    pub(super) fn scene_cast(&self, editable: bool, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(editor) = &self.scenes else {
            return div();
        };
        let locked = self.busy || !editable || self.recovery.closing || self.recovery.quitting;
        let cast = editor.shown_cast();
        let dirty = editor.cast_dirty();
        let rows = cast.into_iter().enumerate().map(|(index, pose)| {
            let (cycle, remove) = (pose.name.clone(), pose.name.clone());
            let single = editor.expressions(&pose.name).len() < 2;
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(pose.name.clone())
                .child(
                    Button::new(("desktop-cast-expression", index))
                        .self_start()
                        .label(if pose.expression.is_empty() {
                            "Choose expression".to_owned()
                        } else {
                            format!("Expression: {}", pose.expression)
                        })
                        .disabled(locked || single)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_cast(CastChange::NextExpression(cycle.clone()), cx);
                        })),
                )
                .child(
                    Button::new(("desktop-cast-remove", index))
                        .self_start()
                        .ghost()
                        .label(format!("Remove {}", pose.name))
                        .disabled(locked)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_cast(CastChange::Remove(remove.clone()), cx);
                        })),
                )
        });
        let additions = editor
            .available_cast()
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let added = name.clone();
                Button::new(("desktop-cast-add", index))
                    .self_start()
                    .label(format!("Add {name}"))
                    .disabled(locked)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change_cast(CastChange::Add(added.clone()), cx);
                    }))
            })
            .collect::<Vec<_>>();
        let has_profiles = editor
            .content()
            .is_some_and(|content| !content.board.plan.cast.is_empty());
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Cast"))
            .when(editor.shown_cast().is_empty(), |body| {
                body.child(muted("No characters appear in this scene.", p))
            })
            .children(rows)
            .when(!additions.is_empty(), |body| {
                body.child(div().flex().flex_wrap().gap_2().children(additions))
            })
            .when(!has_profiles, |body| {
                body.child(muted(
                    "This video has no character profiles, so its scenes cannot be recast.",
                    p,
                ))
            })
            .when(dirty, |body| {
                body.child(notice(
                    "Recasting redraws this scene's frame. Narration and voices stay as saved.",
                    p,
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("desktop-cast-review")
                                .self_start()
                                .label("Review cast change")
                                .disabled(locked)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.scene_action(SceneAction::ReviewCast, window, cx);
                                })),
                        )
                        .child(
                            Button::new("desktop-cast-discard")
                                .self_start()
                                .ghost()
                                .label("Discard cast change")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.change_cast(CastChange::Discard, cx);
                                })),
                        ),
                )
            })
            .when_some(editor.cast_review.as_ref(), |body, review| {
                let units: Vec<_> = review
                    .impact
                    .changed_units
                    .iter()
                    .chain(&review.impact.reused_dependents)
                    .chain(&review.impact.panel_dependents)
                    .map(|unit| unit.saturating_add(1).to_string())
                    .collect();
                body.child(notice(
                    &format!(
                        "Frames to redraw: scenes {}. Saving clears sequence approval.",
                        units.join(", ")
                    ),
                    p,
                ))
                .child(
                    Button::new("desktop-cast-confirm")
                        .self_start()
                        .primary()
                        .label("Confirm cast change")
                        .disabled(self.busy || review.impact.changed_units.is_empty())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.scene_action(SceneAction::SaveCast, window, cx);
                        })),
                )
            })
    }
}
