//! Queue pages and event replay keep durable identity and a bounded visible event window.
use gpui_kit::Context;
use retold_app::queue::{EVENT_PAGE_SIZE, ListOptions};

use super::{
    Shell,
    backend::{JobAction, ObservedJob, Request},
};

impl Shell {
    pub(super) fn activity_page(&mut self, next: bool, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let mut options = ListOptions::default();
        if next {
            let Ok(activity) = &snapshot.activity else {
                return;
            };
            options.cursor = activity.page.next_cursor.clone();
            if options.cursor.is_none() {
                return;
            }
        }
        self.request(
            Request::Activity {
                library: snapshot.context.library.clone(),
                options,
            },
            cx,
        );
    }

    pub(super) fn open_job(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let request = Request::Job {
            library: snapshot.context.library.clone(),
            id,
            after: 0,
        };
        self.observed_job = None;
        self.request(request, cx);
    }

    pub(super) fn refresh_job(&mut self, from_start: bool, cx: &mut Context<Self>) {
        let Some(job) = &self.observed_job else {
            return;
        };
        let request = Request::Job {
            library: job.library.clone(),
            id: job.page.job.id.clone(),
            after: if from_start {
                0
            } else {
                job.page.next_sequence
            },
        };
        if from_start {
            self.observed_job = None;
        }
        self.request(request, cx);
    }

    pub(super) fn control_job(&mut self, id: String, action: JobAction, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let after = self
            .observed_job
            .as_ref()
            .filter(|job| job.page.job.id == id)
            .map_or(0, |job| job.page.next_sequence);
        self.request(
            Request::Control {
                library: snapshot.context.library.clone(),
                id,
                action,
                after,
            },
            cx,
        );
    }

    pub(super) fn receive_job(&mut self, mut observed: ObservedJob) {
        let same_workspace = self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.context.library == observed.library);
        if !same_workspace {
            return;
        }
        if let Some(previous) = &self.observed_job
            && previous.page.job.id == observed.page.job.id
        {
            let mut events = previous.page.events.clone();
            events.extend(observed.page.events);
            let skip = events.len().saturating_sub(EVENT_PAGE_SIZE);
            observed.page.events = events.into_iter().skip(skip).collect();
        }
        self.status = format!("Job {}.", observed.page.job.status.as_str());
        self.observed_job = Some(observed);
    }
}
