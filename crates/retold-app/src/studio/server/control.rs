//! Requests are persisted before cancellation; only worker completion marks active work stopped.
use super::{
    JobStatus, StudioError, StudioResponse, StudioServer, TransitionSpec, worker_lock_error,
};
use crate::queue::EVENT_PAGE_SIZE;

impl StudioServer {
    pub(super) fn dispatch_control(
        &self,
        request: super::StudioRequest,
    ) -> Result<StudioResponse, StudioError> {
        use super::StudioRequest;
        match request {
            StudioRequest::Capabilities => Ok(StudioResponse::Capabilities {
                protocol: crate::protocol::PROTOCOL_VERSION,
                features: vec!["durable_stop_all_v1".into(), "event_replay_v1".into()],
            }),
            StudioRequest::GetDispatch => Ok(StudioResponse::Dispatch {
                state: self.queue.dispatch_state()?,
            }),
            StudioRequest::StopAll { control_id } => self.stop_all(&control_id),
            StudioRequest::ResumeDispatch { control_id } => Ok(StudioResponse::Dispatch {
                state: self.queue.resume_dispatch(&control_id)?,
            }),
            _ => Err(StudioError::Protocol(
                "Unsupported workspace control.".into(),
            )),
        }
    }

    pub(super) fn stop_all(&self, control_id: &str) -> Result<StudioResponse, StudioError> {
        let worker = self.worker.lock().map_err(|_| worker_lock_error())?;
        let receipt = self
            .queue
            .stop_all(control_id, &jiff::Timestamp::now().to_string())?;
        let owned = worker
            .as_ref()
            .filter(|active| receipt.stopping.contains(&active.id));
        if let Some(active) = owned {
            active.control.cancel();
        }
        Ok(StudioResponse::StopAccepted { receipt })
    }

    pub(super) fn cancel_requested(
        &self,
        worker: &super::super::worker::Worker,
    ) -> Result<(), StudioError> {
        let job = self.queue.find(&worker.id)?;
        if job.is_some_and(|job| matches!(job.status, JobStatus::Pausing | JobStatus::Stopping)) {
            worker.control.cancel();
        }
        Ok(())
    }

    pub(super) fn inspect_job(&self, id: &str) -> Result<StudioResponse, StudioError> {
        let details = self.queue.details(id)?;
        let (job, result, execution) = details.map_or_else(
            || (Option::default(), Option::default(), Option::default()),
            |details| {
                (
                    Some(details.job),
                    details.result,
                    details.execution.map(Box::new),
                )
            },
        );
        Ok(StudioResponse::Job {
            job,
            result,
            execution,
        })
    }

    pub(super) fn move_pending(
        &self,
        id: &str,
        before: &str,
    ) -> Result<StudioResponse, StudioError> {
        let job = self
            .queue
            .find(id)?
            .ok_or_else(|| crate::queue::QueueError::NotFound(id.into()))?;
        let _activity = crate::studio::worker::activity(&self.queue, &job.id)?;
        self.queue.move_before(&job.id, before)?;
        Ok(StudioResponse::JobTransitioned {
            job: self
                .queue
                .find(&job.id)?
                .ok_or_else(|| crate::queue::QueueError::NotFound(job.id))?,
        })
    }

    pub(super) fn control_job(
        &self,
        id: &str,
        requested: JobStatus,
    ) -> Result<StudioResponse, StudioError> {
        let job = self
            .queue
            .find(id)?
            .ok_or_else(|| crate::queue::QueueError::NotFound(id.into()))?;
        let target = control_target(job.status, requested);
        if job.status == target {
            return Ok(StudioResponse::JobTransitioned { job });
        }
        let _activity = crate::studio::worker::activity(&self.queue, &job.id)?;
        let now = jiff::Timestamp::now().to_string();
        let spec = TransitionSpec {
            id: &job.id,
            target,
            error: None,
        };
        let updated = self.queue.transition(spec, &now)?;
        let worker = self.worker.lock().map_err(|_| worker_lock_error())?;
        let cancelling = matches!(target, JobStatus::Pausing | JobStatus::Stopping);
        let owned = worker.as_ref().filter(|worker| worker.id == job.id);
        if let Some(worker) = owned
            && cancelling
        {
            worker.control.cancel();
        }
        Ok(StudioResponse::JobTransitioned { job: updated })
    }

    pub(super) fn begin_shutdown(&self) -> Result<(), StudioError> {
        let worker = self.worker.lock().map_err(|_| worker_lock_error())?;
        if let Some(worker) = worker.as_ref() {
            let persisted = self.request_worker_pause(&worker.id);
            // A broken journal must never prevent stopping physical work.
            worker.control.cancel();
            return persisted;
        }
        Ok(())
    }

    fn request_worker_pause(&self, id: &str) -> Result<(), StudioError> {
        let job = self.queue.find(id)?;
        if job.is_some_and(|job| job.status == JobStatus::Running) {
            let spec = TransitionSpec {
                id,
                target: JobStatus::Pausing,
                error: None,
            };
            self.queue
                .transition(spec, &jiff::Timestamp::now().to_string())?;
        }
        Ok(())
    }

    pub(super) fn attach_events(
        &self,
        id: &str,
        after: Option<u64>,
    ) -> Result<StudioResponse, StudioError> {
        let job = self
            .queue
            .find(id)?
            .ok_or_else(|| crate::queue::QueueError::NotFound(id.into()))?;
        let events = self.queue.events(&job.id, after.unwrap_or(0))?;
        let is_terminal = job.status.is_terminal() && events.len() < EVENT_PAGE_SIZE;
        Ok(StudioResponse::Events {
            events,
            is_terminal,
        })
    }
}

const fn control_target(current: JobStatus, requested: JobStatus) -> JobStatus {
    match (current, requested) {
        (JobStatus::Queued | JobStatus::Paused, JobStatus::Pausing) => JobStatus::Paused,
        (JobStatus::Queued | JobStatus::Paused | JobStatus::Stopped, JobStatus::Stopping) => {
            JobStatus::Stopped
        }
        _ => requested,
    }
}
