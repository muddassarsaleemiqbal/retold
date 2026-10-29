//! Series list, part matrix with linked-video progress, linking and reviewed subset runs.
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, div, prelude::*};

use super::{empty_state, error_notice, heading_small, muted, notice, page_header, refresh_button};
use crate::shell::{Shell, series::SeriesTask};
use gpui_kit::assets::IconName;

impl Shell {
    pub(super) fn series_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.series;
        let body = div().flex().flex_col().gap_4()
            .child(page_header("Series", "Link each part to a video, choose the parts to make, and review one plan for all of them.", [
                refresh_button("desktop-series-refresh", "Refresh series", self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.series_task(SeriesTask::List, cx))).into_any_element(),
                self.new_series_button(cx).into_any_element(),
            ], p))
            .when_some(panel.error.as_ref(), |body, error| body.child(error_notice(error, p)))
            .when_some(panel.message.as_ref(), |body, message| body.child(notice(message, p)))
            .child(self.new_series_form(cx));
        let Some(list) = &panel.list else {
            return body.child(super::skeleton(4, p));
        };
        if list.is_empty() {
            return body.child(empty_state(
                IconName::Layers,
                "No series yet",
                "A series links several videos so every part shares one cast and look.",
                p,
            ));
        }
        let open_id = panel
            .matrix
            .as_ref()
            .map(|matrix| matrix.manifest.id.clone());
        let tabs = list
            .iter()
            .enumerate()
            .map(|(index, summary)| {
                let id = summary.id.clone();
                Button::new(("desktop-series-open", index))
                    .self_start()
                    .label(format!("{} · {} parts", summary.work, summary.part_count))
                    .selected(open_id.as_deref() == Some(summary.id.as_str()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.series_task(SeriesTask::Open(id.clone()), cx)
                    }))
            })
            .collect::<Vec<_>>();
        let body = body.child(div().flex().flex_wrap().gap_2().children(tabs));
        let Some(matrix) = &panel.matrix else {
            return body;
        };
        body.child(heading_small(&matrix.manifest.work))
            .children(
                matrix
                    .manifest
                    .parts
                    .iter()
                    .enumerate()
                    .map(|(index, part)| self.series_row(index, part, cx)),
            )
            .child(self.series_linker(cx))
            .child(self.series_review(cx))
            .child(self.series_add_parts(cx))
    }

    fn series_row(
        &self,
        index: usize,
        part: &retold_app::series::SeriesPart,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.palette();
        let panel = &self.series;
        let cells = panel
            .matrix
            .as_ref()
            .and_then(|matrix| matrix.progress.get(&part.id).cloned().flatten());
        let progress = cells.map_or_else(
            || "Not linked".to_owned(),
            |cells| {
                let done = cells
                    .iter()
                    .filter(|cell| cell.decision == "reused_from_cache")
                    .count();
                format!("{done}/{} stages saved", cells.len())
            },
        );
        let (toggle, link) = (part.id.clone(), part.id.clone());
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(
                Button::new(("desktop-series-part", index))
                    .self_start()
                    .label(format!("{} {}", part.id, part.title))
                    .selected(panel.selected.contains(&part.id))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.series.toggle(&toggle);
                        cx.notify();
                    })),
            )
            .child(muted(part.video_ref.as_deref().unwrap_or("No video"), p))
            .child(muted(&progress, p))
            .when(part.completed, |row| row.child(notice("Ready", p)))
            .child(
                Button::new(("desktop-series-link", index))
                    .self_start()
                    .ghost()
                    .label("Link video")
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.series.linking = Some(link.clone());
                        cx.notify();
                    })),
            )
    }

    fn series_linker(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let (Some(part), Some(snapshot)) = (&self.series.linking, &self.snapshot) else {
            return div();
        };
        let choices = snapshot
            .videos
            .iter()
            .take(24)
            .enumerate()
            .map(|(index, video)| {
                let (part, reference) = (part.clone(), video.reference.clone());
                Button::new(("desktop-series-link-video", index))
                    .self_start()
                    .label(video.title.clone())
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.series_task(SeriesTask::Link(part.clone(), reference.clone()), cx)
                    }))
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small(&format!("Link {part} to a video")))
            .child(muted(
                "Only videos with the series' guide and channel can be linked.",
                p,
            ))
            .child(div().flex().flex_wrap().gap_2().children(choices))
    }

    fn series_review(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.series;
        let scope = panel
            .selector()
            .map_or_else(|| "all parts".to_owned(), |parts| parts.replace(',', ", "));
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Button::new("desktop-series-plan")
                    .self_start()
                    .label(format!("Review plan for {scope}"))
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.series_task(SeriesTask::Plan, cx))),
            )
            .when_some(panel.plan.as_ref(), |body, plan| {
                body.child(notice(&plan.text, p))
                    .child(muted(
                        &format!(
                            "Allowance: {} · {} paid requests",
                            super::run::usd(plan.usd_micros()),
                            plan.requests(),
                        ),
                        p,
                    ))
                    .children(
                        plan.blockers
                            .iter()
                            .map(|blocker| error_notice(&blocker.message, p)),
                    )
                    .child(
                        Button::new("desktop-series-run")
                            .self_start()
                            .primary()
                            .label("Submit reviewed parts")
                            .disabled(self.busy || !plan.can_execute)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.series_task(SeriesTask::Run, cx)),
                            ),
                    )
            })
    }
}

