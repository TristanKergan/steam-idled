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

    // Wait briefly for socket to become available
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
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

    // Verify mock client received games
    assert_eq!(mock_clone.active_appids().await, vec![730]);

    // 4. Status while idling
    let resp = client.call(&Request::Status).await.unwrap();
    if let Response::Status(status) = resp {
        assert_eq!(status.daemon_state, DaemonState::IdleRunning);
        assert!(status.idle_active);
        assert_eq!(status.active_games.len(), 1);
        assert_eq!(status.active_games[0].appid, 730);
        assert_eq!(status.active_games[0].name, "Counter-Strike 2");
    } else {
        panic!("Expected Status response");
    }

    // 5. Query games list
    let resp = client.call(&Request::Games).await.unwrap();
    if let Response::Games(games) = resp {
        assert!(games.active_games.iter().any(|g| g.appid == 730));
    } else {
        panic!("Expected Games response");
    }

    // 6. Stop Idle
    let resp = client.call(&Request::Stop { appids: None }).await.unwrap();
    assert!(matches!(resp, Response::Ok { .. }));

    // Verify mock client games cleared
    assert!(mock_clone.active_appids().await.is_empty());

    // 7. Status after stop
    let resp = client.call(&Request::Status).await.unwrap();
    if let Response::Status(status) = resp {
        assert_eq!(status.daemon_state, DaemonState::IdleStopped);
        assert!(!status.idle_active);
        assert!(status.active_games.is_empty());
    } else {
        panic!("Expected Status response");
    }

    // Stop server
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_multi_game_start_and_partial_stop() {
    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("test-multi.sock");

    let cfg = Config {
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

#[tokio::test]
async fn test_steam_disconnect_and_reconnect_recovery() {
    let cfg = Config::default();
    let mock_steam = MockSteamClient::new();
    let mock_clone = mock_steam.clone();

    let mut engine = Engine::new(cfg, Box::new(mock_steam));
    engine.initialize_steam().await.unwrap();

    // Start idling CS2
    engine.start_idle(vec![730]).await.unwrap();
    assert_eq!(engine.state(), DaemonState::IdleRunning);
    assert_eq!(mock_clone.active_appids().await, vec![730]);

    // Simulate Steam dying / crashing
    mock_clone.set_steam_running(false);

    // Engine ticks and detects disconnect
    engine.tick().await.unwrap();
    assert_eq!(engine.state(), DaemonState::SteamDisconnected);
    assert!(mock_clone.active_appids().await.is_empty());

    // While Steam is still offline, tick keeps it in SteamDisconnected
    engine.tick().await.unwrap();
    assert_eq!(engine.state(), DaemonState::SteamDisconnected);

    // Now Steam comes back online
    mock_clone.set_steam_running(true);

    // Next tick reconnects and restores idle!
    engine.tick().await.unwrap();
    assert_eq!(engine.state(), DaemonState::IdleRunning);
    assert_eq!(mock_clone.active_appids().await, vec![730]);
}

#[tokio::test]
async fn test_duplicate_operations() {
    let cfg = Config::default();
    let mock_steam = MockSteamClient::new();
    let mut engine = Engine::new(cfg, Box::new(mock_steam));
    engine.initialize_steam().await.unwrap();

    // Duplicate stop when already stopped is a safe no-op
    assert!(engine.stop_idle(None).await.is_ok());
    assert_eq!(engine.state(), DaemonState::IdleStopped);

    // Start
    assert!(engine.start_idle(vec![730]).await.is_ok());
    assert_eq!(engine.state(), DaemonState::IdleRunning);

    // Duplicate start with same game is a safe no-op
    assert!(engine.start_idle(vec![730]).await.is_ok());
    assert_eq!(engine.state(), DaemonState::IdleRunning);

    // Stop
    assert!(engine.stop_idle(None).await.is_ok());
    assert_eq!(engine.state(), DaemonState::IdleStopped);

    // Another stop
    assert!(engine.stop_idle(None).await.is_ok());
    assert_eq!(engine.state(), DaemonState::IdleStopped);
}
