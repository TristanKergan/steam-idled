# Steam Idle Daemon (`steam-idled`) & CLI (`cs`)

A modern, standalone Linux daemon and command-line interface written in Rust for managing Steam game presence and playtime tracking without downloading or launching game assets.

> **Note:**
> The daemon does not launch the game executable.
> It manages Steam game presence through the Steam client connection.

---

## Features

- **Headless Linux Daemon (`steam-idled`)**: Runs in the background as a user-level process with low resource usage (< 10 MB RAM).
- **Modern CLI (`cs`)**: Fast, human-readable terminal commands with colors and formatting.
- **Steam Presence Integration**: Uses native Steam client IPC to announce game presence without needing passwords, credentials, or session tokens.
- **Multiple Game Support**: Idle Counter-Strike 2 (730), Dota 2 (570), Team Fortress 2 (440), or any Steam AppID simultaneously.
- **Safe Real Game Detection (`auto_resume`)**: Uses non-invasive `/proc` scanning to detect when you launch the real CS2 game, temporarily clears idle presence so you can play without conflicts, and resumes idle after you exit.
- **Local Security**: Secure Unix Domain Socket communication (`0600` permissions), restricted strictly to your local user account. No TCP or remote ports are opened.
- **Systemd Integration**: Includes `systemd --user` service unit and non-root installer.

---

## Quick Start

### 1. Installation

```bash
git clone https://github.com/TryDkg/Farm.git steam-idled
cd steam-idled
./install.sh
```

To automatically enable and start the daemon with your session:

```bash
./install.sh --enable
```

### 2. Basic Usage

Start idling Counter-Strike 2 (default AppID: 730):

```bash
$ cs start

Steam Idle
────────────────────────
✓ Connected to Steam
✓ Game presence enabled
✓ Counter-Strike 2
  AppID: 730
```

Check current daemon and session status:

```bash
$ cs status

Steam Idle
────────────────────────
Daemon:       RUNNING
Steam:        CONNECTED
Idle:         ACTIVE
Game:         Counter-Strike 2
AppID:        730
Session:      01:27:42
Real process: NOT RUNNING
Auto-resume:  ENABLED
```

Stop idling:

```bash
$ cs stop

Steam Idle
────────────────────────
✓ Game presence stopped
✓ CS2 is ready to launch
```

Check status after stopping:

```bash
$ cs status

Steam Idle
────────────────────────
Daemon:       RUNNING
Steam:        CONNECTED
Idle:         STOPPED
Real process: NOT RUNNING
```

---

## CLI Commands

| Command | Example | Description |
|---|---|---|
| `cs start` | `cs start` | Start idling default game (Counter-Strike 2) |
| `cs start [APPID...]` | `cs start 730 570 440` | Idle one or multiple specific AppIDs |
| `cs stop` | `cs stop` | Stop all active idle presence |
| `cs stop [APPID...]` | `cs stop 570` | Stop idling a specific AppID |
| `cs status` | `cs status` | View daemon, Steam, and idle status |
| `cs restart` | `cs restart` | Restart the current idle session |
| `cs games` | `cs games` | List configured games and active state |
| `cs logs` | `cs logs -n 50` | Tail the most recent daemon log entries |
| `cs version` | `cs version` | Print version and protocol information |
| `cs daemon status` | `cs daemon status` | Ping daemon connectivity |
| `cs daemon stop` | `cs daemon stop` | Request clean daemon shutdown |

---

## How It Works

### The Game Presence Mechanism

Steam provides game presence and playtime accounting through two primary layers:

1. **Local Steam Client IPC**:
   The official Steam desktop client runs locally under your user session and maintains an authenticated connection to Valve's servers. Games notify the local Steam client of their presence via the Steamworks API (`libsteam_api.so` / `steamclient.so`). By initializing Steamworks with the target AppID, the running Steam client recognizes that the title is active.

2. **Network Protocol (`CMsgClientGamesPlayed`)**:
   Upon receiving local notification, the Steam desktop client packages active games into a Protocol Buffers network message `EMsg::ClientGamesPlayed` (`CMsgClientGamesPlayed`) and transmits it to Valve Connection Manager (CM) servers. The server updates your Steam community profile to "In-Game" and records playtime.

3. **Clearing Presence**:
   When `cs stop` is issued or the daemon shuts down, the worker process invokes `SteamAPI_Shutdown()` and closes the IPC channel. The Steam client immediately updates `CMsgClientGamesPlayed` without the AppID and notifies Valve servers, clearing your in-game status.

### Architecture

```text
                    ┌────────────────────┐
                    │       cs CLI       │
                    └─────────┬──────────┘
                              │
                         Unix Socket
              ($XDG_RUNTIME_DIR/steam-idled.sock)
                              │
                    ┌─────────▼──────────┐
                    │  steam-idled       │
                    │  Rust daemon       │
                    └─────────┬──────────┘
                              │
                       Steam client
                       IPC connection
                              │
                    ┌─────────▼──────────┐
                    │   Steam Client     │
                    └─────────┬──────────┘
                              │
                    CMsgClientGamesPlayed
                              │
                    ┌─────────▼──────────┐
                    │   Valve Servers    │
                    └────────────────────┘
```

Each game runs in an isolated worker process (`steam-idled worker --appid <id>`). This prevents SDK singleton collisions when idling multiple games simultaneously and protects the daemon from any worker crashes.

---

## Real Game Detection & Auto-Resume

When `auto_resume = true` is enabled in configuration:

1. `steam-idled` continuously monitors the Linux process table via `/proc` (reading `/proc/[pid]/comm` and `/proc/[pid]/exe`).
2. If the user launches the real Counter-Strike 2 executable (`cs2`), the daemon detects the real process and immediately suspends the idle presence so Steam does not flag an "Application already running" conflict.
3. Once the user closes CS2, the daemon waits for a configurable debounce delay (default: 10 seconds) and automatically restores the idle presence.

---

## Configuration

Configuration is located at `~/.config/steam-idled/config.toml`:

```toml
# Automatically start idling on daemon startup
enabled = false

# Default list of AppIDs to idle
games = [730]

# Automatically suspend idle when real game is detected
auto_resume = true

# Debounce delay (in seconds) after real game exits before resuming idle
resume_delay_seconds = 10
```

Runtime logs are saved to:
```text
~/.local/state/steam-idled/steam-idled.log
```

---

## Systemd User Service

Manage the daemon using standard systemd user commands:

```bash
# Start daemon
systemctl --user start steam-idled.service

# Stop daemon
systemctl --user stop steam-idled.service

# Enable on boot/login
systemctl --user enable steam-idled.service

# Check service status
systemctl --user status steam-idled.service
```

---

## Limitations

- The official Steam desktop client must be running and logged into an account.
- The user account must have a license to play the chosen AppID (Counter-Strike 2 is free to play).
- Playtime accumulation and card drop rates are governed by Valve's servers and Steam client behavior.

---

## Documentation

- [Architecture & Design](docs/architecture.md)
- [Steam Protocol Details](docs/steam-protocol.md)
- [Configuration Reference](docs/configuration.md)

---

## References

- [Valve Steamworks SDK Documentation](https://partner.steamgames.com/doc/api/steam_api)
- [SteamKit2 Protocol Specifications](https://github.com/SteamDatabase/Protobufs)
- [Rust Steamworks Bindings](https://crates.io/crates/steamworks)

---

## License

This project is licensed under the [MIT License](LICENSE).
