//! Explicit delivery scope and a separate hashed confirmation.
use super::{error_notice, heading, heading_small, muted};
use crate::shell::Shell;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};

/// Export shapes offered natively; each choice is the exact format list the plan hash pins.
const FORMAT_CHOICES: [(&[&str], &str); 4] = [
    (&["16x9"], "Landscape 16:9"),
    (&["9x16"], "Portrait 9:16"),
    (&["1x1"], "Square 1:1"),
    (&["16x9", "9x16"], "Landscape and portrait"),
];

impl Shell {
    pub(super) fn delivery_page(&self, cx: &mut Context<Self>) -> Div {
        let Some(panel) = &self.delivery else {
            return div();
        };
        let p = self.palette();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(heading(&format!("Export {}", panel.title)))
            .child(muted(
                "Export packages a finished video. It does not create missing scenes or narration.",
                p,
            ))
            .child(heading_small("Edition"))
            .children(panel.editions.iter().enumerate().map(|(index, edition)| {
                let edition = edition.clone();
                Button::new(("desktop-export-edition", index))
                    .self_start()
                    .label(edition.label.clone())
                    .selected(panel.options.editions.contains(&edition.id))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(panel) = &mut this.delivery {
                            panel.options.editions = vec![edition.id.clone()];
                            panel.review = None;
                        }
                        cx.notify();
                    }))
            }))
            .child(heading_small("Format"))
            .child(
                div().flex().flex_wrap().gap_2().children(
                    FORMAT_CHOICES
                        .into_iter()
                        .enumerate()
                        .map(|(index, (formats, label))| {
                            let chosen: Vec<String> =
                                formats.iter().map(|format| (*format).to_owned()).collect();
                            let selected = panel.options.formats == chosen;
                            Button::new(("desktop-export-format", index))
                                .self_start()
                                .label(label)
                                .selected(selected)
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(panel) = &mut this.delivery {
                                        panel.options.formats.clone_from(&chosen);
                                        panel.review = None;
                                    }
                                    cx.notify();
                                }))
                        }),
                ),
            )
            .child(self.delivery_presets(cx))
            .child(heading_small("Output"))
            .child(
                div().flex().flex_wrap().gap_2().children(
                    [(true, "Editor project only"), (false, "Render video")]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (project_only, label))| {
                            Button::new(("desktop-export-output", index))
                                .self_start()
                                .label(label)
                                .selected(panel.options.project_only == project_only)
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(panel) = &mut this.delivery {
                                        panel.options.project_only = project_only;
                                        panel.review = None;
                                    }
                                    cx.notify();
                                }))
                        }),
                ),
            )
            .child(muted(
                "Files go to this video's deliverables folder, by edition and format.",
                p,
            ))
            .child(heading_small("Limits for this export"))
            .child(muted(
                "Spending limits start at zero. Set what you allow, then review.",
                p,
            ))
            .child(
                div()
                    .id("desktop-export-authority")
                    .test_support()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(muted("Spending cap (USD)", p))
                            .child(
                                Input::new(&panel.allowance)
                                    .aria_label("Export allowance USD")
                                    .disabled(self.busy),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(muted("Request cap", p))
                            .child(
                                Input::new(&panel.request_limit)
                                    .aria_label("Export maximum paid requests")
                                    .disabled(self.busy),
                            ),
                    ),
            )
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            })
            .child(
                Button::new("desktop-export-review")
                    .self_start()
                    .label("Review export plan")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.review_delivery(cx))),
            );
        if let Some(review) = &panel.review {
            let plan = review.plan();
            body = body
                .child(heading_small("Review this export"))
                .child(self.blockers(&plan.blockers, cx))
                .child(muted(
                    &format!(
                        "Maximum authorized for this export: {} · {} paid requests",
                        super::run::usd(plan.authority.usd_micros),
                        plan.authority.requests
                    ),
                    p,
                ))
                .when(!plan.authority.routes.is_empty(), |body| {
                    body.child(heading_small("Provider accounts"))
                })
                .children(plan.authority.routes.iter().map(|(route, account)| {
                    muted(
                        &format!("{route}: bills to {}", super::run::account_label(account)),
                        p,
                    )
                }))
                .child(heading_small("Prepared deliveries"))
                .when(review.available().is_empty(), |body| {
                    body.child(muted(
                        "No finished files yet. Finish the video, then export.",
                        p,
                    ))
                })
                .children(review.available().iter().map(|delivery| {
                    muted(
                        &format!(
                            "{} · {}",
                            retold_app::desktop::edition_label(&delivery.edition),
                            delivery.format.replace('x', ":")
                        ),
                        p,
                    )
                }))
                .child(muted(
                    &format!(
                        "Saves to {}",
                        super::middle_truncated(
                            &super::home_relative(&review.output_directory()),
                            72
                        )
                    ),
                    p,
                ))
                .when(plan.is_unblocked(), |body| {
                    body.child(
                        Button::new("desktop-export-confirm")
                            .self_start()
                            .primary()
                            .label(if panel.options.project_only {
                                "Confirm project export"
                            } else {
                                "Confirm video render"
                            })
                            .tooltip("Starts the export; Activity shows when the files are ready")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.submit_delivery(cx))),
                    )
                });
        }
        body.child(
            Button::new("desktop-export-cancel")
                .self_start()
                .label("Back to video")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.delivery = None;
                    cx.notify();
                })),
        )
    }
}
