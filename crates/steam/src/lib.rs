pub mod mock;
pub mod real;

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait SteamClient: Send + Sync {
    /// Check if Steam client is running on the host system.
    async fn is_steam_running(&self) -> bool;

    /// Connect to the Steam client.
    async fn connect(&mut self) -> Result<()>;

    /// Set games actively played.
    async fn set_games_played(&mut self, appids: &[u32]) -> Result<()>;

    /// Clear all active games played.
    async fn clear_games_played(&mut self) -> Result<()>;

    /// Check if currently connected to Steam.
    async fn is_connected(&self) -> bool;

    /// Return list of currently active AppIDs.
    async fn active_appids(&self) -> Vec<u32>;

    /// Clean disconnect.
    async fn disconnect(&mut self) -> Result<()>;
}

pub use mock::MockSteamClient;
pub use real::RealSteamClient;
