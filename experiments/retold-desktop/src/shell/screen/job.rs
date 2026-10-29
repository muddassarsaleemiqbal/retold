use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, Styled, div, prelude::*};
use retold_app::queue::{EVENT_PAGE_SIZE, JobRecord, JobStatus};

use super::{heading, muted, notice};
use crate::shell::{Shell, backend::JobAction};

impl Shell {
    /// The saved title for a video reference, or the reference when the video is not listed.
    pub(super) fn video_title(&self, reference: &str) -> String {
        self.snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .videos
                    .iter()
                    .find(|video| video.reference == reference)
            })
            .map_or_else(|| reference.to_owned(), |video| video.title.clone())
    }

    pub(super) fn job_row(
        &self,
        index: usize,
        job: &JobRecord,
        previous: Option<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.palette();
        let id = job.id.clone();
        let move_id = job.id.clone();
        div()
            .rounded_lg()
            .bg(p.surface)
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
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
                            .gap_2()
                            .child(self.video_title(&job.video_ref))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(status_dot(&job.id, job.status, p))
                                    .child(muted(&super::title_case(job.status.as_str()), p)),
                            ),
                    )
                    .child(
                        Button::new(("desktop-job", index))
                            .self_start()
                            .label("Inspect")
                            .disabled(self.busy)
                            .accessibility_label(format!("Inspect job {}", job.id))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.open_job(id.clone(), cx)),
                            ),
                    ),
            )
            .when(job.status == JobStatus::Queued, |row| {
                row.child(
                    Button::new(("desktop-move-job", index))
                        .self_start()
                        .label("Move up in queue")
                        .disabled(self.busy || previous.is_none())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(before) = &previous {
                                this.control_job(
                                    move_id.clone(),
                                    JobAction::MoveBefore(before.clone()),
                                    cx,
                                );
                            }
                        })),
                )
            })
    }

    pub(super) fn job_details(&self, cx: &mut Context<Self>) -> Div {
        let Some(observed) = &self.observed_job else {
            return div();
        };
        let p = self.palette();
        let page = &observed.page;
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                Button::new("desktop-job-back")
                    .self_start()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .ml(gpui_kit::px(-10.))
                    .label("Activity")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.observed_job = None;
                        this.activity_page(false, cx);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(heading(&self.video_title(&page.job.video_ref)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py(gpui_kit::px(2.))
                            .rounded_full()
                            .bg(p.text.opacity(0.08))
                            .child(status_dot(&page.job.id, page.job.status, p))
                            .child(super::title_case(page.job.status.as_str())),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .font_family("Menlo")
                    .text_color(p.secondary)
                    .child(page.job.id.clone()),
            )
            .child(muted(
                if page.job.status == JobStatus::Stopped {
                    "Its full history is saved."
                } else if page.job.status.is_terminal() {
                    "This job has finished. Its full history is saved."
                } else if observed.owner_present {
                    "Running now. This is its latest saved progress."
                } else {
                    "The renderer has not reported in. Progress is confirmed once it does."
                },
                p,
            ))
            .child(self.job_controls(&page.job, cx))
            .when_some(page.job.error_message.as_ref(), |body, message| {
                body.child(notice(message, p))
            })
            .child(muted(
                &if page.events.len() == 1 {
                    "1 event".to_owned()
                } else {
                    format!("{} events", page.events.len())
                },
                p,
            ))
            .when(page.events.len() == EVENT_PAGE_SIZE, |body| {
                body.child(muted("Showing the latest events.", p))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .children(page.events.iter().enumerate().map(|(index, event)| {
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_3()
                            .py_2()
                            .when(index > 0, |row| row.border_t_1().border_color(p.border))
                            .child(
                                div()
                                    .w(gpui_kit::px(28.))
                                    .text_xs()
                                    .text_color(p.secondary)
                                    .child(event.sequence.to_string()),
                            )
                            .child(
                                div()
                                    .w(gpui_kit::px(120.))
                                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                                    .child(super::title_case(&event.stage)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(p.secondary)
                                    .child(event.message.clone()),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .when(!page.is_terminal, |row| {
                        row.child(
                            Button::new("desktop-job-events")
                                .self_start()
                                .label("Load newer events")
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.refresh_job(false, cx)),
                                ),
                        )
                        .child(
                            Button::new("desktop-job-follow")
                                .self_start()
                                .label(if self.follow.enabled {
                                    "Stop following"
                                } else {
                                    "Follow live"
                                })
                                .on_click(cx.listener(|this, _, _, cx| this.toggle_follow(cx))),
                        )
                    })
                    .child(
                        Button::new("desktop-job-events-start")
                            .self_start()
                            .label("Show full log")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_job(true, cx))),
                    ),
            )
    }

    fn job_controls(&self, job: &JobRecord, cx: &mut Context<Self>) -> Div {
        let pause = job.id.clone();
        let resume = job.id.clone();
        let stop = job.id.clone();
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .when(
                matches!(job.status, JobStatus::Queued | JobStatus::Running),
                |row| {
                    row.child(
                        Button::new("desktop-pause-job")
                            .self_start()
                            .label("Pause")
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.control_job(pause.clone(), JobAction::Pause, cx)
                            })),
                    )
                },
            )
            .when(job.status == JobStatus::Paused, |row| {
                row.child(
                    Button::new("desktop-resume-job")
                        .self_start()
                        .label("Resume with saved plan")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.control_job(resume.clone(), JobAction::Resume, cx)
                        })),
                )
            })
            .when(
                !job.status.is_terminal() && job.status != JobStatus::Stopping,
                |row| {
                    row.child(
                        Button::new("desktop-stop-job")
                            .self_start()
                            .label("Stop this job")
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.control_job(stop.clone(), JobAction::Stop, cx)
                            })),
                    )
                },
            )
    }
}

/// A status dot: accent and gently pulsing while a job runs, quiet otherwise.
fn status_dot(id: &str, status: JobStatus, p: crate::theme::Palette) -> gpui_kit::AnyElement {
    use gpui_kit::{Animation, AnimationExt as _};
    let running = matches!(status, JobStatus::Running);
    let colour = match status {
        JobStatus::Running => p.accent,
        JobStatus::Completed => p.success,
        JobStatus::Failed | JobStatus::Blocked => p.error,
        _ => p.secondary,
    };
    let dot = div().size(gpui_kit::px(8.)).rounded_full().bg(colour);
    if !running || !super::MOTION.load(std::sync::atomic::Ordering::Relaxed) {
        return dot.into_any_element();
    }
    dot.with_animation(
        gpui_kit::ElementId::Name(format!("job-dot-{id}").into()),
        Animation::new(std::time::Duration::from_millis(1400))
            .repeat()
            .with_max_fps(super::DECORATIVE_FPS)
            .with_easing(gpui_kit::pulsating_between(0.45, 1.)),
        |dot, opacity| dot.opacity(opacity),
    )
    .into_any_element()
}
