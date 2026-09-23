use crate::SteamClient;
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockEvent {
    Connected,
    Disconnected,
    SetGames(Vec<u32>),
    ClearedGames,
}

/// In-memory mock implementation of `SteamClient` for unit and integration testing.
#[derive(Clone)]
pub struct MockSteamClient {
    steam_running: Arc<Mutex<bool>>,
    connected: Arc<Mutex<bool>>,
    active_games: Arc<Mutex<Vec<u32>>>,
    events: Arc<Mutex<Vec<MockEvent>>>,
}

impl Default for MockSteamClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockSteamClient {
    pub fn new() -> Self {
        Self {
            steam_running: Arc::new(Mutex::new(true)),
            connected: Arc::new(Mutex::new(false)),
            active_games: Arc::new(Mutex::new(Vec::new())),
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Simulate whether the Steam client desktop application is running.
    pub fn set_steam_running(&self, running: bool) {
        *self.steam_running.lock().unwrap() = running;
        if !running {
            *self.connected.lock().unwrap() = false;
            self.active_games.lock().unwrap().clear();
            self.events.lock().unwrap().push(MockEvent::Disconnected);
        }
    }

    /// Simulate sudden network / Steam client disconnect.
    pub fn simulate_disconnect(&self) {
        *self.connected.lock().unwrap() = false;
        self.active_games.lock().unwrap().clear();
        self.events.lock().unwrap().push(MockEvent::Disconnected);
    }

    /// Retrieve recorded events.
    pub fn get_events(&self) -> Vec<MockEvent> {
        self.events.lock().unwrap().clone()
    }
}

#[async_trait]
impl SteamClient for MockSteamClient {
    async fn is_steam_running(&self) -> bool {
        *self.steam_running.lock().unwrap()
    }

    async fn connect(&mut self) -> Result<()> {
        if !*self.steam_running.lock().unwrap() {
            bail!("Steam client is not running");
        }
        *self.connected.lock().unwrap() = true;
        self.events.lock().unwrap().push(MockEvent::Connected);
        Ok(())
    }

    async fn set_games_played(&mut self, appids: &[u32]) -> Result<()> {
        if !*self.connected.lock().unwrap() {
            bail!("Cannot set games: not connected to Steam");
        }
        let mut games = self.active_games.lock().unwrap();
        games.clear();
        games.extend_from_slice(appids);
        self.events
            .lock()
            .unwrap()
            .push(MockEvent::SetGames(appids.to_vec()));
        Ok(())
    }

    async fn clear_games_played(&mut self) -> Result<()> {
        self.active_games.lock().unwrap().clear();
        self.events.lock().unwrap().push(MockEvent::ClearedGames);
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }

    async fn active_appids(&self) -> Vec<u32> {
        self.active_games.lock().unwrap().clone()
    }

    async fn disconnect(&mut self) -> Result<()> {
        *self.connected.lock().unwrap() = false;
        self.active_games.lock().unwrap().clear();
        self.events.lock().unwrap().push(MockEvent::Disconnected);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_lifecycle() {
        let mut client = MockSteamClient::new();
        assert!(client.is_steam_running().await);
        assert!(!client.is_connected().await);

        client.connect().await.unwrap();
        assert!(client.is_connected().await);

        client.set_games_played(&[730]).await.unwrap();
        assert_eq!(client.active_appids().await, vec![730]);

        client.clear_games_played().await.unwrap();
        assert!(client.active_appids().await.is_empty());

        client.simulate_disconnect();
        assert!(!client.is_connected().await);
    }
}
