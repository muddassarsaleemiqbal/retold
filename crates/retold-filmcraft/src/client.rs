//! Bounded, cancellable loopback requests. A failed request poisons the connection, since
//! a delayed reply must never be interpreted as the reply to a later command.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

use crate::Command;

const MAX_REPLY: usize = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("FilmCraft I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("FilmCraft protocol error: {0}")]
    Protocol(String),
    #[error("FilmCraft rejected {command}: {message}")]
    Rejected { command: String, message: String },
    #[error("FilmCraft request timed out; reconnect and inspect the project before retrying")]
    Timeout,
    #[error("cancelled")]
    Cancelled,
    #[error("video export needs explicit permission")]
    Permission,
}

#[derive(Debug)]
struct Session {
    stream: BufReader<TcpStream>,
    next_id: u64,
    poisoned: bool,
}

#[derive(Debug)]
pub struct Client {
    session: tokio::sync::Mutex<Session>,
    timeout: Duration,
}

#[derive(Debug, Deserialize)]
struct Reply {
    id: u64,
    ok: bool,
    #[serde(default)]
    result: Value,
    #[serde(default)]
    error: Value,
}

impl Client {
    /// The caller must launch and own this private port. Never use a port discovered from
    /// the user's open project.
    pub async fn connect(
        port: u16,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<Self, Error> {
        let stream = tokio::select! {
            () = cancel.cancelled() => return Err(Error::Cancelled),
            stream = tokio::time::timeout(timeout, TcpStream::connect(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))) => stream.map_err(|_| Error::Timeout)??,
        };
        Ok(Self {
            session: tokio::sync::Mutex::new(Session {
                stream: BufReader::new(stream),
                next_id: 0,
                poisoned: false,
            }),
            timeout,
        })
    }

    /// This path can inspect or prepare an edit but cannot export a video.
    pub async fn send_command(
        &self,
        command: &Command,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        if matches!(
            command,
            Command::ExportMedia { .. } | Command::ExportAudio { .. }
        ) {
            return Err(Error::Permission);
        }
        self.request(command, cancel).await
    }

    /// Only call after the user explicitly approves video creation. Permission is scoped
    /// to this call, rather than retained implicitly across other exports.
    pub async fn export_approved(
        &self,
        command: &Command,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        if !matches!(
            command,
            Command::ExportMedia { .. } | Command::ExportAudio { .. }
        ) {
            return Err(Error::Protocol("expected an export command".into()));
        }
        self.request(command, cancel).await
    }

    async fn request(&self, command: &Command, cancel: &CancellationToken) -> Result<Value, Error> {
        let mut session = tokio::select! {
            () = cancel.cancelled() => return Err(Error::Cancelled),
            guard = self.session.lock() => guard,
        };
        if session.poisoned {
            return Err(Error::Protocol(
                "connection needs to be reopened after a failed request".into(),
            ));
        }
        session.next_id += 1;
        let id = session.next_id;
        let (name, params) = command.wire();
        let mut bytes = serde_json::to_vec(
            &json!({"id":id,"method":"engine.execute","params":{"command":name,"params":params}}),
        )
        .map_err(|e| Error::Protocol(e.to_string()))?;
        bytes.push(b'\n');
        let result = tokio::select! {
            () = cancel.cancelled() => Err(Error::Cancelled),
            result = tokio::time::timeout(self.timeout, async {
                session.stream.get_mut().write_all(&bytes).await?;
                let line = read_line(&mut session.stream).await?;
                let reply: Reply = serde_json::from_slice(&line).map_err(|e| Error::Protocol(e.to_string()))?;
                if reply.id != id { return Err(Error::Protocol(format!("expected reply {id}, got {}", reply.id))); }
                if !reply.ok { return Err(Error::Rejected { command:name.into(), message:reply.error.to_string() }); }
                Ok(reply.result)
            }) => result.unwrap_or(Err(Error::Timeout)),
        };
        if result.is_err() {
            session.poisoned = true;
        }
        result
    }
}

async fn read_line(stream: &mut BufReader<TcpStream>) -> Result<Vec<u8>, Error> {
    let mut line = Vec::new();
    loop {
        let bytes = stream.fill_buf().await?;
        if bytes.is_empty() {
            return Err(Error::Protocol(
                "connection closed before a complete reply".into(),
            ));
        }
        let count = bytes
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |at| at + 1);
        let line_len = line.len();
        if line_len + count > MAX_REPLY {
            return Err(Error::Protocol("reply exceeds 8 MB".into()));
        }
        let done = bytes[count - 1] == b'\n';
        line.extend_from_slice(&bytes[..count]);
        stream.consume(count);
        if done {
            return Ok(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn handles_fragmented_reply_and_blocks_unapproved_video_export() {
        let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (stream, _) = server.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            let request: Value =
                serde_json::from_slice(&read_line(&mut reader).await.unwrap()).unwrap();
            assert_eq!(request["params"]["command"], "project.inspect");
            reader
                .get_mut()
                .write_all(b"{\"id\":1,\"ok\":true,")
                .await
                .unwrap();
            reader
                .get_mut()
                .write_all(b"\"result\":{\"children\":[]}}\n")
                .await
                .unwrap();
            let mut next = Vec::new();
            // No second command should be sent when export is refused.
            assert!(
                tokio::time::timeout(
                    Duration::from_millis(30),
                    reader.read_until(b'\n', &mut next)
                )
                .await
                .is_err()
            );
        });
        let cancel = CancellationToken::new();
        let client = Client::connect(port, Duration::from_secs(1), &cancel)
            .await
            .unwrap();
        assert_eq!(
            client
                .send_command(&Command::InspectProject, &cancel)
                .await
                .unwrap(),
            json!({"children":[]})
        );
        let command = Command::ExportMedia {
            loudness_lufs: -14.0,
            path: "uncreated.mp4".into(),
            sequence: json!(1),
            width: 1080,
            height: 1920,
            fps: retold_core::Fps::new(30).unwrap(),
        };
        assert!(matches!(
            client.send_command(&command, &cancel).await,
            Err(Error::Permission)
        ));
        assert!(matches!(
            client
                .send_command(
                    &Command::ExportAudio {
                        path: "uncreated.wav".into(),
                        sequence: json!(1),
                        loudness_lufs: -14.0
                    },
                    &cancel
                )
                .await,
            Err(Error::Permission)
        ));
        task.await.unwrap();
    }

    #[tokio::test]
    async fn mismatched_ids_poison_the_connection() {
        let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = server.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            read_line(&mut reader).await.unwrap();
            reader
                .get_mut()
                .write_all(b"{\"id\":9,\"ok\":true,\"result\":{}}\n")
                .await
                .unwrap();
        });
        let cancel = CancellationToken::new();
        let client = Client::connect(port, Duration::from_secs(1), &cancel)
            .await
            .unwrap();
        assert!(matches!(
            client.send_command(&Command::Jobs, &cancel).await,
            Err(Error::Protocol(_))
        ));
        assert!(
            client
                .send_command(&Command::Jobs, &cancel)
                .await
                .unwrap_err()
                .to_string()
                .contains("reopened")
        );
    }
}
