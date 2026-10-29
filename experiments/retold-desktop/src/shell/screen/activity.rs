use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, Div, Styled, div, prelude::*};

use super::{card, empty, empty_state, muted, notice, page_header, refresh_button};
use crate::shell::Shell;
use crate::shell::controls::stop_summary;
use gpui_kit::assets::IconName;

impl Shell {
    pub(super) fn activity(&self, cx: &mut Context<Self>) -> Div {
        if self.observed_job.is_some() {
            return self.job_details(cx);
        }
        let p = self.palette();
        let Some(snapshot) = &self.snapshot else {
            return empty(
                "Activity",
                "Open a workspace to inspect its production queue.",
                p,
            );
        };
        let activity = match &snapshot.activity {
            Ok(activity) => activity,
            Err(error) => return empty("Queue unavailable", &error.message, p),
        };
        div().flex().flex_col().gap_4()
            .child(page_header("Activity", "Renders and long tasks in this workspace.", [
                refresh_button("desktop-refresh-activity", "Refresh activity", self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.activity_page(false, cx))).into_any_element(),
            ], p))

            .children(activity.worker.issues.iter().map(|issue| notice(&issue.message, p)))
            .child(div().flex().flex_wrap().items_center().justify_between().gap_3().p_3().rounded_lg().border_1().border_color(p.border).bg(p.surface)
                .child(div().flex().items_center().gap_2().child(div().size(gpui_kit::px(8.)).rounded_full().bg(if activity.worker.running { p.success } else { p.secondary })).child(muted(if activity.worker.running { "Background renderer running · It keeps working after you close Studio." } else if activity.dispatch.inhibited_by.is_some() { "Background renderer stopped." } else { "Background renderer stopped · Starting it runs any queued jobs." }, p)))
                .child(div().flex().flex_wrap().gap_2()
                .when(activity.dispatch.inhibited_by.is_some(), |row| row.child(Button::new("desktop-enable-dispatch").self_start().when(!activity.page.jobs.is_empty(), |button| button.primary()).icon(IconName::Play).label("Resume queue").disabled(self.busy || self.control_busy).on_click(cx.listener(|this, _, _, cx| this.enable_dispatch(cx)))))
                .when(!activity.worker.running, |row| row.child(Button::new("desktop-start-worker").self_start().when(activity.dispatch.inhibited_by.is_none() && !activity.page.jobs.is_empty(), |button| button.primary()).icon(IconName::Server).label("Start background renderer").tooltip("Runs jobs even after Studio closes").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.start_worker(cx)))))
                .when(activity.worker.running, |row| row.child(Button::new("desktop-shutdown-worker").self_start().icon(IconName::CircleStop).label("Stop renderer").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.shutdown_worker(cx)))))
                .child(Button::new("desktop-stop-all").self_start().danger().outline().icon(IconName::CircleStop).label("Stop all jobs…").disabled(self.control_busy || self.confirm_stop).on_click(cx.listener(|this, _, _, cx| { this.confirm_stop = true; cx.notify(); })))))
            .when(self.confirm_stop, |body| body.child(self.stop_confirmation(cx)))
            .when(activity.dispatch.inhibited_by.is_some(), |body| body.child(notice("Queue paused. New jobs will not start until you resume it; stopped jobs stay stopped.", p)))
            .when_some(self.last_stop.as_ref().filter(|_| activity.dispatch.inhibited_by.is_none()), |body, stop| body.child(card("Last stop request", &stop_summary(stop), p)))
            .when(self.activity_options.cursor.is_some() || activity.page.next_cursor.is_some(), |body| body.child(div().flex().flex_wrap().gap_2()
                .child(Button::new("desktop-activity-first").self_start().label("First page").disabled(self.busy || self.activity_options.cursor.is_none()).on_click(cx.listener(|this, _, _, cx| this.activity_page(false, cx))))
                .child(Button::new("desktop-activity-next").self_start().label("Next page").disabled(self.busy || activity.page.next_cursor.is_none()).on_click(cx.listener(|this, _, _, cx| this.activity_page(true, cx))))))
            .when(!activity.page.jobs.is_empty(), |body| body
                .child(muted(&format!("Showing {} of {} jobs", activity.page.jobs.len(), activity.page.total), p))
                )
            .children(activity.page.jobs.iter().enumerate().map(|(index, job)| {
                let previous = activity.page.jobs[..index].iter().rev().find(|job| job.status == retold_app::queue::JobStatus::Queued).map(|job| job.id.clone());
                let row = self.job_row(index, job, previous, cx);
                super::rise_in(gpui_kit::ElementId::Name(format!("desktop-job-rise-{}", job.id).into()), index, row, cx)
            }))
            .when(activity.page.jobs.is_empty(), |body| body.child(empty_state(IconName::Activity, "No jobs yet", "Start a run from any video; its progress shows here.", p)))
    }
}

impl Shell {
    /// Inline confirmation before stopping every job in the workspace.
    fn stop_confirmation(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.error.opacity(0.5))
            .bg(p.error.opacity(0.08))
            .child(div().child("Stop every job in this workspace? Finished work is kept."))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("desktop-stop-all-cancel")
                            .ghost()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_stop = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("desktop-stop-all-confirm")
                            .danger()
                            .label("Stop jobs")
                            .disabled(self.control_busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_stop = false;
                                this.stop_all(cx);
                            })),
                    ),
            )
    }
}