impl Shell {
    fn series_add_parts(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("desktop-series-add-one")
                            .self_start()
                            .label("Add a part")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.series_task(SeriesTask::ReviewAdd(1), cx)
                            })),
                    )
                    .child(
                        Button::new("desktop-series-add-three")
                            .self_start()
                            .label("Add three parts")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.series_task(SeriesTask::ReviewAdd(3), cx)
                            })),
                    ),
            )
            .when_some(self.series.addition.as_ref(), |body, addition| {
                body.child(muted(
                    &format!(
                        "Adds unlinked parts {}. Existing parts and links are unchanged.",
                        addition.new_parts.join(", ")
                    ),
                    p,
                ))
                .child(
                    Button::new("desktop-series-add-confirm")
                        .self_start()
                        .primary()
                        .label("Confirm new parts")
                        .disabled(self.busy)
                        .on_click(
                            cx.listener(|this, _, _, cx| this.series_task(SeriesTask::Add, cx)),
                        ),
                )
            })
    }

    fn new_series_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("desktop-series-new")
            .primary()
            .icon(gpui_kit::assets::IconName::Plus)
            .label("New series")
            .disabled(self.busy || self.snapshot.is_none() || self.series.creating.is_some())
            .on_click(cx.listener(|this, _, window, cx| this.begin_new_series(window, cx)))
    }

    fn new_series_form(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(form) = &self.series.creating else {
            return div();
        };
        let guides = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.choices.as_ref().ok())
            .map(|choices| choices.guides.clone())
            .unwrap_or_default();
        let guide_buttons = guides
            .iter()
            .enumerate()
            .map(|(index, guide)| {
                let id = guide.id.clone();
                Button::new(("desktop-series-new-guide", index))
                    .self_start()
                    .label(guide.name.clone())
                    .selected(form.guide.as_deref() == Some(guide.id.as_str()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(form) = &mut this.series.creating {
                            form.guide = Some(id.clone());
                        }
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        let counts = [2usize, 3, 4, 5, 6]
            .into_iter()
            .map(|count| {
                Button::new(("desktop-series-new-parts", count))
                    .self_start()
                    .label(format!("{count} parts"))
                    .selected(form.parts == count)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(form) = &mut this.series.creating {
                            form.parts = count;
                        }
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading_small("New series"))
            .child(
                gpui_kit::component::input::Input::new(&form.work).aria_label("Series work title"),
            )
            .child(div().flex().flex_wrap().gap_2().children(guide_buttons))
            .child(div().flex().flex_wrap().gap_2().children(counts))
            .child(muted(
                "Parts start unlinked. Nothing is generated or queued.",
                p,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("desktop-series-new-create")
                            .self_start()
                            .primary()
                            .label("Create series")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.submit_new_series(cx))),
                    )
                    .child(
                        Button::new("desktop-series-new-cancel")
                            .self_start()
                            .ghost()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.series.creating = None;
                                cx.notify();
                            })),
                    ),
            )
    }
}
