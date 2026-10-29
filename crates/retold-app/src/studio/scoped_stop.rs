//! Offline inhibition precedes IPC; connection loss never acknowledges physical cancellation.
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{StudioError, StudioRequest, StudioResponse, send_request};
use crate::queue::{QueueStore, StopReceipt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedStop {
    pub receipt: StopReceipt,
    /// The daemon accepted the request, not proof that active work has exited.
    pub worker_acknowledged: bool,
    pub connection_issue: Option<String>,
}

pub fn stop_workspace_queue(state: &Path, control_id: &str) -> Result<ScopedStop, StudioError> {
    super::validate_startup(Option::default(), state)?;
    let queue = QueueStore::open(&state.join(super::server::QUEUE_DB_NAME))?;
    let receipt = queue.stop_all(control_id, &jiff::Timestamp::now().to_string())?;
    let acknowledgment = notify_worker(state, control_id);
    let connection_issue = acknowledgment.err().map(|error| error.to_string());
    Ok(ScopedStop {
        receipt,
        worker_acknowledged: connection_issue.is_none(),
        connection_issue,
    })
}

fn notify_worker(state: &Path, id: &str) -> Result<(), StudioError> {
    let response = send_request(state, &StudioRequest::Capabilities)?;
    let supported = matches!(response, StudioResponse::Capabilities { features, .. }
        if features.iter().any(|feature| feature == "durable_stop_all_v1"));
    if !supported {
        return Err(StudioError::Protocol("Worker does not support durable Stop all. Inhibition is saved; active work remains unconfirmed.".into()));
    }
    let request = StudioRequest::StopAll {
        control_id: id.into(),
    };
    match send_request(state, &request)? {
        StudioResponse::StopAccepted { receipt } if receipt.control_id == id => Ok(()),
        _ => Err(StudioError::Protocol(
            "Worker did not acknowledge this stop receipt.".into(),
        )),
    }
}
