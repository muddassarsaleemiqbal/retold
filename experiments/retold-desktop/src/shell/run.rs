//! Review callbacks bind allowance, target and workspace to the returned exact plan.
use gpui_kit::Context;
use retold_app::desktop::ReviewedRun;
use retold_app::plan::PlanOptions;

use super::{Shell, backend::Request};

impl Shell {
    pub(super) fn review_selected(&mut self, cx: &mut Context<Self>) {
        self.review_target(None, cx);
    }

    pub(super) fn review_target(&mut self, until: Option<&str>, cx: &mut Context<Self>) {
        let (Some(snapshot), Some(reference)) = (&self.snapshot, &self.selected_video) else {
            return;
        };
        let Ok(requests) = self.request_limit.read(cx).value().parse::<u64>() else {
            self.status = "Maximum requests must be a nonnegative whole number.".into();
            cx.notify();
            return;
        };
        let options = PlanOptions {
            until: until.map(str::to_owned),
            spend: Some(self.allowance.read(cx).value().to_string()),
            max_requests: Some(requests),
            ..PlanOptions::default()
        };
        self.review = None;
        self.request(
            Request::Review {
                root: snapshot.context.library.clone(),
                reference: reference.clone(),
                options,
            },
            cx,
        );
    }

    pub(super) fn receive_review(&mut self, review: ReviewedRun, cx: &mut Context<Self>) {
        let same_target = self.selected_video.as_deref() == Some(&review.reference);
        let same_root = self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.context.library == review.library);
        let spend = self.allowance.read(cx).value();
        let requests = self.request_limit.read(cx).value().parse::<u64>().ok();
        let same_allowance = review.options.spend.as_deref() == Some(spend.as_ref())
            && review.options.max_requests == requests;
        if !same_target || !same_root || !same_allowance {
            self.status = "The selection or allowance changed. Review again before running.".into();
            return;
        }
        self.status = if review.plan.is_unblocked() {
            "Plan ready for review. Nothing has been submitted.".into()
        } else {
            "Plan reviewed.".into()
        };
        self.review = Some(review);
    }

    pub(super) fn submit_review(&mut self, cx: &mut Context<Self>) {
        if let Some(review) = self.review.clone() {
            let current = self.selected_video.as_deref() == Some(&review.reference)
                && self
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.context.library == review.library)
                && review.options.spend.as_deref()
                    == Some(self.allowance.read(cx).value().as_ref())
                && review.options.max_requests
                    == self.request_limit.read(cx).value().parse::<u64>().ok();
            if !current {
                self.review = None;
                self.status =
                    "The selection or allowance changed. Review again before running.".into();
                cx.notify();
                return;
            }
            self.request(Request::Submit(Box::new(review)), cx);
        }
    }
}
