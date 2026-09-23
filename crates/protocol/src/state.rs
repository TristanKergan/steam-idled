use serde::{Deserialize, Serialize};
use std::fmt;

/// Explicit state of the steam-idled daemon and idle presence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DaemonState {
    /// Daemon started, not yet connected to Steam.
    Disconnected,
    /// Actively establishing connection to local Steam client.
    Connecting,
    /// Connected to Steam client, idle is currently stopped/ready.
    Connected,
    /// Connected to Steam client, idle is explicitly stopped.
    IdleStopped,
    /// Initializing game presence for target AppIDs.
    IdleStarting,
    /// Game presence is actively reported to Steam (idling).
    IdleRunning,
    /// Real CS2 / game process detected, idle presence temporarily cleared.
    SuspendedForRealGame,
    /// Connection to local Steam client was lost (Steam exited/crashed).
    SteamDisconnected,
    /// Waiting and attempting to reconnect to local Steam client.
    Reconnecting,
    /// Daemon is performing clean shutdown.
    Stopping,
}

impl DaemonState {
    /// Returns true if game presence is actively active right now.
    pub fn is_idle_active(&self) -> bool {
        matches!(self, DaemonState::IdleRunning)
    }

    /// Returns true if steam is considered connected.
    pub fn is_steam_connected(&self) -> bool {
        matches!(
            self,
            DaemonState::Connected
                | DaemonState::IdleStopped
                | DaemonState::IdleStarting
                | DaemonState::IdleRunning
                | DaemonState::SuspendedForRealGame
        )
    }

    /// Validates whether a transition from `self` to `target` is legally permitted.
    pub fn can_transition_to(&self, target: DaemonState) -> bool {
        use DaemonState::*;
        match (*self, target) {
            // Self-transitions are no-ops / allowed
            (a, b) if a == b => true,

            // Stopping can be initiated from almost any operational state
            (_, Stopping) => true,

            // Disconnected can go to Connecting or Stopping
            (Disconnected, Connecting) => true,

            // Connecting can succeed to Connected / IdleStopped or fail back
            (Connecting, Connected | IdleStopped | Disconnected | SteamDisconnected) => true,

            // Connected and IdleStopped can interchange, start idle, or lose Steam
            (
                Connected | IdleStopped,
                Connected | IdleStopped | IdleStarting | SteamDisconnected | Disconnected,
            ) => true,

            // Starting can transition to Running or fail back to IdleStopped or SteamDisconnected
            (IdleStarting, IdleRunning | IdleStopped | SteamDisconnected) => true,

            // Running can stop, update/start new games, suspend for real game, or lose Steam
            (
                IdleRunning,
                IdleStarting | IdleStopped | SuspendedForRealGame | SteamDisconnected,
            ) => true,

            // Suspended can resume to Starting, be explicitly Stopped, or lose Steam
            (SuspendedForRealGame, IdleStarting | IdleStopped | SteamDisconnected) => true,

            // Steam disconnected transitions to Reconnecting or stays disconnected
            (SteamDisconnected, Reconnecting | Disconnected) => true,

            // Reconnecting can succeed to Connected / IdleStarting (restoring) or fail back
            (Reconnecting, Connected | IdleStarting | IdleStopped | SteamDisconnected) => true,

            // Stopping finishes in Disconnected
            (Stopping, Disconnected) => true,

            // Disallow any other random jumps
            _ => false,
        }
    }
}

impl fmt::Display for DaemonState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DaemonState::Disconnected => write!(f, "DISCONNECTED"),
            DaemonState::Connecting => write!(f, "CONNECTING"),
            DaemonState::Connected => write!(f, "CONNECTED"),
            DaemonState::IdleStopped => write!(f, "IDLE_STOPPED"),
            DaemonState::IdleStarting => write!(f, "IDLE_STARTING"),
            DaemonState::IdleRunning => write!(f, "IDLE_RUNNING"),
            DaemonState::SuspendedForRealGame => write!(f, "SUSPENDED (REAL GAME RUNNING)"),
            DaemonState::SteamDisconnected => write!(f, "STEAM_DISCONNECTED"),
            DaemonState::Reconnecting => write!(f, "RECONNECTING"),
            DaemonState::Stopping => write!(f, "STOPPING"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        let state = DaemonState::Disconnected;
        assert!(state.can_transition_to(DaemonState::Connecting));
        assert!(state.can_transition_to(DaemonState::Stopping));
        assert!(!state.can_transition_to(DaemonState::IdleRunning));

        let state = DaemonState::Connected;
        assert!(state.can_transition_to(DaemonState::IdleStarting));
        assert!(state.can_transition_to(DaemonState::IdleStopped));
        assert!(state.can_transition_to(DaemonState::SteamDisconnected));

        let state = DaemonState::IdleRunning;
        assert!(state.can_transition_to(DaemonState::IdleStarting));
        assert!(state.can_transition_to(DaemonState::IdleStopped));
        assert!(state.can_transition_to(DaemonState::SuspendedForRealGame));
        assert!(state.can_transition_to(DaemonState::SteamDisconnected));

        let state = DaemonState::SuspendedForRealGame;
        assert!(state.can_transition_to(DaemonState::IdleStarting));
        assert!(state.can_transition_to(DaemonState::IdleStopped));
    }

    #[test]
    fn test_status_helpers() {
        assert!(DaemonState::IdleRunning.is_idle_active());
        assert!(!DaemonState::IdleStopped.is_idle_active());
        assert!(!DaemonState::SuspendedForRealGame.is_idle_active());

        assert!(DaemonState::Connected.is_steam_connected());
        assert!(DaemonState::IdleRunning.is_steam_connected());
        assert!(!DaemonState::SteamDisconnected.is_steam_connected());
        assert!(!DaemonState::Disconnected.is_steam_connected());
    }
}
