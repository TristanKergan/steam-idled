use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Representation of a Steam Game with metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub appid: u32,
    pub name: String,
}

impl Game {
    pub fn new(appid: u32, name: impl Into<String>) -> Self {
        Self {
            appid,
            name: name.into(),
        }
    }
}

/// Metadata provider for Steam AppIDs to friendly names.
pub struct GameDatabase {
    known: HashMap<u32, &'static str>,
}

impl Default for GameDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl GameDatabase {
    pub fn new() -> Self {
        let mut known = HashMap::new();
        known.insert(730, "Counter-Strike 2");
        known.insert(570, "Dota 2");
        known.insert(440, "Team Fortress 2");
        known.insert(252490, "Rust");
        known.insert(10, "Counter-Strike 1.6");
        known.insert(322170, "Geometry Dash");
        known.insert(289070, "Sid Meier's Civilization VI");
        known.insert(105600, "Terraria");
        known.insert(271590, "Grand Theft Auto V");
        known.insert(1172470, "Apex Legends");
        known.insert(550, "Left 4 Dead 2");
        known.insert(4000, "Garry's Mod");
        known.insert(230410, "Warframe");
        known.insert(252950, "Rocket League");
        known.insert(1091500, "Cyberpunk 2077");
        known.insert(221100, "DayZ");
        known.insert(1245620, "ELDEN RING");
        known.insert(578080, "PUBG: BATTLEGROUNDS");
        known.insert(218620, "PAYDAY 2");
        known.insert(227300, "Euro Truck Simulator 2");

        Self { known }
    }

    /// Resolve an AppID to a friendly game name.
    pub fn get_name(&self, appid: u32) -> String {
        if let Some(name) = self.known.get(&appid) {
            (*name).to_string()
        } else {
            format!("AppID {}", appid)
        }
    }

    /// Check if this AppID is recognized in the static database.
    pub fn is_known(&self, appid: u32) -> bool {
        self.known.contains_key(&appid)
    }

    /// List all built-in games.
    pub fn all_known(&self) -> Vec<Game> {
        let mut list: Vec<Game> = self
            .known
            .iter()
            .map(|(&appid, &name)| Game::new(appid, name))
            .collect();
        list.sort_by_key(|g| g.appid);
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_database_lookup() {
        let db = GameDatabase::new();
        assert_eq!(db.get_name(730), "Counter-Strike 2");
        assert_eq!(db.get_name(570), "Dota 2");
        assert_eq!(db.get_name(99999999), "AppID 99999999");
        assert!(db.is_known(730));
        assert!(!db.is_known(99999999));
    }
}
