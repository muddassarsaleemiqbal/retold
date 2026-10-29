//! Studio IPC client queries and socket communication.

use super::platform::UnixStream;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::server::{LOCK_NAME, QUEUE_DB_NAME, SOCKET_NAME};
use super::types::{ProgressEvent, StudioError, StudioRequest, StudioResponse, StudioStatus};
use crate::queue::QueueStore;

const TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_POLL: Duration = Duration::from_millis(25);

#[must_use]
pub fn query_status(workspace_state: &Path) -> StudioStatus {
    let (depth, issues) = match workspace_queue_depth(workspace_state) {
        Ok(depth) => (depth, Vec::new()),
        Err(error) => (0, vec![error]),
    };
    let mut status = if studio_files_exist(workspace_state) {
        ping_status(workspace_state, depth)
    } else {
        offline_status(depth)
    };
    status.issues.extend(issues);
    status
}

fn studio_files_exist(workspace_state: &Path) -> bool {
    let lock_path = workspace_state.join(LOCK_NAME);
    let socket_path = workspace_state.join(SOCKET_NAME);
    lock_path.exists() || socket_path.exists()
}

fn endpoint_error(error: StudioError) -> StudioError {
    match error {
        StudioError::Io(error) => connection_error(error),
        error => error,
    }
}

fn workspace_queue_depth(workspace_state: &Path) -> Result<usize, crate::protocol::Diagnostic> {
    read_local_queue_depth(&workspace_state.join(QUEUE_DB_NAME))
}

fn ping_status(workspace_state: &Path, fallback_depth: usize) -> StudioStatus {
    match send_request(workspace_state, &StudioRequest::Ping) {
        Ok(StudioResponse::Pong {
            pid,
            uptime_seconds,
            active_jobs,
            queue_depth,
        }) => StudioStatus {
            running: true,
            pid: Some(pid),
            active_jobs,
            queue_depth,
            uptime_seconds: Some(uptime_seconds),
            issues: Vec::new(),
        },
        response => unavailable_status(response, fallback_depth),
    }
}

fn unavailable_status(
    response: Result<StudioResponse, StudioError>,
    queue_depth: usize,
) -> StudioStatus {
    match response {
        Ok(StudioResponse::Error { code, message }) => {
            failed_status(queue_depth, &StudioError::Remote { code, message })
        }
        Ok(_) => failed_status(
            queue_depth,
            &StudioError::Protocol("Unexpected studio status response.".into()),
        ),
        Err(StudioError::NotRunning) => offline_status(queue_depth),
        Err(error) => failed_status(queue_depth, &error),
    }
}

fn failed_status(queue_depth: usize, error: &StudioError) -> StudioStatus {
    let mut status = offline_status(queue_depth);
    status.issues.push(error.diagnostic());
    status
}

fn read_local_queue_depth(db_path: &Path) -> Result<usize, crate::protocol::Diagnostic> {
    if !db_path.exists() {
        return Ok(0);
    }
    QueueStore::open_read_only(db_path)
        .and_then(|store| store.pending_count())
        .map_err(|error| error.diagnostic())
}

const fn offline_status(queue_depth: usize) -> StudioStatus {
    StudioStatus {
        running: false,
        pid: None,
        active_jobs: 0,
        queue_depth,
        uptime_seconds: None,
        issues: Vec::new(),
    }
}

pub fn send_request(
    workspace_state: &Path,
    request: &StudioRequest,
) -> Result<StudioResponse, StudioError> {
    let socket_path = super::endpoint::address(workspace_state).map_err(endpoint_error)?;
    let stream = UnixStream::connect(&socket_path).map_err(connection_error)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    exchange(stream, request)
}

fn connection_error(error: std::io::Error) -> StudioError {
    match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
            StudioError::NotRunning
        }
        _ => StudioError::Io(error),
    }
}

pub fn poll_events(
    workspace_state: &Path,
    job_id: &str,
) -> Result<(Vec<ProgressEvent>, bool), StudioError> {
    let request = StudioRequest::AttachEvents {
        job_id: job_id.to_string(),
        after_sequence: None,
    };
    dispatch_events_request(workspace_state, &request)
}

pub fn poll_events_after(
    workspace_state: &Path,
    job_id: &str,
    after_sequence: u64,
) -> Result<(Vec<ProgressEvent>, bool), StudioError> {
    let request = StudioRequest::AttachEvents {
        job_id: job_id.to_string(),
        after_sequence: Some(after_sequence),
    };
    dispatch_events_request(workspace_state, &request)
}

fn dispatch_events_request(
    workspace_state: &Path,
    request: &StudioRequest,
) -> Result<(Vec<ProgressEvent>, bool), StudioError> {
    match send_request(workspace_state, request)? {
        StudioResponse::Events {
            events,
            is_terminal,
        } => Ok((events, is_terminal)),
        StudioResponse::Error { code, message } => Err(StudioError::Remote { code, message }),
        _ => Err(StudioError::Protocol(
            "Unexpected studio response variant".into(),
        )),
    }
}

