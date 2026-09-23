use daemon::{Engine, UnixServer};
use protocol::{Config, DaemonState, IpcClient, Request, Response};
use std::sync::Arc;
use std::time::Duration;
use steam::MockSteamClient;
use steam::SteamClient;
use tempfile::tempdir;
use tokio::sync::{broadcast, Mutex};

#[tokio::test]
async fn test_ipc_lifecycle_with_mock_steam() {
    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("test-steam-idled.sock");

    let cfg = Config {
        enabled: false, // Start manually for this test
        socket_path: Some(sock_path.clone()),
        games: vec![730],
        ..Default::default()
    };

    let mock_steam = MockSteamClient::new();
    let mock_clone = mock_steam.clone();

    let engine = Arc::new(Mutex::new(Engine::new(cfg, Box::new(mock_steam))));

    // Initialize Steam
    {
        let mut eng = engine.lock().await;
        eng.initialize_steam().await.unwrap();
        assert_eq!(eng.state(), DaemonState::IdleStopped);
    }

    let (shutdown_tx, _) = broadcast::channel(1);
    let server = UnixServer::new(sock_path.clone(), engine.clone(), shutdown_tx.clone());
    let server_shutdown = shutdown_tx.subscribe();
    let server_handle = tokio::spawn(async move {
        let _ = server.run(server_shutdown).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&sock_path).await.unwrap();

    // 1. Ping
    let resp = client.call(&Request::Ping).await.unwrap();
    assert!(matches!(resp, Response::Pong));

    // 2. Initial Status
    let resp = client.call(&Request::Status).await.unwrap();
    if let Response::Status(status) = resp {
        assert_eq!(status.daemon_state, DaemonState::IdleStopped);
        assert!(status.steam_connected);
        assert!(!status.idle_active);
        assert!(!status.manual_stop);
        assert!(status.active_games.is_empty());
    } else {
        panic!("Expected Status response");
    }

    // 3. Start Idle
    let resp = client
        .call(&Request::Start { appids: vec![730] })
        .await
        .unwrap();
    assert!(matches!(resp, Response::Ok { .. }));
    assert_eq!(mock_clone.active_appids().await, vec![730]);

    // 4. Status while idling
    let resp = client.call(&Request::Status).await.unwrap();
    if let Response::Status(status) = resp {
        assert_eq!(status.daemon_state, DaemonState::IdleRunning);
        assert!(status.idle_active);
        assert!(!status.manual_stop);
        assert_eq!(status.active_games.len(), 1);
        assert_eq!(status.active_games[0].appid, 730);
    } else {
        panic!("Expected Status response");
    }

    // 5. Stop Idle
    let resp = client.call(&Request::Stop { appids: None }).await.unwrap();
    assert!(matches!(resp, Response::Ok { .. }));
    assert!(mock_clone.active_appids().await.is_empty());

    // 6. Status after stop shows manual_stop = true
    let resp = client.call(&Request::Status).await.unwrap();
    if let Response::Status(status) = resp {
        assert_eq!(status.daemon_state, DaemonState::IdleStopped);
        assert!(!status.idle_active);
        assert!(status.manual_stop);
    } else {
        panic!("Expected Status response");
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_steam_unavailable_at_startup_and_retry() {
    let mock_steam = MockSteamClient::new();
    mock_steam.set_steam_running(false); // Steam not running yet

    let cfg = Config {
        enabled: true,
        steam_retry_seconds: 1, // Fast 1-second retry for testing
        games: vec![730],
        ..Default::default()
    };

    let mut engine = Engine::new(cfg, Box::new(mock_steam.clone()));

    // Daemon starts, Steam is not running
    engine.initialize_steam().await.unwrap();
    assert_eq!(engine.state(), DaemonState::SteamDisconnected);
    assert!(!engine.is_manual_stop());

    // Tick immediately: 1-second retry interval not elapsed yet
    engine.tick().await.unwrap();
    assert_eq!(engine.state(), DaemonState::SteamDisconnected);

    // Wait for retry interval
    tokio::time::sleep(Duration::from_millis(1050)).await;

    // Steam is still not running: tick should check and remain in SteamDisconnected
    engine.tick().await.unwrap();
    assert_eq!(engine.state(), DaemonState::SteamDisconnected);

    // Now user launches Steam!
    mock_steam.set_steam_running(true);

    // Wait for next retry interval
    tokio::time::sleep(Duration::from_millis(1050)).await;

    // Tick: detects Steam, connects, and automatically starts idle!
    engine.tick().await.unwrap();
    assert_eq!(engine.state(), DaemonState::IdleRunning);
    assert_eq!(mock_steam.active_appids().await, vec![730]);
}

#[tokio::test]
async fn test_manual_override_prevents_auto_resume_on_reconnect() {
    let mock_steam = MockSteamClient::new();
    let cfg = Config {
        enabled: true,
        steam_retry_seconds: 1,
        games: vec![730],
        ..Default::default()
    };

    let mut engine = Engine::new(cfg, Box::new(mock_steam.clone()));
    engine.initialize_steam().await.unwrap();
    assert_eq!(engine.state(), DaemonState::IdleRunning);

    // User explicitly issues `cs stop`
    engine.stop_idle(None).await.unwrap();
    assert_eq!(engine.state(), DaemonState::IdleStopped);
    assert!(engine.is_manual_stop());
    assert!(mock_steam.active_appids().await.is_empty());

    // Steam disconnects
    mock_steam.set_steam_running(false);
    engine.tick().await.unwrap();
    assert_eq!(engine.state(), DaemonState::SteamDisconnected);

    // Steam reconnects
    mock_steam.set_steam_running(true);
    tokio::time::sleep(Duration::from_millis(1050)).await;
    engine.tick().await.unwrap();

    // CRITICAL: Idle MUST remain STOPPED because manual override is active!
    assert_eq!(engine.state(), DaemonState::IdleStopped);
    assert!(engine.is_manual_stop());
    assert!(mock_steam.active_appids().await.is_empty());

    // User explicitly issues `cs start` -> clears manual override
    engine.start_idle(vec![730]).await.unwrap();
    assert_eq!(engine.state(), DaemonState::IdleRunning);
    assert!(!engine.is_manual_stop());
    assert_eq!(mock_steam.active_appids().await, vec![730]);
}

#[tokio::test]
async fn test_auto_resume_vs_manual_stop() {
    let mock_steam = MockSteamClient::new();
    let cfg = Config {
        enabled: true,
        auto_resume: true,
        resume_delay_seconds: 1,
        games: vec![730],
        ..Default::default()
    };

    let mut engine = Engine::new(cfg, Box::new(mock_steam.clone()));
    engine.initialize_steam().await.unwrap();
    assert_eq!(engine.state(), DaemonState::IdleRunning);

    // If manual stop is invoked, auto_resume is inhibited
    engine.stop_idle(None).await.unwrap();
    assert_eq!(engine.state(), DaemonState::IdleStopped);
    assert!(engine.is_manual_stop());

    // Even if ticks occur, manual stop stays in effect
    engine.tick().await.unwrap();
    assert_eq!(engine.state(), DaemonState::IdleStopped);
}

#[tokio::test]
async fn test_multi_game_start_and_partial_stop() {
    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("test-multi.sock");

    let cfg = Config {
        enabled: false,
        socket_path: Some(sock_path.clone()),
        ..Default::default()
    };

    let mock_steam = MockSteamClient::new();
    let mock_clone = mock_steam.clone();

    let engine = Arc::new(Mutex::new(Engine::new(cfg, Box::new(mock_steam))));

    {
        let mut eng = engine.lock().await;
        eng.initialize_steam().await.unwrap();
    }

    let (shutdown_tx, _) = broadcast::channel(1);
    let server = UnixServer::new(sock_path.clone(), engine.clone(), shutdown_tx.clone());
    let server_shutdown = shutdown_tx.subscribe();
    let server_handle = tokio::spawn(async move {
        let _ = server.run(server_shutdown).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&sock_path).await.unwrap();

    // Start 3 games: 730 (CS2), 570 (Dota 2), 440 (TF2)
    let resp = client
        .call(&Request::Start {
            appids: vec![730, 570, 440],
        })
        .await
        .unwrap();
    assert!(matches!(resp, Response::Ok { .. }));
    assert_eq!(mock_clone.active_appids().await, vec![730, 570, 440]);

    // Check status
    if let Response::Status(status) = client.call(&Request::Status).await.unwrap() {
        assert_eq!(status.active_games.len(), 3);
    } else {
        panic!("Expected Status response");
    }

    // Stop one game: 570
    let resp = client
        .call(&Request::Stop {
            appids: Some(vec![570]),
        })
        .await
        .unwrap();
    assert!(matches!(resp, Response::Ok { .. }));

    // Remaining should be 730 and 440
    let remaining = mock_clone.active_appids().await;
    assert_eq!(remaining, vec![730, 440]);

    // Stop all remaining
    let resp = client.call(&Request::Stop { appids: None }).await.unwrap();
    assert!(matches!(resp, Response::Ok { .. }));
    assert!(mock_clone.active_appids().await.is_empty());

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}
