use crate::state::DaemonState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

#[derive(Error, Debug)]
pub enum IpcError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Connection closed unexpectedly")]
    ConnectionClosed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload")]
pub enum Request {
    /// Start idling specified appids (or default config if empty)
    Start { appids: Vec<u32> },
    /// Stop idling specified appids (or all if None/empty)
    Stop { appids: Option<Vec<u32>> },
    /// Query current status
    Status,
    /// Restart idle session
    Restart,
    /// List configured and known games
    Games,
    /// Retrieve recent logs
    Logs { lines: usize },
    /// Health check ping
    Ping,
    /// Request daemon shutdown
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "data")]
pub enum Response {
    Ok { message: String },
    Error { code: String, message: String },
    Status(StatusPayload),
    Games(GamesPayload),
    Logs(LogsPayload),
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveGameInfo {
    pub appid: u32,
    pub name: String,
    pub session_duration_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPayload {
    pub daemon_state: DaemonState,
    pub steam_connected: bool,
    pub idle_active: bool,
    pub active_games: Vec<ActiveGameInfo>,
    pub total_session_secs: u64,
    pub real_cs2_running: bool,
    pub auto_resume_enabled: bool,
    pub uptime_secs: u64,
    pub socket_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEntry {
    pub appid: u32,
    pub name: String,
    pub is_default: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamesPayload {
    pub configured_games: Vec<GameEntry>,
    pub active_games: Vec<GameEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsPayload {
    pub lines: Vec<String>,
}

/// Helper to send a newline-delimited JSON message over an async writer.
pub async fn send_json<W, T>(writer: &mut W, msg: &T) -> Result<(), IpcError>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let mut data = serde_json::to_vec(msg)?;
    data.push(b'\n');
    writer.write_all(&data).await?;
    writer.flush().await?;
    Ok(())
}

/// Helper to receive a newline-delimited JSON message from an async buffered reader.
pub async fn recv_json<R, T>(reader: &mut R) -> Result<T, IpcError>
where
    R: AsyncBufReadExt + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Err(IpcError::ConnectionClosed);
    }
    let msg: T = serde_json::from_str(line.trim_end())?;
    Ok(msg)
}

/// Client IPC connection wrapper.
pub struct IpcClient {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl IpcClient {
    pub async fn connect(path: &std::path::Path) -> Result<Self, IpcError> {
        let stream = tokio::net::UnixStream::connect(path).await?;
        let (read_half, write_half) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(read_half),
            writer: write_half,
        })
    }

    pub async fn send_request(&mut self, req: &Request) -> Result<(), IpcError> {
        send_json(&mut self.writer, req).await
    }

    pub async fn recv_response(&mut self) -> Result<Response, IpcError> {
        recv_json(&mut self.reader).await
    }

    pub async fn call(&mut self, req: &Request) -> Result<Response, IpcError> {
        self.send_request(req).await?;
        self.recv_response().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ipc_serialization() {
        let req = Request::Start {
            appids: vec![730, 570],
        };
        let encoded = serde_json::to_string(&req).unwrap();
        let decoded: Request = serde_json::from_str(&encoded).unwrap();
        match decoded {
            Request::Start { appids } => assert_eq!(appids, vec![730, 570]),
            _ => panic!("Wrong variant decoded"),
        }
    }

    #[tokio::test]
    async fn test_ipc_stream() {
        let (client, server) = tokio::net::UnixStream::pair().unwrap();
        let (c_read, mut c_write) = client.into_split();
        let (s_read, mut s_write) = server.into_split();

        let mut c_reader = BufReader::new(c_read);
        let mut s_reader = BufReader::new(s_read);

        // Send request from client to server
        let req = Request::Status;
        send_json(&mut c_write, &req).await.unwrap();

        let server_recv: Request = recv_json(&mut s_reader).await.unwrap();
        assert!(matches!(server_recv, Request::Status));

        // Send response from server to client
        let resp = Response::Ok {
            message: "ready".into(),
        };
        send_json(&mut s_write, &resp).await.unwrap();

        let client_recv: Response = recv_json(&mut c_reader).await.unwrap();
        assert!(matches!(client_recv, Response::Ok { message } if message == "ready"));
    }
}
