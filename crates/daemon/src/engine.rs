use anyhow::{bail, Result};
use protocol::{
    ActiveGameInfo, Config, DaemonState, GameDatabase, GameEntry, GamesPayload, StatusPayload,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use steam::SteamClient;
use tracing::{debug, error, info, warn};

use crate::process::ProcessDetector;

pub struct Engine {
    state: DaemonState,
    config: Config,
    steam: Box<dyn SteamClient>,
    game_db: GameDatabase,
    process_detector: ProcessDetector,

    /// Target games that user requested to idle
    target_appids: Vec<u32>,
    /// Track when each game began idling
    session_starts: HashMap<u32, Instant>,
    /// Global start time of daemon
    daemon_start: Instant,
    /// Whether idle should be resumed when Steam reconnects
    should_resume_after_steam_reconnect: bool,
    /// Manual override flag (set by `cs stop`, cleared by `cs start`)
    manual_stop: bool,
    /// Timestamp of last Steam connection retry attempt
    last_steam_retry: Option<Instant>,
    /// Resolved socket path
    socket_path: PathBuf,
}

impl Engine {
    pub fn new(config: Config, steam: Box<dyn SteamClient>) -> Self {
        let resume_delay = config.resume_delay_seconds;
        let socket_path = config.get_socket_path();
        let default_games = config.games.clone();

        Self {
            state: DaemonState::Disconnected,
            config,
            steam,
            game_db: GameDatabase::new(),
            process_detector: ProcessDetector::new_cs2(resume_delay),
            target_appids: default_games,
            session_starts: HashMap::new(),
            daemon_start: Instant::now(),
            should_resume_after_steam_reconnect: false,
            manual_stop: false,
            last_steam_retry: None,
            socket_path,
        }
    }

    #[allow(dead_code)]
    pub fn state(&self) -> DaemonState {
        self.state
    }

    #[allow(dead_code)]
    pub fn is_manual_stop(&self) -> bool {
        self.manual_stop
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Attempt initial connection to Steam.
    pub async fn initialize_steam(&mut self) -> Result<()> {
        let retry_secs = self.config.get_steam_retry_duration().as_secs();
        self.transition_to(DaemonState::Connecting)?;
        info!("daemon started");
        info!("checking Steam connection");

        if !self.steam.is_steam_running().await {
            warn!("Steam is not running");
            info!("retrying Steam connection in {} seconds", retry_secs);
            self.last_steam_retry = Some(Instant::now());
            self.transition_to(DaemonState::SteamDisconnected)?;
            return Ok(());
        }

        match self.steam.connect().await {
            Ok(_) => {
                info!("Steam detected");
                info!("connected to Steam");
                self.transition_to(DaemonState::IdleStopped)?;

                // If config has enabled = true and not manually stopped, auto-start idle
                if self.config.enabled && !self.manual_stop {
                    let games = self.target_appids.clone();
                    info!("starting idle for appid={:?}", games);
                    if let Err(e) = self.start_idle(games).await {
                        error!("Failed to auto-start idle: {}", e);
                    } else {
                        info!("idle active");
                    }
                }
                Ok(())
            }
            Err(e) => {
                warn!("Steam is not running: {}", e);
                info!("retrying Steam connection in {} seconds", retry_secs);
                self.last_steam_retry = Some(Instant::now());
                self.transition_to(DaemonState::SteamDisconnected)?;
                Ok(())
            }
        }
    }

    /// Internal state transition with validation.
    fn transition_to(&mut self, next: DaemonState) -> Result<()> {
        if self.state == next {
            return Ok(());
        }
        if !self.state.can_transition_to(next) {
            bail!("Illegal state transition from {} to {}", self.state, next);
        }
        debug!("State transition: {} -> {}", self.state, next);
        self.state = next;
        Ok(())
    }

    /// Start idling specified appids (clears manual override).
    pub async fn start_idle(&mut self, appids: Vec<u32>) -> Result<()> {
        // Clear manual stop override
        if self.manual_stop {
            info!("Manual stop cleared. Starting idle presence...");
            self.manual_stop = false;
        }

        let games_to_run = if appids.is_empty() {
            if self.config.games.is_empty() {
                vec![730] // Default CS2
            } else {
                self.config.games.clone()
            }
        } else {
            appids
        };

        if !self.state.is_steam_connected() {
            bail!("Cannot start idle: Steam client is not connected");
        }

        // Check if real CS2 is running right now
        if self.process_detector.is_process_running() {
            bail!("Real game process is currently running! Cannot start idle presence.");
        }

        // Duplicate start check: already idling identical games
        if self.state == DaemonState::IdleRunning && self.target_appids == games_to_run {
            info!(
                "Idle presence already active for requested games: {:?}",
                games_to_run
            );
            return Ok(());
        }

        self.transition_to(DaemonState::IdleStarting)?;
        info!("Starting idle presence for AppIDs: {:?}", games_to_run);

        match self.steam.set_games_played(&games_to_run).await {
            Ok(_) => {
                self.target_appids = games_to_run.clone();
                let now = Instant::now();
                self.session_starts.clear();
                for &id in &games_to_run {
                    self.session_starts.insert(id, now);
                }
                self.transition_to(DaemonState::IdleRunning)?;
                info!("Idle presence successfully enabled");
                Ok(())
            }
            Err(e) => {
                error!("Failed to set games played: {}", e);
                let _ = self.transition_to(DaemonState::IdleStopped);
                Err(e)
            }
        }
    }

    /// Stop all active idling games (activates manual override).
    pub async fn stop_all(&mut self) -> Result<()> {
        // Activate manual override
        self.manual_stop = true;
        self.should_resume_after_steam_reconnect = false;

        if self.state == DaemonState::IdleStopped {
            info!("Idle presence is already stopped (manual override active)");
            return Ok(());
        }

        info!("Stopping all active idle presences (manual override active)");
        let _ = self.steam.clear_games_played().await;
        self.session_starts.clear();
        self.transition_to(DaemonState::IdleStopped)?;
        info!("Game presence stopped");
        Ok(())
    }

    /// Stop idling.
    pub async fn stop_idle(&mut self, appids: Option<Vec<u32>>) -> Result<()> {
        match appids {
            None => self.stop_all().await,
            Some(ids) => {
                let remaining: Vec<u32> = self
                    .target_appids
                    .iter()
                    .cloned()
                    .filter(|id| !ids.contains(id))
                    .collect();

                if remaining.is_empty() {
                    self.stop_all().await
                } else {
                    self.steam.set_games_played(&remaining).await?;
                    for id in ids {
                        self.session_starts.remove(&id);
                    }
                    self.target_appids = remaining;
                    Ok(())
                }
            }
        }
    }

    /// Periodic maintenance tick (heartbeat, real game detector, reconnect timer).
    pub async fn tick(&mut self) -> Result<()> {
        let retry_duration = self.config.get_steam_retry_duration();
        let retry_secs = retry_duration.as_secs();

        // 1. Steam connection monitoring when connected
        if self.state.is_steam_connected() {
            let is_running = self.steam.is_steam_running().await;
            if !is_running {
                warn!("Steam client connection lost!");
                if self.state == DaemonState::IdleRunning && !self.manual_stop {
                    self.should_resume_after_steam_reconnect = true;
                }
                let _ = self.steam.clear_games_played().await;
                self.transition_to(DaemonState::SteamDisconnected)?;
                info!("retrying Steam connection in {} seconds", retry_secs);
                self.last_steam_retry = Some(Instant::now());
                return Ok(());
            }
        }

        // 2. Reconnection retry loop when Steam is disconnected
        if self.state == DaemonState::SteamDisconnected || self.state == DaemonState::Reconnecting {
            let should_retry = match self.last_steam_retry {
                Some(last) => Instant::now().duration_since(last) >= retry_duration,
                None => true,
            };

            if should_retry {
                self.last_steam_retry = Some(Instant::now());
                info!("checking Steam connection");

                if !self.steam.is_steam_running().await {
                    warn!("Steam is still not running");
                    info!("retrying Steam connection in {} seconds", retry_secs);
                    return Ok(());
                }

                info!("Steam detected");
                self.transition_to(DaemonState::Reconnecting)?;
                match self.steam.connect().await {
                    Ok(_) => {
                        info!("connected to Steam");
                        self.transition_to(DaemonState::Connected)?;

                        // If NOT manually stopped, auto-resume or auto-start idle
                        if !self.manual_stop
                            && (self.should_resume_after_steam_reconnect || self.config.enabled)
                        {
                            let games = self.target_appids.clone();
                            info!("starting idle for appid={:?}", games);
                            if let Err(e) = self.start_idle(games).await {
                                error!("Failed to resume idle: {}", e);
                            } else {
                                info!("idle active");
                            }
                            self.should_resume_after_steam_reconnect = false;
                        } else {
                            if self.manual_stop {
                                info!("Manual stop active - idle not automatically started");
                            }
                            self.transition_to(DaemonState::IdleStopped)?;
                        }
                    }
                    Err(e) => {
                        warn!("Steam connection attempt failed: {}", e);
                        info!("retrying Steam connection in {} seconds", retry_secs);
                        self.transition_to(DaemonState::SteamDisconnected)?;
                    }
                }
            }
            return Ok(());
        }

        // 3. Real CS2 process detection & auto-resume logic
        if self.config.auto_resume {
            let (_is_running, just_started, just_exited) = self.process_detector.update();

            if just_started {
                if self.state == DaemonState::IdleRunning {
                    info!("Real game launched! Suspending idle presence to prevent conflict...");
                    let _ = self.steam.clear_games_played().await;
                    self.transition_to(DaemonState::SuspendedForRealGame)?;
                }
            } else if just_exited && self.state == DaemonState::SuspendedForRealGame {
                if !self.manual_stop {
                    info!("Real game closed. Resuming idle presence...");
                    let games = self.target_appids.clone();
                    self.transition_to(DaemonState::Connected)?;
                    let _ = self.start_idle(games).await;
                } else {
                    info!("Real game closed, but manual stop is active. Idle remains stopped.");
                    self.transition_to(DaemonState::IdleStopped)?;
                }
            }
        }

        Ok(())
    }

    /// Build detailed status payload for CLI.
    pub fn build_status(&self) -> StatusPayload {
        let now = Instant::now();
        let mut active_games = Vec::new();
        let mut total_session_secs = 0;

        if self.state == DaemonState::IdleRunning {
            for &appid in &self.target_appids {
                let duration = self
                    .session_starts
                    .get(&appid)
                    .map(|start| now.duration_since(*start).as_secs())
                    .unwrap_or(0);

                if duration > total_session_secs {
                    total_session_secs = duration;
                }

                active_games.push(ActiveGameInfo {
                    appid,
                    name: self.game_db.get_name(appid),
                    session_duration_secs: duration,
                });
            }
        }

        StatusPayload {
            daemon_state: self.state,
            steam_connected: self.state.is_steam_connected(),
            idle_active: self.state.is_idle_active(),
            active_games,
            total_session_secs,
            real_cs2_running: self.process_detector.is_active(),
            auto_resume_enabled: self.config.auto_resume,
            manual_stop: self.manual_stop,
            uptime_secs: now.duration_since(self.daemon_start).as_secs(),
            socket_path: self.socket_path.clone(),
        }
    }

    /// Build games information payload.
    pub fn build_games(&self) -> GamesPayload {
        let configured: Vec<GameEntry> = self
            .config
            .games
            .iter()
            .map(|&appid| GameEntry {
                appid,
                name: self.game_db.get_name(appid),
                is_default: appid == 730,
                is_active: self.target_appids.contains(&appid) && self.state.is_idle_active(),
            })
            .collect();

        let active: Vec<GameEntry> = self
            .target_appids
            .iter()
            .map(|&appid| GameEntry {
                appid,
                name: self.game_db.get_name(appid),
                is_default: appid == 730,
                is_active: self.state.is_idle_active(),
            })
            .collect();

        GamesPayload {
            configured_games: configured,
            active_games: active,
        }
    }

    /// Read last N lines from the daemon log file.
    pub fn get_recent_logs(&self, lines_count: usize) -> Vec<String> {
        let log_path = self.config.get_log_path();
        if !log_path.exists() {
            return vec!["Log file does not exist yet.".into()];
        }

        match std::fs::read_to_string(&log_path) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                let start = if lines.len() > lines_count {
                    lines.len() - lines_count
                } else {
                    0
                };
                lines[start..].iter().map(|s| s.to_string()).collect()
            }
            Err(e) => vec![format!("Failed to read log file: {}", e)],
        }
    }
}
