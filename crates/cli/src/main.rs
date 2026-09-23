mod client;
mod output;

use anyhow::Result;
use clap::{Parser, Subcommand};
use client::DaemonClient;
use protocol::{Request, Response};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "cs",
    author,
    version,
    about = "Steam Idle & Game Presence Manager"
)]
struct Cli {
    /// Custom Unix socket path
    #[arg(long, global = true)]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start idling game presence (default: AppID 730 / Counter-Strike 2)
    Start {
        /// Optional AppIDs to idle (e.g., 730 570 440)
        #[arg(value_name = "APPID")]
        appids: Vec<u32>,
    },
    /// Stop idling game presence
    Stop {
        /// Specific AppIDs to stop (leaves others running, or all if omitted)
        #[arg(value_name = "APPID")]
        appids: Vec<u32>,
    },
    /// Show current status of daemon, Steam connection, and active games
    Status,
    /// Restart active idle session
    Restart,
    /// List configured and active games
    Games,
    /// View recent logs from the daemon
    Logs {
        /// Number of lines to display
        #[arg(short = 'n', long, default_value_t = 25)]
        lines: usize,
    },
    /// Display version information
    Version,
    /// Daemon management utilities
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
}

#[derive(Subcommand, Debug)]
enum DaemonAction {
    /// Check daemon connectivity
    Status,
    /// Request daemon to stop gracefully
    Stop,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run_cli(cli).await {
        output::print_error(&e.to_string());
        std::process::exit(1);
    }
}

async fn run_cli(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Version => {
            println!("cs (Steam Idle Manager) v{}", env!("CARGO_PKG_VERSION"));
            println!("Protocol version: 1.0");
            Ok(())
        }
        Commands::Start { appids } => {
            let mut client = DaemonClient::connect(cli.socket.as_deref()).await?;
            let resp = client
                .call(&Request::Start {
                    appids: appids.clone(),
                })
                .await?;
            match resp {
                Response::Ok { .. } => {
                    // Fetch status to display game name cleanly
                    if let Ok(Response::Status(status)) = client.call(&Request::Status).await {
                        output::print_start_success(&appids, &status.active_games);
                    } else {
                        output::print_start_success(&appids, &[]);
                    }
                    Ok(())
                }
                Response::Error { message, .. } => {
                    anyhow::bail!("{}", message);
                }
                _ => anyhow::bail!("Unexpected response from daemon"),
            }
        }
        Commands::Stop { appids } => {
            let mut client = DaemonClient::connect(cli.socket.as_deref()).await?;
            let filter = if appids.is_empty() {
                None
            } else {
                Some(appids)
            };
            let resp = client.call(&Request::Stop { appids: filter }).await?;
            match resp {
                Response::Ok { .. } => {
                    output::print_stop_success();
                    Ok(())
                }
                Response::Error { message, .. } => {
                    anyhow::bail!("{}", message);
                }
                _ => anyhow::bail!("Unexpected response from daemon"),
            }
        }
        Commands::Status => {
            let mut client = DaemonClient::connect(cli.socket.as_deref()).await?;
            let resp = client.call(&Request::Status).await?;
            match resp {
                Response::Status(status) => {
                    output::print_status(&status);
                    Ok(())
                }
                Response::Error { message, .. } => {
                    anyhow::bail!("{}", message);
                }
                _ => anyhow::bail!("Unexpected response from daemon"),
            }
        }
        Commands::Restart => {
            let mut client = DaemonClient::connect(cli.socket.as_deref()).await?;
            let resp = client.call(&Request::Restart).await?;
            match resp {
                Response::Ok { message } => {
                    println!("{}", message);
                    Ok(())
                }
                Response::Error { message, .. } => {
                    anyhow::bail!("{}", message);
                }
                _ => anyhow::bail!("Unexpected response from daemon"),
            }
        }
        Commands::Games => {
            let mut client = DaemonClient::connect(cli.socket.as_deref()).await?;
            let resp = client.call(&Request::Games).await?;
            match resp {
                Response::Games(games) => {
                    output::print_games(&games);
                    Ok(())
                }
                Response::Error { message, .. } => {
                    anyhow::bail!("{}", message);
                }
                _ => anyhow::bail!("Unexpected response from daemon"),
            }
        }
        Commands::Logs { lines } => {
            let mut client = DaemonClient::connect(cli.socket.as_deref()).await?;
            let resp = client.call(&Request::Logs { lines }).await?;
            match resp {
                Response::Logs(logs) => {
                    output::print_logs(&logs.lines);
                    Ok(())
                }
                Response::Error { message, .. } => {
                    anyhow::bail!("{}", message);
                }
                _ => anyhow::bail!("Unexpected response from daemon"),
            }
        }
        Commands::Daemon { action } => match action {
            DaemonAction::Status => {
                let mut client = DaemonClient::connect(cli.socket.as_deref()).await?;
                let resp = client.call(&Request::Ping).await?;
                match resp {
                    Response::Pong => {
                        println!(
                            "steam-idled daemon is running and responding (socket: {}).",
                            client.socket_path().display()
                        );
                        Ok(())
                    }
                    _ => anyhow::bail!("Unexpected response from daemon"),
                }
            }
            DaemonAction::Stop => {
                let mut client = DaemonClient::connect(cli.socket.as_deref()).await?;
                let resp = client.call(&Request::Shutdown).await?;
                match resp {
                    Response::Ok { message } => {
                        println!("{}", message);
                        Ok(())
                    }
                    Response::Error { message, .. } => {
                        anyhow::bail!("{}", message);
                    }
                    _ => anyhow::bail!("Unexpected response from daemon"),
                }
            }
        },
    }
}
