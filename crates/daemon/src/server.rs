use anyhow::{Context, Result};
use protocol::{recv_json, send_json, IpcError, Request, Response};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error, info, warn};

use crate::engine::Engine;

pub struct UnixServer {
    socket_path: PathBuf,
    engine: Arc<Mutex<Engine>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl UnixServer {
    pub fn new(
        socket_path: PathBuf,
        engine: Arc<Mutex<Engine>>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Self {
        Self {
            socket_path,
            engine,
            shutdown_tx,
        }
    }

    /// Prepare directories, remove any stale socket, and bind listener with 0600 permissions.
    pub async fn run(self, mut shutdown_rx: broadcast::Receiver<()>) -> Result<()> {
        let path = &self.socket_path;

        // Ensure parent directory exists and has 0700 permissions
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create socket directory {}", parent.display())
            })?;
            #[cfg(unix)]
            {
                let perms = fs::Permissions::from_mode(0o700);
                let _ = fs::set_permissions(parent, perms);
            }
        }

        // Clean up stale socket file if it exists
        if path.exists() {
            debug!("Removing stale socket file at {}", path.display());
            let _ = fs::remove_file(path);
        }

        let listener = UnixListener::bind(path)
            .with_context(|| format!("Failed to bind Unix socket at {}", path.display()))?;

        // Restrict socket file permissions to 0600 (owner read/write only)
        #[cfg(unix)]
        {
            let perms = fs::Permissions::from_mode(0o600);
            if let Err(e) = fs::set_permissions(path, perms) {
                warn!("Failed to set 0600 permissions on socket file: {}", e);
            }
        }

        info!("Unix Domain Socket server listening at {}", path.display());

        loop {
            tokio::select! {
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, _)) => {
                            let engine = self.engine.clone();
                            let shutdown_tx = self.shutdown_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, engine, shutdown_tx).await {
                                    debug!("Client connection terminated: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Error accepting connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Unix socket server received shutdown signal");
                    break;
                }
            }
        }

        // Clean up socket file on exit
        if path.exists() {
            let _ = fs::remove_file(path);
        }

        Ok(())
    }
}

async fn handle_connection(
    stream: UnixStream,
    engine: Arc<Mutex<Engine>>,
    shutdown_tx: broadcast::Sender<()>,
) -> Result<(), IpcError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    loop {
        let req: Request = match recv_json(&mut reader).await {
            Ok(r) => r,
            Err(IpcError::ConnectionClosed) => break,
            Err(e) => return Err(e),
        };

        let is_shutdown = matches!(req, Request::Shutdown);
        let resp = process_request(req, &engine).await;
        send_json(&mut write_half, &resp).await?;

        if is_shutdown {
            let _ = shutdown_tx.send(());
            break;
        }
    }

    Ok(())
}

async fn process_request(req: Request, engine: &Arc<Mutex<Engine>>) -> Response {
    match req {
        Request::Start { appids } => {
            let mut eng = engine.lock().await;
            match eng.start_idle(appids).await {
                Ok(_) => Response::Ok {
                    message: "Game presence successfully enabled".into(),
                },
                Err(e) => Response::Error {
                    code: "START_FAILED".into(),
                    message: e.to_string(),
                },
            }
        }
        Request::Stop { appids } => {
            let mut eng = engine.lock().await;
            match eng.stop_idle(appids).await {
                Ok(_) => Response::Ok {
                    message: "Game presence stopped".into(),
                },
                Err(e) => Response::Error {
                    code: "STOP_FAILED".into(),
                    message: e.to_string(),
                },
            }
        }
        Request::Status => {
            let eng = engine.lock().await;
            Response::Status(eng.build_status())
        }
        Request::Restart => {
            let mut eng = engine.lock().await;
            let current_games = eng.config().games.clone();
            let _ = eng.stop_idle(None).await;
            match eng.start_idle(current_games).await {
                Ok(_) => Response::Ok {
                    message: "Idle presence restarted".into(),
                },
                Err(e) => Response::Error {
                    code: "RESTART_FAILED".into(),
                    message: e.to_string(),
                },
            }
        }
        Request::Games => {
            let eng = engine.lock().await;
            Response::Games(eng.build_games())
        }
        Request::Logs { lines } => {
            let eng = engine.lock().await;
            let lines = eng.get_recent_logs(lines);
            Response::Logs(protocol::LogsPayload { lines })
        }
        Request::Ping => Response::Pong,
        Request::Shutdown => {
            // Signal engine to stop
            let mut eng = engine.lock().await;
            let _ = eng.stop_idle(None).await;
            Response::Ok {
                message: "Daemon shutting down".into(),
            }
        }
    }
}
