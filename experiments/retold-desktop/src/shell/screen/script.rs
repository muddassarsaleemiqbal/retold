//! Readable script sections, retained drafts and explicit saved-version conflict choices.
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::Textarea,
};
use gpui_kit::{Context, Div, Styled, div, prelude::*, px};

use super::{error_notice, heading_small, muted, notice};
use crate::shell::{Shell, editing::ScriptAction};

impl Shell {
    pub(super) fn retained_script_notice(&self, cx: &mut Context<Self>) -> Div {
        let Some(editor) = &self.editor else {
            return div();
        };
        let visible = self.destination == crate::shell::Destination::Videos && self.show_script;
        if !editor.dirty(cx) || visible {
            return div();
        }
        let p = self.palette();
        let title = editor
            .document
            .as_ref()
            .map_or("your video", |document| document.title.as_str());
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mb_4()
            .child(notice(
                &format!("Unsaved script text is kept for {title}."),
                p,
            ))
            .child(
                Button::new("desktop-return-script-draft")
                    .self_start()
                    .label("Return to script draft")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(editor) = &this.editor {
                            this.selected_video = Some(editor.reference.clone());
                            this.destination = crate::shell::Destination::Videos;
                            this.show_script = true;
                            this.show_lab = false;
                        }
                        cx.notify();
                    })),
            )
    }

    pub(super) fn script_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let body =
            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    Button::new("desktop-script-overview")
                        .self_start()
                        .ghost()
                        .icon(gpui_kit::assets::IconName::ChevronLeft)
                        .ml(gpui_kit::px(-10.))
                        .label("Back to video")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_script = false;
                            cx.notify();
                        })),
                )
                .child(super::page_header(
                    "Script",
                    "The narration every edition is made from. Earlier versions stay in history.",
                    [super::refresh_button(
                        "desktop-script-refresh",
                        "Check saved script",
                        self.busy,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.script_action(ScriptAction::Read, window, cx)
                    }))
                    .into_any_element()],
                    p,
                ));
        let Some(editor) = &self.editor else {
            return body;
        };
        let body = body
            .when_some(editor.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            })
            .child(self.script_conflict(cx));
        let Some(content) = editor.content() else {
            return body.child(muted(if self.busy { "Reading the saved script…" } else { "No saved script yet. Review a run through Script from the video overview to create one." }, p));
        };
        let dirty = editor.dirty(cx);
        let conflict = editor.incoming.is_some();
        body.child(if conflict { div() } else if content.approved && !dirty && !content.source_changed { super::success_notice("Approved. Review a run from the video when you're ready to rebuild.", p) } else { notice(if content.source_changed { "The source or guide changed. Undo these revisions before generating a new baseline, then reapply the text you want to keep." }
                else if dirty { "Unsaved changes. Save this section or discard changes before choosing another section." }
                else if content.approved { "Approved. A new run still needs your reviewed confirmation." }
                else { "Review required. Approve the current saved script when it is ready." }, p) })
            .child(div().flex().child(super::segmented(content.script.sections.iter().enumerate().map(|(index, section)| {
                Button::new(("desktop-script-section", index)).self_start().label(super::title_case(&section.section)).selected(index == editor.section)
                    .disabled(self.busy || dirty || editor.incoming.is_some())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(editor) = &mut this.editor {
                            editor.section = index;
                            editor.writing = None;
                            editor.compare = false;
                            editor.checkpoint = retold_app::notes::draft_id().ok();
                            editor.load_text(window, cx);
                        }
                        cx.notify();
                    }))
            }), p)))
            .child(div().flex().items_center().justify_between().gap_3()
                .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).text_size(gpui_kit::rems(1.15)).child(content.script.sections.get(editor.section).map_or_else(|| "Section".to_owned(), |section| super::title_case(&section.section))))
                .child(Button::new("desktop-script-comment").ghost().icon(gpui_kit::assets::IconName::Plus).label("Comment on this section").disabled(self.busy || dirty).on_click(cx.listener(|this, _, window, cx| this.comment_script(window, cx)))))
            .child(Textarea::new(&editor.text).aria_label("Script section text").h(px(200.)).disabled(self.busy || self.recovery.closing || self.recovery.quitting || !content.editable || content.source_changed))
            .child(self.checkpoint_status(editor.checkpoint.as_deref(), dirty))
            .when(dirty && editor.incoming.is_none(), |body| body.child(super::callout_info("Saving will rebuild scenes, pictures, voice and captions from this script. Research is kept.", p)))
            .child(self.script_actions(cx))
            .when(editor.incoming.is_none(), |body| body.child(self.writing_panel(cx)))
            .when(editor.compare, |body| body.child(heading_small("Saved version")).child(
                div().p_4().bg(p.surface).rounded_lg().child(editor.comparison_text())))
            .when(!content.problems.is_empty(), |body| body.child(heading_small("Before approval"))
                .children(content.problems.iter().map(|problem| notice(problem, p))))
            .child(self.script_history(cx))
    }

    fn script_actions(&self, cx: &mut Context<Self>) -> Div {
        let Some(editor) = &self.editor else {
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
                    Button::new("desktop-script-save")
                        .self_start()
                        .primary()
                        .label("Save section")
                        .disabled(blocked || !dirty || !content.editable || content.source_changed)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.script_action(ScriptAction::Save, window, cx)
                        })),
                )
            })
            .when(dirty, |row| {
                row.child(
                    Button::new("desktop-script-discard")
                        .self_start()
                        .label("Discard changes")
                        .disabled(self.busy || !dirty)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(editor) = &mut this.editor {
                                editor.load_text(window, cx);
                                editor.incoming = None;
                            }
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new("desktop-script-check-writing")
                    .icon(gpui_kit::assets::IconName::Sparkles)
                    .label("Check writing")
                    .tooltip("Look for clichés, weak verbs and rhythm problems")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.check_writing(cx))),
            )
            .child(
                Button::new("desktop-script-compare")
                    .self_start()
                    .label("Compare with saved")
                    .selected(editor.compare)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(editor) = &mut this.editor {
                            editor.compare = !editor.compare;
                        }
                        cx.notify();
                    })),
            )
            .when(!dirty && !content.approved, |row| {
                row.child(
                    Button::new("desktop-script-approve")
                        .self_start()
                        .primary()
                        .tooltip("Approve the saved script (⌘↩)")
                        .label("Approve script")
                        .disabled(
                            blocked
                                || dirty
                                || content.approved
                                || !content.editable
                                || content.source_changed
                                || !content.problems.is_empty(),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.script_action(ScriptAction::Approve, window, cx)
                        })),
                )
            })
            .when(!dirty && content.history.undo, |row| {
                row.child(
                    Button::new("desktop-script-undo")
                        .self_start()
                        .label("Undo revision")
                        .disabled(blocked || dirty || !content.history.undo)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.script_action(ScriptAction::Undo, window, cx)
                        })),
                )
            })
            .when(!dirty && content.history.redo, |row| {
                row.child(
                    Button::new("desktop-script-redo")
                        .self_start()
                        .label("Redo revision")
                        .disabled(
                            blocked || dirty || !content.history.redo || content.source_changed,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.script_action(ScriptAction::Redo, window, cx)
                        })),
                )
            })
    }

    fn script_conflict(&self, cx: &mut Context<Self>) -> Div {
        let Some(editor) = &self.editor else {
            return div();
        };
        if editor.incoming.is_none() {
            return div();
        }
        let p = self.palette();
        super::decision(
            "The saved script changed while you were editing. Keep your text, or switch to the saved version shown below.",
            [
                Button::new("desktop-script-use-saved")
                    .label("Use saved version")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(editor) = &mut this.editor
                            && editor.adopt_incoming()
                        {
                            editor.load_text(window, cx);
                        }
                        this.checkpoint_script(cx);
                        cx.notify();
                    })),
                Button::new("desktop-script-keep-local")
                    .primary()
                    .label("Keep my text")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(editor) = &mut this.editor {
                            editor.adopt_incoming();
                        }
                        this.checkpoint_script(cx);
                        cx.notify();
                    })),
            ],
            p,
        )
    }

    fn script_history(&self, cx: &mut Context<Self>) -> Div {
        let Some(editor) = &self.editor else {
            return div();
        };
        let p = self.palette();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("Revision history"))
            .child(
                Button::new("desktop-script-history")
                    .self_start()
                    .label(if editor.history_cursor.is_some() {
                        "Older revisions"
                    } else {
                        "Load revisions"
                    })
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.script_action(ScriptAction::History, window, cx)
                    })),
            )
            .when_some(editor.history.as_ref(), |body, history| {
                body.when(history.entries.is_empty(), |body| {
                    body.child(muted("No manual revisions yet.", p))
                })
                .children(history.entries.iter().map(|entry| {
                    muted(
                        &format!(
                            "{} · {}",
                            super::when(&entry.created_at.to_string()),
                            entry.section.as_deref().unwrap_or("Script approval")
                        ),
                        p,
                    )
                }))
            })
    }
}

