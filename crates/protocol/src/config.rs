use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("I/O error reading config at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to parse TOML config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Failed to serialize TOML config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

fn default_enabled() -> bool {
    true
}

fn default_games() -> Vec<u32> {
    vec![730] // Counter-Strike 2
}

fn default_auto_resume() -> bool {
    true
}

fn default_resume_delay_seconds() -> u64 {
    10
}

fn default_steam_retry_seconds() -> u64 {
    15
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_games")]
    pub games: Vec<u32>,

    #[serde(default = "default_auto_resume")]
    pub auto_resume: bool,

    #[serde(default = "default_resume_delay_seconds")]
    pub resume_delay_seconds: u64,

    #[serde(default = "default_steam_retry_seconds")]
    pub steam_retry_seconds: u64,

    #[serde(default)]
    pub socket_path: Option<PathBuf>,

    #[serde(default)]
    pub log_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            games: default_games(),
            auto_resume: default_auto_resume(),
            resume_delay_seconds: default_resume_delay_seconds(),
            steam_retry_seconds: default_steam_retry_seconds(),
            socket_path: None,
            log_path: None,
        }
    }
}

impl Config {
    /// Returns standard config directory: ~/.config/steam-idled/
    pub fn default_config_dir() -> PathBuf {
        if let Ok(val) = std::env::var("XDG_CONFIG_HOME") {
            if !val.trim().is_empty() {
                return PathBuf::from(val).join("steam-idled");
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".config").join("steam-idled")
    }

    /// Returns standard config file path: ~/.config/steam-idled/config.toml
    pub fn default_config_path() -> PathBuf {
        Self::default_config_dir().join("config.toml")
    }

    /// Returns standard socket path:
    /// Priority 1: $XDG_RUNTIME_DIR/steam-idled.sock
    /// Priority 2: ~/.run/steam-idled.sock
    pub fn default_socket_path() -> PathBuf {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            if !runtime_dir.trim().is_empty() {
                return PathBuf::from(runtime_dir).join("steam-idled.sock");
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".run").join("steam-idled.sock")
    }

    /// Returns standard state directory: ~/.local/state/steam-idled/
    pub fn default_state_dir() -> PathBuf {
        if let Ok(val) = std::env::var("XDG_STATE_HOME") {
            if !val.trim().is_empty() {
                return PathBuf::from(val).join("steam-idled");
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("steam-idled")
    }

    /// Returns standard log path: ~/.local/state/steam-idled/steam-idled.log
    pub fn default_log_path() -> PathBuf {
        Self::default_state_dir().join("steam-idled.log")
    }

    /// Resolved socket path from config or fallback
    pub fn get_socket_path(&self) -> PathBuf {
        self.socket_path
            .clone()
            .unwrap_or_else(Self::default_socket_path)
    }

    /// Resolved log path from config or fallback
    pub fn get_log_path(&self) -> PathBuf {
        self.log_path.clone().unwrap_or_else(Self::default_log_path)
    }

    /// Get validated retry duration for Steam reconnect (falls back to 15s if <= 0)
    pub fn get_steam_retry_duration(&self) -> Duration {
        if self.steam_retry_seconds == 0 {
            Duration::from_secs(15)
        } else {
            Duration::from_secs(self.steam_retry_seconds)
        }
    }

    /// Load config from given file or use defaults.
    pub fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path).map_err(|e| ConfigError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let cfg: Config = toml::from_str(&content)?;
        Ok(cfg)
    }

    /// Load from standard location or return default config.
    pub fn load_or_default() -> Self {
        let path = Self::default_config_path();
        if path.exists() {
            match Self::load_from_file(&path) {
                Ok(cfg) => cfg,
                Err(err) => {
                    eprintln!(
                        "Warning: Failed to parse config at {}: {}",
                        path.display(),
                        err
                    );
                    Config::default()
                }
            }
        } else {
            Config::default()
        }
    }

    /// Save configuration to file.
    pub fn save_to_file(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| ConfigError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content).map_err(|e| ConfigError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.games, vec![730]);
        assert!(cfg.auto_resume);
        assert_eq!(cfg.resume_delay_seconds, 10);
        assert_eq!(cfg.steam_retry_seconds, 15);
        assert_eq!(cfg.get_steam_retry_duration(), Duration::from_secs(15));
    }

    #[test]
    fn test_parse_toml() {
        let raw = r#"
            enabled = false
            games = [730, 570, 440]
            auto_resume = false
            resume_delay_seconds = 20
            steam_retry_seconds = 30
        "#;
        let cfg: Config = toml::from_str(raw).expect("Failed to parse");
        assert!(!cfg.enabled);
        assert_eq!(cfg.games, vec![730, 570, 440]);
        assert!(!cfg.auto_resume);
        assert_eq!(cfg.resume_delay_seconds, 20);
        assert_eq!(cfg.steam_retry_seconds, 30);
        assert_eq!(cfg.get_steam_retry_duration(), Duration::from_secs(30));
    }

    #[test]
    fn test_fallback_zero_retry() {
        let cfg = Config {
            steam_retry_seconds: 0,
            ..Default::default()
        };
        assert_eq!(cfg.get_steam_retry_duration(), Duration::from_secs(15));
    }

    #[test]
    fn test_save_and_load() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        let cfg = Config {
            games: vec![730, 570],
            ..Default::default()
        };
        cfg.save_to_file(&path).unwrap();

        let loaded = Config::load_from_file(&path).unwrap();
        assert_eq!(cfg, loaded);
    }
}
