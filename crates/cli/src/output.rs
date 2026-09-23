use colored::*;
use protocol::{ActiveGameInfo, DaemonState, GamesPayload, StatusPayload};

pub fn print_header() {
    println!("{}", "Steam Idle".bold());
    println!("{}", "────────────────────────".dimmed());
}

pub fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

pub fn print_start_success(appids: &[u32], active_games: &[ActiveGameInfo]) {
    print_header();
    println!("{} {}", "✓".green().bold(), "Manual stop cleared".white());
    println!("{} {}", "✓".green().bold(), "Connected to Steam".white());
    println!("{} {}", "✓".green().bold(), "Game presence enabled".white());

    if !active_games.is_empty() {
        for game in active_games {
            println!("{} {}", "✓".green().bold(), game.name.bold());
            println!("  AppID: {}", game.appid.to_string().cyan());
        }
    } else {
        for &id in appids {
            println!("  AppID: {}", id.to_string().cyan());
        }
    }
}

pub fn print_stop_success() {
    print_header();
    println!("{} {}", "✓".green().bold(), "Idle stopped".white());
    println!(
        "{} {}",
        "✓".green().bold(),
        "Automatic idle temporarily disabled".white()
    );
}

pub fn print_status(status: &StatusPayload) {
    print_header();

    let daemon_str = "RUNNING".green().bold();

    let steam_str = if status.steam_connected {
        "CONNECTED".green().bold()
    } else {
        "WAITING FOR STEAM (RETRYING)".yellow().bold()
    };

    let idle_str = match status.daemon_state {
        DaemonState::IdleRunning => "ACTIVE".green().bold(),
        DaemonState::IdleStarting => "STARTING".yellow().bold(),
        DaemonState::IdleStopped | DaemonState::Connected => {
            if status.manual_stop {
                "STOPPED (MANUAL OVERRIDE)".yellow().bold()
            } else {
                "STOPPED".yellow().bold()
            }
        }
        DaemonState::SuspendedForRealGame => "SUSPENDED (REAL GAME RUNNING)".cyan().bold(),
        DaemonState::SteamDisconnected => "STEAM DISCONNECTED (WAITING)".yellow().bold(),
        DaemonState::Reconnecting => "RECONNECTING".yellow().bold(),
        DaemonState::Connecting => "CONNECTING".yellow().bold(),
        DaemonState::Disconnected => "DISCONNECTED".red().bold(),
        DaemonState::Stopping => "STOPPING".red().bold(),
    };

    println!("{:<16} {}", "Daemon:", daemon_str);
    println!("{:<16} {}", "Steam:", steam_str);
    println!("{:<16} {}", "Idle:", idle_str);

    if status.idle_active && !status.active_games.is_empty() {
        if status.active_games.len() == 1 {
            let game = &status.active_games[0];
            println!("{:<16} {}", "Game:", game.name.bold());
            println!("{:<16} {}", "AppID:", game.appid.to_string().cyan());
            println!(
                "{:<16} {}",
                "Session:",
                format_duration(game.session_duration_secs).bold()
            );
        } else {
            println!("{:<16} {} active", "Games:", status.active_games.len());
            for game in &status.active_games {
                println!(
                    "  • {} (AppID: {}) - {}",
                    game.name.bold(),
                    game.appid.to_string().cyan(),
                    format_duration(game.session_duration_secs)
                );
            }
            println!(
                "{:<16} {}",
                "Total Session:",
                format_duration(status.total_session_secs).bold()
            );
        }
    }

    let real_proc_str = if status.real_cs2_running {
        "RUNNING (IDLE PAUSED)".yellow().bold()
    } else {
        "NOT RUNNING".dimmed()
    };
    println!("{:<16} {}", "Real process:", real_proc_str);

    if status.manual_stop {
        println!(
            "{:<16} {}",
            "Manual override:",
            "ACTIVE (run 'cs start' to resume)".yellow().bold()
        );
    }

    if status.auto_resume_enabled {
        println!("{:<16} {}", "Auto-resume:", "ENABLED".green());
    }
}

pub fn print_games(games: &GamesPayload) {
    print_header();
    println!("{}", "Configured Games:".bold());
    for game in &games.configured_games {
        let status = if game.is_active {
            "[ACTIVE]".green().bold()
        } else {
            "[IDLE]".dimmed()
        };
        println!("  • {:<8} {:<30} {}", game.appid, game.name, status);
    }
}

pub fn print_logs(lines: &[String]) {
    print_header();
    println!("{}", "Recent Daemon Logs:".bold());
    for line in lines {
        println!("{}", line);
    }
}

pub fn print_error(msg: &str) {
    eprintln!("{} {}", "Error:".red().bold(), msg);
}
