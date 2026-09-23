use crate::SteamClient;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{debug, error, info};

/// Represents an active worker child process running game presence for an AppID.
struct WorkerProcess {
    _appid: u32,
    child: Child,
}

pub struct RealSteamClient {
    connected: bool,
    workers: HashMap<u32, WorkerProcess>,
    executable_path: Option<PathBuf>,
}

impl Default for RealSteamClient {
    fn default() -> Self {
        Self::new()
    }
}

impl RealSteamClient {
    pub fn new() -> Self {
        let exe = std::env::current_exe().ok();
        Self {
            connected: false,
            workers: HashMap::new(),
            executable_path: exe,
        }
    }

    /// Set an explicit executable path (useful for testing or specific installs).
    pub fn with_executable(mut self, path: PathBuf) -> Self {
        self.executable_path = Some(path);
        self
    }

    /// Check if the Steam desktop client process is running on Linux.
    pub fn check_steam_running_sync() -> bool {
        // Method 1: Check ~/.steam/steam.pid
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let pid_path = Path::new(&home).join(".steam").join("steam.pid");
        if pid_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&pid_path) {
                if let Ok(pid) = content.trim().parse::<i32>() {
                    let proc_path = PathBuf::from(format!("/proc/{}", pid));
                    if proc_path.exists() {
                        return true;
                    }
                }
            }
        }

        // Method 2: Scan /proc for any process named "steam"
        if let Ok(entries) = std::fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if let Some(name_str) = name.to_str() {
                    if name_str.chars().all(|c| c.is_ascii_digit()) {
                        let comm_path = entry.path().join("comm");
                        if let Ok(comm) = std::fs::read_to_string(comm_path) {
                            if comm.trim() == "steam" {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Method 3: Check ~/.steam/steam.pipe or /tmp/steam_chrome_shmem_*
        let pipe_path = Path::new(&home).join(".steam").join("steam.pipe");
        pipe_path.exists()
    }

    /// Spawn a worker child process for a given AppID.
    async fn spawn_worker(&self, appid: u32) -> Result<WorkerProcess> {
        let exe = self
            .executable_path
            .clone()
            .or_else(|| std::env::current_exe().ok())
            .context("Cannot determine current executable path")?;

        info!("Spawning idle worker for AppID {}", appid);

        let mut cmd = Command::new(&exe);
        cmd.arg("worker")
            .arg("--appid")
            .arg(appid.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn worker for AppID {}", appid))?;

        let stdout = child
            .stdout
            .take()
            .context("Failed to take worker stdout pipe")?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        // Wait for ready handshake from worker
        tokio::select! {
            res = reader.read_line(&mut line) => {
                match res {
                    Ok(n) if n > 0 => {
                        let trimmed = line.trim();
                        if trimmed.starts_with("READY") {
                            debug!("Worker for AppID {} reported READY: {}", appid, trimmed);
                            Ok(WorkerProcess { _appid: appid, child })
                        } else {
                            let _ = child.kill().await;
                            bail!("Worker for AppID {} failed init: {}", appid, trimmed);
                        }
                    }
                    Ok(_) => {
                        let _ = child.kill().await;
                        bail!("Worker for AppID {} closed stdout without handshake", appid);
                    }
                    Err(e) => {
                        let _ = child.kill().await;
                        bail!("Failed to read handshake from worker for AppID {}: {}", appid, e);
                    }
                }
            }
            status = child.wait() => {
                bail!("Worker for AppID {} exited prematurely with status: {:?}", appid, status);
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                let _ = child.kill().await;
                bail!("Timeout waiting for worker AppID {} to initialize Steamworks", appid);
            }
        }
    }
}

#[async_trait]
impl SteamClient for RealSteamClient {
    async fn is_steam_running(&self) -> bool {
        tokio::task::spawn_blocking(Self::check_steam_running_sync)
            .await
            .unwrap_or(false)
    }

    async fn connect(&mut self) -> Result<()> {
        if !self.is_steam_running().await {
            bail!("Steam client is not running on this system");
        }
        self.connected = true;
        info!("Successfully verified Steam client connection");
        Ok(())
    }

    async fn set_games_played(&mut self, appids: &[u32]) -> Result<()> {
        if !self.connected {
            bail!("Not connected to Steam client");
        }

        // Identify AppIDs to stop
        let current_appids: Vec<u32> = self.workers.keys().cloned().collect();
        for id in current_appids {
            if !appids.contains(&id) {
                if let Some(mut worker) = self.workers.remove(&id) {
                    info!("Stopping idle worker for AppID {}", id);
                    let _ = worker.child.kill().await;
                    let _ = worker.child.wait().await;
                }
            }
        }

        // Identify AppIDs to start
        for &appid in appids {
            if !self.workers.contains_key(&appid) {
                match self.spawn_worker(appid).await {
                    Ok(worker) => {
                        self.workers.insert(appid, worker);
                    }
                    Err(e) => {
                        error!("Failed to start worker for AppID {}: {}", appid, e);
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn clear_games_played(&mut self) -> Result<()> {
        info!("Clearing all active game presences");
        for (id, mut worker) in self.workers.drain() {
            debug!("Terminating worker for AppID {}", id);
            let _ = worker.child.kill().await;
            let _ = worker.child.wait().await;
        }
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        if !self.connected {
            return false;
        }
        self.is_steam_running().await
    }

    async fn active_appids(&self) -> Vec<u32> {
        let mut list: Vec<u32> = self.workers.keys().cloned().collect();
        list.sort();
        list
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.clear_games_played().await?;
        self.connected = false;
        Ok(())
    }
}
