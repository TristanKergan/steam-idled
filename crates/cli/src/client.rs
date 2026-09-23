use anyhow::{bail, Result};
use protocol::{Config, IpcClient, Request, Response};
use std::path::{Path, PathBuf};

pub struct DaemonClient {
    ipc: IpcClient,
    socket_path: PathBuf,
}

impl DaemonClient {
    pub async fn connect(custom_socket: Option<&Path>) -> Result<Self> {
        let socket_path = if let Some(p) = custom_socket {
            p.to_path_buf()
        } else {
            Config::default_socket_path()
        };

        if !socket_path.exists() {
            bail!(
                "steam-idled daemon is not running (socket not found at {}).\n\
                Start it using:\n  systemctl --user start steam-idled\n\
                Or run manually:\n  steam-idled &",
                socket_path.display()
            );
        }

        match IpcClient::connect(&socket_path).await {
            Ok(ipc) => Ok(Self { ipc, socket_path }),
            Err(e) => {
                bail!(
                    "Failed to connect to steam-idled daemon at {}: {}\n\
                    Make sure the daemon is running and socket permissions are correct.",
                    socket_path.display(),
                    e
                );
            }
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn call(&mut self, request: &Request) -> Result<Response> {
        self.ipc
            .call(request)
            .await
            .map_err(|e| anyhow::anyhow!("Communication error with daemon: {}", e))
    }
}