pub fn stop_studio(workspace_state: &Path) -> Result<bool, StudioError> {
    match send_request(workspace_state, &StudioRequest::Shutdown) {
        Ok(StudioResponse::Error { code, message }) => Err(StudioError::Remote { code, message }),
        Ok(StudioResponse::ShutdownAccepted) => wait_for_shutdown(workspace_state),
        Ok(StudioResponse::Stopped) => Ok(true),
        Ok(_) => Err(StudioError::Protocol(
            "Unexpected shutdown response.".into(),
        )),
        Err(StudioError::NotRunning) => Ok(false),
        Err(err) => Err(err),
    }
}

fn wait_for_shutdown(state: &Path) -> Result<bool, StudioError> {
    let started = Instant::now();
    let socket = state.join(SOCKET_NAME);
    while socket.exists() {
        if started.elapsed() >= TIMEOUT {
            return Err(StudioError::Protocol("Shutdown was accepted; studio is still draining in-flight work. Inspect studio status before retrying.".into()));
        }
        std::thread::sleep(SHUTDOWN_POLL);
    }
    Ok(true)
}

fn exchange(
    mut stream: UnixStream,
    request: &StudioRequest,
) -> Result<StudioResponse, StudioError> {
    let mut payload = serde_json::to_vec(request)?;
    payload.push(b'\n');
    stream.write_all(&payload)?;
    stream.flush()?;

    let mut reader =
        BufReader::new(stream.take(super::types::MAX_RESPONSE_BYTES.saturating_add(1)));
    let mut line = String::new();
    let count = reader.read_line(&mut line)?;
    let bounded = u64::try_from(count).unwrap_or(u64::MAX) <= super::types::MAX_RESPONSE_BYTES;
    if !bounded || !line.ends_with('\n') {
        return Err(StudioError::Protocol(
            "Expected one bounded, complete studio reply.".into(),
        ));
    }
    let response: StudioResponse = serde_json::from_str(&line)?;
    Ok(response)
}

#[must_use]
pub fn resolve_workspace_state(library: Option<&Path>) -> PathBuf {
    if let Ok(state_env) = std::env::var("RETOLD_STATE") {
        return PathBuf::from(state_env);
    }
    if let Some(lib) = library {
        return lib.join(".retold/studio");
    }
    directories::ProjectDirs::from("", "", "retold").map_or_else(
        || PathBuf::from(".retold/studio"),
        |dirs| dirs.data_local_dir().join("default"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn status_after_reply(reply: &[u8]) -> StudioStatus {
        let root = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(root.path().join(SOCKET_NAME)).unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                BufReader::new(&stream).read_line(&mut request).unwrap();
                stream.write_all(reply).unwrap();
            });
            query_status(root.path())
        })
    }

    #[test]
    fn offline_long_socket_paths_are_read_only() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("x".repeat(128));
        std::fs::create_dir(&state).unwrap();
        let error = send_request(&state, &StudioRequest::Ping).unwrap_err();
        assert!(matches!(error, StudioError::NotRunning), "{error}");
        assert_eq!(std::fs::read_dir(&state).unwrap().count(), 0);
    }

    #[test]
    fn status_preserves_a_live_studios_error_reply() {
        let reply = StudioResponse::Error {
            code: crate::protocol::ErrorCode::AccountBusy,
            message: "Queue is busy; retry after its writer finishes.".into(),
        };
        let mut bytes = serde_json::to_vec(&reply).unwrap();
        bytes.push(b'\n');
        let status = status_after_reply(&bytes);
        assert_eq!(status.issues.len(), 1);
        assert_eq!(
            status.issues[0].code,
            crate::protocol::ErrorCode::AccountBusy
        );
    }

    #[test]
    fn status_reports_a_broken_live_reply_instead_of_claiming_offline() {
        let status = status_after_reply(b"invalid-json\n");
        assert_eq!(status.issues.len(), 1);
        assert_eq!(
            status.issues[0].code,
            crate::protocol::ErrorCode::ExecutionFailed
        );
    }

    #[test]
    fn malformed_or_oversized_replies_are_rejected_before_deserialization() {
        let oversized =
            vec![b'x'; usize::try_from(super::super::types::MAX_RESPONSE_BYTES).unwrap() + 1];
        for reply in [oversized, br#"{"response":"stopped"}"#.to_vec()] {
            let root = tempfile::tempdir().unwrap();
            let listener = UnixListener::bind(root.path().join(SOCKET_NAME)).unwrap();
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut request = String::new();
                    BufReader::new(&stream).read_line(&mut request).unwrap();
                    let _ = stream.write_all(&reply);
                });
                let result = send_request(root.path(), &StudioRequest::Ping);
                assert!(matches!(result, Err(StudioError::Protocol(_))));
            });
        }
    }
}
