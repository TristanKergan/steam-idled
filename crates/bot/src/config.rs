use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    /// Telegram bot token obtained from @BotFather
    pub bot_token: String,
    /// Telegram user ID allowed to control the bot (from @userinfobot)
    pub admin_user_id: i64,
    /// Custom path to the daemon Unix domain socket (optional)
    pub socket_path: Option<PathBuf>,
    /// Long-polling timeout in seconds for Telegram updates
    #[serde(default = "default_poll_timeout")]
    pub poll_timeout_secs: u64,
}

fn default_poll_timeout() -> u64 {
    30
}

impl BotConfig {
    pub fn default_config_path() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(home).join(".config")
            });
        base.join("steam-idled").join("bot.toml")
    }

    pub fn load_or_env(custom_path: Option<&Path>) -> Result<Self> {
        let path = custom_path
            .map(|p| p.to_path_buf())
            .unwrap_or_else(Self::default_config_path);

        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read bot config at {}", path.display()))?;
            let mut cfg: Self = toml::from_str(&content)
                .with_context(|| format!("Failed to parse TOML from {}", path.display()))?;

            // Allow environment variables to override if present
            if let Ok(token) =
                std::env::var("STEAM_IDLED_BOT_TOKEN").or_else(|_| std::env::var("BOT_TOKEN"))
            {
                if !token.trim().is_empty() {
                    cfg.bot_token = token.trim().to_string();
                }
            }
            if let Ok(admin_str) =
                std::env::var("STEAM_IDLED_ADMIN_ID").or_else(|_| std::env::var("ADMIN_USER_ID"))
            {
                if let Ok(id) = admin_str.trim().parse::<i64>() {
                    cfg.admin_user_id = id;
                }
            }
            return Ok(cfg);
        }

        // Try environment variables if config file doesn't exist
        let token = std::env::var("STEAM_IDLED_BOT_TOKEN")
            .or_else(|_| std::env::var("BOT_TOKEN"))
            .with_context(|| {
                format!(
                    "Bot configuration not found at {}. Either create it or set STEAM_IDLED_BOT_TOKEN.",
                    path.display()
                )
            })?;

        let admin_str = std::env::var("STEAM_IDLED_ADMIN_ID")
            .or_else(|_| std::env::var("ADMIN_USER_ID"))
            .with_context(|| {
                "Admin Telegram User ID not set in config or STEAM_IDLED_ADMIN_ID env"
            })?;

        let admin_user_id: i64 = admin_str.trim().parse().with_context(|| {
            format!("Invalid admin user ID '{}': must be an integer", admin_str)
        })?;

        Ok(Self {
            bot_token: token.trim().to_string(),
            admin_user_id,
            socket_path: None,
            poll_timeout_secs: default_poll_timeout(),
        })
    }

    pub fn example_toml() -> &'static str {
        r#"# steam-idled-bot configuration
# Place this at ~/.config/steam-idled/bot.toml

# Bot token from https://t.me/BotFather
bot_token = "YOUR_TELEGRAM_BOT_TOKEN"

# Your Telegram User ID (numeric, e.g. from https://t.me/userinfobot)
admin_user_id = 123456789

# Optional: custom socket path
# socket_path = "/run/user/1000/steam-idled.sock"

# Long-polling timeout in seconds
poll_timeout_secs = 30
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bot_config() {
        let toml_str = r#"
            bot_token = "123456:ABC-DEF"
            admin_user_id = 987654321
            poll_timeout_secs = 25
        "#;
        let cfg: BotConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.bot_token, "123456:ABC-DEF");
        assert_eq!(cfg.admin_user_id, 987654321);
        assert_eq!(cfg.poll_timeout_secs, 25);
    }
}
