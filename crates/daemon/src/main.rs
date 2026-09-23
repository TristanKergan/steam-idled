use anyhow::Result;
use clap::{Parser, Subcommand};
use daemon::{run_worker, Engine, UnixServer};
use protocol::Config;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use steam::RealSteamClient;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser, Debug)]
#[command(
    name = "steam-idled",
    author,
    version,
    about = "Steam Game Presence & Playtime Daemon"
)]
struct Cli {
    /// Path to custom config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Override target AppID on launch
    #[arg(long)]
    appid: Option<u32>,

    /// Subcommands
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Internal worker process that maintains Steamworks presence for a specific AppID
    Worker {
        #[arg(long)]
        appid: u32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // If invoked as worker, run internal worker logic directly
    if let Some(Commands::Worker { appid }) = cli.command {
        return run_worker(appid);
    }

    // Load configuration
    let mut config = if let Some(config_path) = cli.config {
        Config::load_from_file(&config_path)?
    } else {
        Config::load_or_default()
    };

    if let Some(appid) = cli.appid {
        config.games = vec![appid];
        config.enabled = true;
    }

    // Initialize logging: both stdout and file log (~/.local/state/steam-idled/steam-idled.log)
    let log_path = config.get_log_path();
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let file_appender = tracing_appender::rolling::never(
        log_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(".")),
        log_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("steam-idled.log"),
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,steam_idled=debug,steam=debug"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(non_blocking),
        )
        .init();

    info!("Starting steam-idled daemon v{}", env!("CARGO_PKG_VERSION"));
    info!("Using socket: {}", config.get_socket_path().display());
    info!("Using log file: {}", log_path.display());

    // Create real steam client
    let steam_client = RealSteamClient::new();
    let socket_path = config.get_socket_path();
    let engine = Arc::new(Mutex::new(Engine::new(config, Box::new(steam_client))));

    // Initialize Steam connection
    {
        let mut eng = engine.lock().await;
        if let Err(e) = eng.initialize_steam().await {
            error!("Initial Steam connection error: {}", e);
        }
    }

    let (shutdown_tx, _) = broadcast::channel::<()>(4);

    // Spawn periodic maintenance loop
    let tick_engine = engine.clone();
    let mut tick_shutdown = shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1000));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let mut eng = tick_engine.lock().await;
                    if let Err(e) = eng.tick().await {
                        error!("Engine tick error: {}", e);
                    }
                }
                _ = tick_shutdown.recv() => {
                    break;
                }
            }
        }
    });

    // Spawn Unix Socket Server
    let server = UnixServer::new(socket_path.clone(), engine.clone(), shutdown_tx.clone());
    let server_shutdown = shutdown_tx.subscribe();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.run(server_shutdown).await {
            error!("Unix socket server error: {}", e);
        }
    });

    // Setup OS signal listeners (SIGTERM, SIGINT) and IPC shutdown listener
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut ipc_shutdown = shutdown_tx.subscribe();

    tokio::select! {
        _ = sigterm.recv() => {
            info!("Received SIGTERM. Initiating graceful shutdown...");
            let _ = shutdown_tx.send(());
        }
        _ = sigint.recv() => {
            info!("Received SIGINT. Initiating graceful shutdown...");
            let _ = shutdown_tx.send(());
        }
        _ = ipc_shutdown.recv() => {
            info!("Received IPC shutdown command. Initiating graceful shutdown...");
        }
    }

    // Stop active idling sessions
    {
        info!("Stopping active game sessions...");
        let mut eng = engine.lock().await;
        let _ = eng.stop_idle(None).await;
    }

    let _ = server_handle.await;

    // Clean up socket file
    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }

    info!("steam-idled daemon shutdown completed successfully");
    Ok(())
}
