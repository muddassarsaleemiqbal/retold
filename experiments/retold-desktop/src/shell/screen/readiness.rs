//! The Home setup checklist: each prerequisite with its state and the one action that fixes it.
use gpui_kit::component::{
    Disableable as _, Icon,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, FontWeight, div, prelude::*, px, relative};
use retold_app::desktop::readiness::Item;

use super::{error_notice, muted, notice};
use crate::shell::{Destination, Shell, readiness::ReadinessTask};

/// What a missing step needs the user to do.
#[derive(Clone, Copy)]
enum Fix {
    Providers,
    Account,
    ExampleGuide,
}

fn fix_for(item: &Item) -> Option<Fix> {
    let label = item.label.to_lowercase();
    if label.contains("provider") {
        Some(Fix::Providers)
    } else if label.contains("account") {
        Some(Fix::Account)
    } else if label.contains("guide") || label.contains("channel") {
        Some(Fix::ExampleGuide)
    } else {
        None
    }
}

fn fix_label(fix: Fix) -> &'static str {
    match fix {
        Fix::Providers => "Connect an AI",
        Fix::Account => "Set up spending",
        Fix::ExampleGuide => "Add the example guide",
    }
}

/// One plain-language setup step built from the detailed checks.
struct Step {
    title: &'static str,
    ready: bool,
    detail: String,
    fix: Option<Fix>,
}

fn is_folder(item: &Item) -> bool {
    item.detail.starts_with("Directory")
}

/// Fold the doctor's detailed checks into four steps a person can act on.
fn steps(items: &[Item]) -> Vec<Step> {
    let group = |title: &'static str, fix: Option<Fix>, pick: &dyn Fn(&Item) -> bool| {
        let members: Vec<&Item> = items
            .iter()
            .filter(|item| !is_folder(item) && pick(item))
            .collect();
        let missing = members.iter().find(|item| !item.ready);
        Step {
            title,
            ready: missing.is_none(),
            detail: missing.map(|item| item.detail.clone()).unwrap_or_default(),
            fix: missing.and(fix),
        }
    };
    let kind = |item: &Item| fix_for(item);
    vec![
        group("Video tools", None, &|item| kind(item).is_none()),
        group("Guide and channel", Some(Fix::ExampleGuide), &|item| {
            matches!(kind(item), Some(Fix::ExampleGuide))
        }),
        group("Writing providers", Some(Fix::Providers), &|item| {
            matches!(kind(item), Some(Fix::Providers))
        }),
        group("Spending account", Some(Fix::Account), &|item| {
            matches!(kind(item), Some(Fix::Account))
        }),
    ]
}

impl Shell {
    pub(super) fn readiness(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.readiness;
        let Some(items) = panel.items.as_ref() else {
            return div()
                .flex()
                .flex_col()
                .gap_2()
                .child(super::heading_small("Get set up"))
                .child(super::skeleton(3, p));
        };
        let steps = steps(items);
        let total = steps.len();
        let first_missing = steps
            .iter()
            .position(|step| !step.ready && step.fix.is_some());
        let done = steps.iter().filter(|step| step.ready).count();
        if total > 0 && done == total {
            return super::decision(
                "You're all set. Make your first recap.",
                [Button::new("desktop-setup-new-video")
                    .primary()
                    .icon(gpui_kit::assets::IconName::Plus)
                    .label("New video")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.begin_draft(window, cx)))],
                p,
            );
        }
        #[expect(clippy::cast_precision_loss, reason = "a short checklist")]
        let progress = if total == 0 {
            0.
        } else {
            done as f32 / total as f32
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
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(gpui_kit::rems(1.15))
                                    .child("Get set up"),
                            )
                            .child(muted(&format!("{done} of {total} ready"), p)),
                    )
                    .child(
                        super::refresh_button("desktop-readiness-check", "Check again", self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.readiness_task(ReadinessTask::Check, cx)
                            })),
                    ),
            )
            .child(
                div().h(px(6.)).w_full().rounded_full().bg(p.border).child(
                    div()
                        .h_full()
                        .w(relative(progress))
                        .rounded_full()
                        .bg(p.accent_fill),
                ),
            )
            .children(steps.iter().enumerate().map(|(index, step)| {
                self.setup_step(index, step, Some(index) == first_missing, cx)
            }))
            .when_some(panel.example_preview.as_ref(), |body, preview| {
                body.child(super::decision(
                    preview,
                    [Button::new("desktop-example-copy")
                        .primary()
                        .label("Copy into this workspace")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.readiness_task(ReadinessTask::CopyExample, cx)
                        }))],
                    p,
                ))
            })
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            })
            .when_some(panel.message.as_ref(), |body, message| {
                body.child(notice(message, p))
            })
    }

    fn setup_step(&self, index: usize, item: &Step, next: bool, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let fix = item.fix;
        div()
            .flex()
            .items_center()
            .gap_3()
            .py_1()
            .child(if item.ready {
                Icon::new(gpui_kit::assets::IconName::Check)
                    .size(gpui_kit::rems(1.1))
                    .text_color(p.success)
                    .into_any_element()
            } else {
                div()
                    .size(px(14.))
                    .rounded_full()
                    .border_2()
                    .border_color(p.secondary)
                    .into_any_element()
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .when(item.ready, |label| label.text_color(p.secondary))
                            .child(item.title),
                    )
                    .when(!item.ready, |body| body.child(muted(&item.detail, p))),
            )
            .when_some(fix, |row, fix| {
                row.child(
                    Button::new(("desktop-setup-fix", index))
                        .when(next, |button| button.primary())
                        .label(fix_label(fix))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| match fix {
                            Fix::Providers => {
                                this.go_to(Destination::Providers, cx);
                                cx.notify();
                            }
                            Fix::Account => {
                                this.go_to(Destination::Budget, cx);
                                cx.notify();
                            }
                            Fix::ExampleGuide => {
                                this.readiness_task(ReadinessTask::PreviewExample, cx);
                            }
                        })),
                )
            })
    }
}
