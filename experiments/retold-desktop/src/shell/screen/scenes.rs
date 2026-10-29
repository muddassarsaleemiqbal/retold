//! Scene selection, the recorded frame and a reviewed visual edit share one bounded workspace.
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, Textarea},
};
use gpui_kit::{Context, Div, div, prelude::*, px};

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{Destination, Shell, scenes::SceneAction};

impl Shell {
    pub(super) fn retained_scene_notice(&self, cx: &mut Context<Self>) -> Div {
        let Some(editor) = &self.scenes else {
            return div();
        };
        if !editor.dirty(cx) || (self.destination == Destination::Videos && self.show_scenes) {
            return div();
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mb_4()
            .child(notice("Unsaved scene direction is kept.", self.palette()))
            .child(
                Button::new("desktop-return-scene-draft")
                    .self_start()
                    .label("Return to scene draft")
                    .on_click(cx.listener(|this, _, _, cx| this.return_to_scenes(cx))),
            )
    }

    pub(super) fn scene_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                Button::new("desktop-scenes-overview")
                    .self_start()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .ml(gpui_kit::px(-10.))
                    .label("Back to video")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_scenes = false;
                        cx.notify();
                    })),
            )
            .child(super::page_header(
                "Scenes",
                "Direct each scene, then approve the sequence.",
                [super::refresh_button(
                    "desktop-scenes-refresh",
                    "Check saved sequence",
                    self.busy,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.scene_action(SceneAction::Read, window, cx)
                }))
                .into_any_element()],
                p,
            ));
        let Some(editor) = &self.scenes else {
            return body;
        };
        let body = body.when_some(editor.error.as_ref(), |body, error| {
            body.child(super::decision(
                error,
                [Button::new("desktop-scenes-reload")
                    .label("Show latest scenes")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.scene_action(SceneAction::Read, window, cx)
                    }))],
                p,
            ))
        });
        let body = body.when_some(
            editor.bounds_error(cx).filter(|_| editor.dirty(cx)),
            |body, error| body.child(error_notice(error, p)),
        );
        let Some(content) = editor.content() else {
            return body
                .child(muted(
                    if self.busy {
                        "Reading saved scenes…"
                    } else {
                        "No saved scene sequence yet."
                    },
                    p,
                ))
                .child(self.prepare_scene_button(cx));
        };
        let dirty = editor.dirty(cx);
        let mut body = body.when(!content.editable, |body| body.child(notice("This sequence predates versioned scene editing. Review a run through Scenes to establish its original baseline.", p)).child(self.prepare_scene_button(cx)))
            .when(content.source_changed, |body| body.child(notice("The source, script or generated sequence changed. Keep any local text, undo manual scene revisions, then review a run through Scenes before reapplying direction.", p)))
            .child(self.scene_selection(cx));
        let Some(unit) = content
            .board
            .units
            .iter()
            .find(|unit| unit.id == editor.unit)
        else {
            return body;
        };
        let Some(shot) = content
            .board
            .direction
            .shots
            .iter()
            .find(|shot| shot.unit == editor.unit)
        else {
            return body;
        };
        body = body.child(div().flex().gap_4().when(self.compact_layout, |body| body.flex_col()).when(!self.compact_layout, |body| body.flex_wrap())
            .child(div().flex().flex_col().gap_3().min_w(px(280.)).flex_1()
                .child(heading_small(&format!("Scene {} · {}", unit.id.saturating_add(1), unit.section)))
                .child(muted(&format!("About {:.1} seconds · {}", unit.estimated_seconds, shot.shot), p))
                .when(editor.frame_available == Some(true), |body| body.child(div().min_h(px(360.)).child(editor.player.clone())))
                .when(editor.frame_available.is_none(), |body| body.child(muted("Reading recorded frame…", p)))
                .when(editor.frame_available == Some(false), |body| body.child(muted("No recorded frame is available for this scene yet.", p)))
                .child(muted("Saving new direction redraws this frame on the next run.", p))
                .when(editor.frame_available == Some(true), |body| body.child(self.retake_controls(retold_app::retake::Retake::Frame(editor.unit), "Redo this frame", cx)))
                .child(heading_small("Narration")).child(unit.narration.clone())
                .child(self.scene_cast(content.editable && !content.source_changed, cx)))
            .child(div().flex().flex_col().gap_3().min_w(px(280.)).flex_1()
                .child(heading_small("Visual direction"))
                .child(self.checkpoint_status(editor.checkpoint.as_deref(), dirty))
                .child(muted("Action", p)).child(Textarea::new(&editor.action).aria_label("Scene action").h(px(160.)).disabled(self.busy || self.recovery.closing || self.recovery.quitting || !content.editable || content.source_changed))
                .child(muted("Location", p)).child(Input::new(&editor.location).aria_label("Scene location").disabled(self.busy || self.recovery.closing || self.recovery.quitting || !content.editable || content.source_changed))
                .child(muted("Mood", p)).child(Input::new(&editor.mood).aria_label("Scene mood").disabled(self.busy || self.recovery.closing || self.recovery.quitting || !content.editable || content.source_changed))
                .when(editor.incoming.is_some(), |body| body.child(self.scene_conflict(cx)))
                .when(dirty, |body| body.child(notice("Review affected scenes before saving. Existing narration, research and character sheets remain available.", p)))
                .child(self.scene_actions(cx))));
        body.when_some(editor.review.as_ref(), |body, review| {
            let affected: std::collections::BTreeSet<_> = review.impact.changed_units.iter().chain(&review.impact.reused_dependents).chain(&review.impact.panel_dependents).copied().collect();
            let units: Vec<_> = affected.into_iter().map(|unit| unit.saturating_add(1).to_string()).collect();
            let stages: Vec<_> = [("images", "Frames"), ("depth", "Depth and motion"), ("mouths", "Lip sync"), ("shots", "Scene clips"), ("edit", "Timelines"), ("deliver", "Deliveries"), ("metrics", "Quality reports")].into_iter().filter(|(stage, _)| review.impact.affected_stages.iter().any(|affected| affected == stage)).map(|(_, label)| label).collect();
            body.child(heading_small("Save review"))
                .child(notice(&format!("Scenes affected: {}. Rebuild: {}.", units.join(", "), stages.join(", ")), p))
                .when(!review.impact.panel_dependents.is_empty(), |body| body.child(notice("Neighboring scenes share image sheets whose grouping changes. Their frames may need rebuilding too.", p)))
                .when(review.impact.detaches_reuse, |body| body.child(notice("This scene will receive its own frame instead of recutting an earlier image.", p)))
                .child(muted("Saving asks you to approve the sequence again.", p))
                .child(Button::new("desktop-scenes-confirm-save").self_start().primary().label("Confirm direction change").disabled(self.busy || review.impact.changed_units.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| this.scene_action(SceneAction::Save, window, cx))))
        }).when(!content.problems.is_empty(), |body| body.child(heading_small("Before approval")).children(content.problems.iter().map(|problem| notice(problem, p))))
            .child(self.scene_history(cx))
    }

    fn prepare_scene_button(&self, cx: &mut Context<Self>) -> Div {
        div().flex().child(
            Button::new("desktop-scenes-prepare")
                .self_start()
                .label("Review run through Scenes")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_scenes = false;
                    this.review_target(Some("storyboard"), cx);
                })),
        )
    }

    fn scene_selection(&self, cx: &mut Context<Self>) -> Div {
        let Some(editor) = &self.scenes else {
            return div();
        };
        let Some(content) = editor.content() else {
            return div();
        };
        let units = &content.board.units;
        let index = units
            .iter()
            .position(|unit| unit.id == editor.unit)
            .unwrap_or(0);
        let previous = index
            .checked_sub(1)
            .and_then(|index| units.get(index))
            .map(|unit| unit.id);
        let next = units.get(index.saturating_add(1)).map(|unit| unit.id);
        let blocked = self.busy || editor.dirty(cx) || editor.incoming.is_some();
        let navigation = div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new("desktop-scene-previous")
                    .ghost()
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .tooltip("Previous scene (⌘[)")
                    .accessibility_label("Previous scene")
                    .disabled(blocked || previous.is_none())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(unit) = previous {
                            this.select_scene(unit, window, cx);
                        }
                    })),
            )
            .child(
                Button::new("desktop-scene-picker")
                    .icon(gpui_kit::assets::IconName::ChevronDown)
                    .label(format!(
                        "Scene {} of {}",
                        index.saturating_add(1),
                        units.len()
                    ))
                    .disabled(blocked)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.show_scene_picker(window, cx)),
                    ),
            )
            .child(
                Button::new("desktop-scene-next")
                    .ghost()
                    .icon(gpui_kit::assets::IconName::ChevronRight)
                    .tooltip("Next scene (⌘])")
                    .accessibility_label("Next scene")
                    .disabled(blocked || next.is_none())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(unit) = next {
                            this.select_scene(unit, window, cx);
                        }
                    })),
            );
        let frame_ready = editor.frame_target.is_some() && editor.player.read(cx).image_loaded();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_2()
            .child(navigation)
            .when(!blocked, |row| {
                row.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("desktop-scene-comment")
                                .label("Comment on direction")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.comment_scene(false, window, cx)
                                })),
                        )
                        .child(
                            Button::new("desktop-frame-comment")
                                .disabled(!frame_ready)
                                .tooltip(if frame_ready {
                                    "Comment on this frame"
                                } else {
                                    "Available once the frame loads"
                                })
                                .label("Comment on frame")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.comment_scene(true, window, cx)
                                })),
                        ),
                )
            })
    }

    fn scene_actions(&self, cx: &mut Context<Self>) -> Div {
        let Some(editor) = &self.scenes else {
            return div();
        };
        let Some(content) = editor.content() else {
            return div();
        };
        let dirty = editor.dirty(cx);
        let blocked = self.busy || editor.incoming.is_some();
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .when(dirty, |row| {
                row.child(
                    Button::new("desktop-scenes-review-edit")
                        .self_start()
                        .primary()
                        .label("Review direction change")
                        .disabled(
                            blocked
                                || !dirty
                                || !content.editable
                                || content.source_changed
                                || editor.bounds_error(cx).is_some(),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.scene_action(SceneAction::Review, window, cx)
                        })),
                )
            })
            .when(dirty, |row| {
                row.child(
                    Button::new("desktop-scenes-discard")
                        .self_start()
                        .label("Discard changes")
                        .disabled(self.busy || !dirty)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(editor) = &mut this.scenes {
                                if editor.incoming.is_some() {
                                    editor.adopt_incoming();
                                }
                                editor.load_fields(window, cx);
                                editor.review = None;
                            }
                            cx.notify();
                        })),
                )
            })
            .when(!dirty && !content.approved, |row| {
                row.child(
                    Button::new("desktop-scenes-approve")
                        .self_start()
                        .primary()
                        .label("Approve saved direction")
                        .disabled(
                            blocked
                                || dirty
                                || !content.editable
                                || content.source_changed
                                || content.approved
                                || !content.problems.is_empty(),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.scene_action(SceneAction::Approve, window, cx)
                        })),
                )
            })
            .when(!dirty && content.history.undo, |row| {
                row.child(
                    Button::new("desktop-scenes-undo")
                        .self_start()
                        .label("Undo revision")
                        .disabled(blocked || dirty || !content.history.undo)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.scene_action(SceneAction::Undo, window, cx)
                        })),
                )
            })
            .when(!dirty && content.history.redo, |row| {
                row.child(
                    Button::new("desktop-scenes-redo")
                        .self_start()
                        .label("Redo revision")
                        .disabled(
                            blocked || dirty || content.source_changed || !content.history.redo,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.scene_action(SceneAction::Redo, window, cx)
                        })),
                )
            })
            .when(!dirty && content.approved, |row| {
                let p = self.palette();
                row.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_color(p.success)
                        .child(
                            gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Check)
                                .size(gpui_kit::rems(1.)),
                        )
                        .child("Direction approved"),
                )
            })
    }

    fn scene_conflict(&self, cx: &mut Context<Self>) -> Div {
        let Some(editor) = &self.scenes else {
            return div();
        };
        let saved = editor
            .incoming
            .as_ref()
            .and_then(|document| document.content.as_ref())
            .and_then(|content| {
                content
                    .board
                    .direction
                    .shots
                    .iter()
                    .find(|shot| shot.unit == editor.unit)
            });
        let p = self.palette();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(super::decision(
                "This scene changed while you were editing. Compare the saved direction below, then choose which to keep.",
                [
                    Button::new("desktop-scenes-use-saved")
                        .label("Use saved direction")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(editor) = &mut this.scenes
                                && editor.adopt_incoming()
                            {
                                editor.load_fields(window, cx);
                            }
                            cx.notify();
                        })),
                    Button::new("desktop-scenes-keep-local")
                        .primary()
                        .label("Keep my direction")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(editor) = &mut this.scenes {
                                editor.adopt_incoming();
                            }
                            cx.notify();
                        })),
                ],
                p,
            ))
            .when_some(saved, |body, shot| {
                body.child(heading_small("Latest saved direction"))
                    .child(shot.action.clone())
                    .child(muted(&format!("{} · {}", shot.location, shot.mood), p))
            })
    }

    fn scene_history(&self, cx: &mut Context<Self>) -> Div {
        let Some(editor) = &self.scenes else {
            return div();
        };
        let p = self.palette();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Direction history"))
            .child(
                Button::new("desktop-scenes-history")
                    .self_start()
                    .label(if editor.history_cursor.is_some() {
                        "Older revisions"
                    } else {
                        "Load revisions"
                    })
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.scene_action(SceneAction::History, window, cx)
                    })),
            )
            .when_some(editor.history.as_ref(), |body, history| {
                body.children(history.entries.iter().map(|entry| {
                    muted(
                        &format!(
                            "{} · {}",
                            super::when(&entry.created_at.to_string()),
                            entry.section.as_deref().unwrap_or("Sequence approval")
                        ),
                        p,
                    )
                }))
            })
    }
}
