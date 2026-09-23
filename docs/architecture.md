# Architecture & Technical Design

`steam-idled` is an independent, headless Linux daemon and CLI for managing Steam game presence and playtime without launching actual game binaries.

---

## 1. System Overview

```text
               ┌───────────────────────┐
               │        cs CLI         │
               └──────────┬────────────┘
                          │
                     Unix Socket
             ($XDG_RUNTIME_DIR/steam-idled.sock)
                          │
               ┌──────────▼────────────┐
               │      steam-idled      │
               │     Daemon Engine     │
               └─────┬──────────────┬──┘
                     │              │
        IPC Worker Process      Process Monitor
     (Worker: AppID 730, 570)     (/proc scan)
                     │              │
               ┌─────▼──────┐       │
               │   Steam    │◄──────┘
               │  Desktop   │ (Auto-resume debounce)
               │   Client   │
               └─────┬──────┘
                     │
          CMsgClientGamesPlayed
                     │
               ┌─────▼──────┐
               │ Valve CM   │
               │  Servers   │
               └────────────┘
```

---

## 2. Core Components

### 2.1 Protocol Layer (`crates/protocol`)
- **IPC Protocol**: Newline-delimited JSON over Unix Domain Sockets. Request/Response framing supporting `Start`, `Stop`, `Status`, `Restart`, `Games`, `Logs`, `Ping`, `Shutdown`.
- **State Machine (`DaemonState`)**: Explicit finite state transitions.
- **Config**: TOML configuration loading and schema with standard XDG path resolution.
- **Game Database**: AppID to name mapper with pre-populated titles (CS2, Dota 2, TF2, Rust, etc.).

### 2.2 Steam Abstraction Layer (`crates/steam`)
- **`SteamClient` Trait**: Decouples the daemon state machine from protocol details.
- **`RealSteamClient`**: Manages isolated worker subprocesses (`steam-idled worker --appid <id>`) for active games.
- **`MockSteamClient`**: Deterministic in-memory simulation for automated test suites.

### 2.3 Daemon Engine & Supervisor (`crates/daemon`)
- **Engine**: Coordinates connection state, active session timers, auto-resume debounce, and Steam connection monitoring.
- **Worker Process Isolation**: Each game runs in a dedicated child process. This guarantees:
  1. Concurrency: Multiple AppIDs run without singleton conflicts.
  2. Crash Isolation: A Steamworks panic or segfault in a worker never crashes the daemon.
  3. Clean Cleanup: Terminating the child process immediately drops the Steamworks IPC handle, instantly clearing presence in Steam.
- **Process Detector**: Non-invasive, safe inspection of `/proc` to detect when the real CS2 game executable starts or stops. Debounces exit events (default 10s) before restoring idle presence.

### 2.4 Command Line Interface (`crates/cli` - `cs`)
- Lightweight, fast CLI binary that sends JSON commands to the daemon's Unix socket and formats human-readable status, colors, and timestamps.
