mod config;
mod handler;
mod metrics;
mod tg;

use anyhow::Result;
use clap::Parser;
use config::BotConfig;
use handler::BotHandler;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tg::TgClient;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "steam-idled-bot",
    author = "TryDkg",
    version,
    about = "Telegram Bot for remote steam-idled daemon management and telemetry"
)]
struct Cli {
    /// Path to bot configuration TOML file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Override Telegram bot token
    #[arg(short, long)]
    token: Option<String>,

    /// Override Admin Telegram user ID
    #[arg(short, long)]
    admin_id: Option<i64>,

    /// Override Unix domain socket path
    #[arg(short, long)]
    socket: Option<PathBuf>,

    /// Print an example bot.toml configuration and exit
    #[arg(long)]
    example_config: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "steam_idled_bot=info,warn".into()),
        )
        .init();

    let cli = Cli::parse();

    if cli.example_config {
        println!("{}", BotConfig::example_toml());
        return Ok(());
    }

    // Load or resolve config
    let mut cfg = match BotConfig::load_or_env(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration Error: {}", e);
            eprintln!("\nTo configure the bot, either create ~/.config/steam-idled/bot.toml:\n");
            eprintln!("{}", BotConfig::example_toml());
            eprintln!("Or set environment variables:");
            eprintln!("  export STEAM_IDLED_BOT_TOKEN=\"your_token\"");
            eprintln!("  export STEAM_IDLED_ADMIN_ID=\"your_telegram_id\"\n");
            std::process::exit(1);
        }
    };

    if let Some(t) = cli.token {
        cfg.bot_token = t;
    }
    if let Some(a) = cli.admin_id {
        cfg.admin_user_id = a;
    }
    if let Some(s) = cli.socket {
        cfg.socket_path = Some(s);
    }

    info!("Starting steam-idled-bot...");
    let tg = TgClient::new(&cfg.bot_token);

    // Verify bot token
    match tg.get_me().await {
        Ok(me) => {
            info!(
                "Connected to Telegram as @{} (ID: {})",
                me.username.as_deref().unwrap_or("unknown"),
                me.id
            );
            info!("Authorized admin user ID: {}", cfg.admin_user_id);
        }
        Err(e) => {
            error!(
                "Failed to authenticate bot token with Telegram: {}. Check bot_token!",
                e
            );
            std::process::exit(1);
        }
    }

    let poll_timeout = cfg.poll_timeout_secs;
    let handler = Arc::new(BotHandler::new(cfg, tg.clone()));

    let mut offset: Option<i64> = None;

    info!("Bot is listening for commands. Press Ctrl+C to stop.");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received, stopping bot...");
                break;
            }
            updates_res = tg.get_updates(offset, poll_timeout) => {
                match updates_res {
                    Ok(updates) => {
                        for upd in updates {
                            offset = Some(upd.update_id + 1);

                            let h = Arc::clone(&handler);
                            if let Some(msg) = upd.message {
                                tokio::spawn(async move {
                                    if let Err(e) = h.handle_message(msg).await {
                                        warn!("Error handling message: {}", e);
                                    }
                                });
                            } else if let Some(cb) = upd.callback_query {
                                tokio::spawn(async move {
                                    if let Err(e) = h.handle_callback_query(cb).await {
                                        warn!("Error handling callback: {}", e);
                                    }
                                });
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Network or Telegram API error: {}. Retrying in 5s...", e);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }

    info!("steam-idled-bot stopped.");
    Ok(())
}