impl Shell {
    /// The latest writing check: a score, the clichés to replace and what to try instead.
    fn writing_panel(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(check) = self
            .editor
            .as_ref()
            .and_then(|editor| editor.writing.as_ref())
        else {
            return div();
        };
        let tips: Vec<&String> = check
            .rhythm
            .iter()
            .chain(
                check
                    .advice
                    .iter()
                    .filter(|advice| !advice.starts_with("Replace banned")),
            )
            .collect();
        let clean = check.cliches.is_empty() && tips.is_empty();
        let stale = self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.text.read(cx).value().as_ref() != check.text.as_str());
        let tone = if check.score >= 80 && clean {
            p.success
        } else if check.score >= 50 {
            p.attention
        } else {
            p.error
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(muted(&format!("Writing check · {}", check.section), p))
                    .when(stale, |row| {
                        row.child(
                            Button::new("desktop-script-recheck")
                                .icon(gpui_kit::assets::IconName::RefreshCw)
                                .label("Check again")
                                .on_click(cx.listener(|this, _, _, cx| this.check_writing(cx))),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_1()
                    .child(count_up(
                        &format!("{}{}", check.section, check.text),
                        check.score,
                        div()
                            .text_size(gpui_kit::rems(1.7))
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(tone),
                    ))
                    .child(muted("/ 100", p))
                    .child(muted(
                        if check.score >= 90 {
                            " · Excellent"
                        } else if check.score >= 75 {
                            " · Good · aim for 90+"
                        } else if check.score >= 50 {
                            " · Needs work"
                        } else {
                            " · Rewrite suggested"
                        },
                        p,
                    )),
            )
            .child(muted(
                &format!(
                    "{} words · {} vivid verbs",
                    check.words, check.sensory_verbs
                ),
                p,
            ))
            .when(!check.cliches.is_empty(), |panel| {
                panel.child(muted("Replace these stock phrases:", p)).child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(check.cliches.iter().map(|phrase| {
                            div()
                                .px_2()
                                .py(gpui_kit::px(2.))
                                .rounded_md()
                                .bg(p.attention.opacity(0.14))
                                .text_color(p.text)
                                .child(format!("“{phrase}”"))
                        })),
                )
            })
            .children(tips.into_iter().map(|tip| {
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(
                        gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Lightbulb)
                            .size(gpui_kit::rems(1.1))
                            .text_color(p.attention),
                    )
                    .child(div().flex_1().child(tip.clone()))
            }))
            .when(clean, |panel| {
                panel.child(super::success_notice(
                    "Reads cleanly aloud. Nothing to fix.",
                    p,
                ))
            })
    }
}

/// The score counts up from zero over half a second the first time a result appears.
fn count_up(key: &str, value: u64, element: Div) -> gpui_kit::AnyElement {
    use gpui_kit::{Animation, AnimationExt as _};
    use std::hash::{Hash as _, Hasher as _};
    if !super::MOTION.load(std::sync::atomic::Ordering::Relaxed) {
        return element.child(value.to_string()).into_any_element();
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    element
        .with_animation(
            gpui_kit::ElementId::Name(format!("count-{}", hasher.finish()).into()),
            Animation::new(std::time::Duration::from_millis(500))
                .with_easing(gpui_kit::ease_out_quint()),
            move |element, progress| {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss,
                    reason = "a 0–100 score scaled by animation progress"
                )]
                let shown = (value as f32 * progress).round() as u64;
                element.child(shown.to_string())
            },
        )
        .into_any_element()
}
