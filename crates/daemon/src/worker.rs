use anyhow::Result;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use steamworks::Client;
use tracing::{error, info};

pub fn run_worker(appid: u32) -> Result<()> {
    // Set standard Steam environment variables for the worker process
    std::env::set_var("SteamAppId", appid.to_string());
    std::env::set_var("SteamGameId", appid.to_string());

    let running = Arc::new(AtomicBool::new(true));

    info!("Initializing Steamworks for AppID {}", appid);

    let client = match Client::init_app(appid) {
        Ok(c) => c,
        Err(err) => {
            let msg = format!(
                "FAILED: SteamAPI_Init failed for AppID {}: {:?}",
                appid, err
            );
            error!("{}", msg);
            let mut stdout = io::stdout().lock();
            let _ = writeln!(stdout, "{}", msg);
            let _ = stdout.flush();
            std::process::exit(1);
        }
    };

    // Print handshake signal to stdout so parent daemon knows we are live
    {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "READY: {}", appid)?;
        stdout.flush()?;
    }

    info!(
        "Worker active for AppID {}. Running Steam callbacks...",
        appid
    );

    // Spawn thread to monitor stdin: if parent closes stdin, we terminate
    let r_stdin = running.clone();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut line = String::new();
        loop {
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    // Parent closed pipe or error
                    r_stdin.store(false, Ordering::SeqCst);
                    break;
                }
                Ok(_) => {
                    line.clear();
                }
            }
        }
    });

    // Main callback pump loop
    while running.load(Ordering::SeqCst) {
        client.run_callbacks();
        std::thread::sleep(Duration::from_millis(100));
    }

    info!("Worker for AppID {} shutting down cleanly", appid);
    Ok(())
}
