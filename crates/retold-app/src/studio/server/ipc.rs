//! Bounded client I/O keeps malformed or stalled clients off the execution path.
use super::super::platform::UnixStream;
use std::io::{BufRead, BufReader, Read, Write};
use std::time::Duration;

use super::{StudioError, StudioRequest, StudioResponse, StudioServer};

const CLIENT_TIMEOUT: Duration = Duration::from_millis(100);
const MAX_REQUEST_BYTES: u64 = 16 * 1024;

impl StudioServer {
    pub(super) fn serve_client(&self, stream: &UnixStream) -> Result<(), StudioError> {
        configure_stream(stream)?;
        let request = read_request(stream);
        let response = request
            .and_then(|request| self.dispatch(request))
            .unwrap_or_else(|error| error_response(&error));
        write_response(stream, &response)
    }
}

fn error_response(error: &StudioError) -> StudioResponse {
    let diagnostic = error.diagnostic();
    StudioResponse::Error {
        code: diagnostic.code,
        message: diagnostic.message,
    }
}

fn read_request(stream: &UnixStream) -> Result<StudioRequest, StudioError> {
    let mut reader = BufReader::new(stream.take(MAX_REQUEST_BYTES.saturating_add(1)));
    let mut line = String::new();
    let count = reader.read_line(&mut line)?;
    let valid = count > 0
        && u64::try_from(count).unwrap_or(u64::MAX) <= MAX_REQUEST_BYTES
        && line.ends_with('\n');
    if !valid {
        return Err(StudioError::Protocol(
            "Expected one bounded request line.".into(),
        ));
    }
    Ok(serde_json::from_str(&line)?)
}

fn configure_stream(stream: &UnixStream) -> Result<(), StudioError> {
    stream.set_read_timeout(Some(CLIENT_TIMEOUT))?;
    stream.set_write_timeout(Some(CLIENT_TIMEOUT))?;
    Ok(())
}

fn write_response(mut stream: &UnixStream, response: &StudioResponse) -> Result<(), StudioError> {
    let mut payload = serde_json::to_vec(response)?;
    payload.push(b'\n');
    if u64::try_from(payload.len()).unwrap_or(u64::MAX) > super::super::types::MAX_RESPONSE_BYTES {
        return Err(StudioError::Protocol(
            "Studio reply exceeds the bounded protocol size.".into(),
        ));
    }
    stream.write_all(&payload)?;
    stream.flush()?;
    Ok(())
}
