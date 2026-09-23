# steam-idled

A small Linux daemon and CLI for managing Steam game presence.

The project runs a background daemon and exposes a simple `cs` command for
starting, stopping, and inspecting an idle session without launching the actual
game executable.

## What it does

- Reports chosen Steam AppIDs (default: Counter-Strike 2, AppID `730`) as running to your local Steam client.
- Accumulates playtime on your Steam account without downloading or running game binaries.
- Connects through the local Steam client IPC; does not require Steam credentials, passwords, or tokens.
- Supports idling multiple AppIDs simultaneously using isolated worker processes.
- Detects when the real `cs2` process starts via `/proc`, pauses idle presence to avoid conflicts, and resumes it after exit (`auto_resume`).
- Provides an explicit manual override: `cs stop` keeps idle stopped until you run `cs start`.

## How it works

```text
cs CLI
   ↓  (Unix domain socket: $XDG_RUNTIME_DIR/steam-idled.sock)
steam-idled daemon
   ↓  (Subprocess per active game: steam-idled worker --appid <id>)
Steamworks API (libsteam_api.so / steamclient.so)
   ↓  (Local IPC: ~/.steam/steam.pipe)
Steam desktop client
   ↓  (Protobuf: CMsgClientGamesPlayed)
Valve Steam servers
```

1. **CLI communication**: `cs` commands send JSON requests to `steam-idled` over a local Unix domain socket with `0600` permissions.
2. **Worker isolation**: When an AppID is idled, the daemon spawns an internal worker subprocess (`steam-idled worker --appid <id>`). The worker calls `SteamAPI_Init` and pumps callbacks (`SteamAPI_RunCallbacks`) every 100 ms. Isolating workers prevents SDK singleton conflicts across multiple AppIDs and ensures worker crashes do not terminate the daemon.
3. **Presence broadcast**: The local Steam desktop client detects the active game session over IPC and transmits `CMsgClientGamesPlayed` to Valve servers.
4. **Presence removal**: Calling `cs stop` or stopping the daemon terminates the worker subprocess, which closes the Steamworks IPC handle and clears in-game status immediately.
5. **Steam retry loop**: If Steam is not running when the daemon starts or if Steam closes, the daemon transitions to `SteamDisconnected` and retries connection every 15 seconds (`steam_retry_seconds`) using an async timer.

## Requirements

- Linux (x86_64)
- Steam desktop client installed and logged in
- Rust toolchain (1.80+ for building from source)

## Installation

Clone the repository and run the installer:

```bash
git clone git@github.com:TristanKergan/steam-idled.git
cd steam-idled
./install.sh
```

The installer builds release binaries and places them in:
- `~/.local/bin/steam-idled`
- `~/.local/bin/cs`
- `~/.local/bin/libsteam_api.so`

It also creates `~/.config/steam-idled/config.toml` (if not already present) and installs the user systemd service.

Ensure `~/.local/bin` is in your `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

## Configuration

Configuration is stored in `~/.config/steam-idled/config.toml`:

```toml
# Automatically start idling configured games when daemon starts
enabled = true

# Default AppIDs to idle
games = [730]

# Pause idle when real game executable is detected in /proc
auto_resume = true

# Seconds to wait after real game process exits before resuming idle
resume_delay_seconds = 10

# Seconds to wait between retries when Steam is not running
steam_retry_seconds = 15
```

## Usage

```bash
cs status        # Show daemon, Steam, and idle status
cs start         # Start idling default game (AppID 730)
cs start 730 570 # Start idling specific games
cs stop          # Stop idling and set manual override
cs stop 570      # Stop idling a specific AppID
cs restart       # Restart active idle session
cs games         # List configured games and active state
cs logs          # View recent daemon logs
cs version       # Show version info
cs daemon status # Ping daemon process
cs daemon stop   # Stop the daemon
```

### Examples

Check status:

```text
$ cs status
Steam Idle
────────────────────────
Daemon:          RUNNING
Steam:           CONNECTED
Idle:            ACTIVE
Game:            Counter-Strike 2
AppID:           730
Session:         00:42:15
Real process:    NOT RUNNING
Auto-resume:     ENABLED
```

Stop idling:

```text
$ cs stop
Steam Idle
────────────────────────
✓ Idle stopped
✓ Automatic idle temporarily disabled
```

Status after stop:

```text
$ cs status
Steam Idle
────────────────────────
Daemon:          RUNNING
Steam:           CONNECTED
Idle:            STOPPED (MANUAL OVERRIDE)
Real process:    NOT RUNNING
Manual override: ACTIVE (run 'cs start' to resume)
Auto-resume:     ENABLED
```

Resume idling:

```text
$ cs start
Steam Idle
────────────────────────
✓ Manual stop cleared
✓ Connected to Steam
✓ Game presence enabled
✓ Counter-Strike 2
  AppID: 730
```

## systemd

The daemon runs as a user systemd service (`steam-idled.service`):

```bash
systemctl --user enable steam-idled.service
systemctl --user start steam-idled.service
systemctl --user status steam-idled.service
systemctl --user restart steam-idled.service
systemctl --user stop steam-idled.service
```

Unit file location: `~/.config/systemd/user/steam-idled.service`.

## Auto-start

When `enabled = true` in `config.toml`, the daemon automatically connects to Steam and starts idling the configured games as soon as it launches. If Steam is not yet open when you log in, `steam-idled` waits and retries every 15 seconds until Steam appears.

If you prefer to trigger `cs start` with a delay after login via a standalone oneshot systemd unit, an example is provided in `systemd/cs-autostart.service.example`:

```bash
cp systemd/cs-autostart.service.example ~/.config/systemd/user/cs-autostart.service
systemctl --user daemon-reload
systemctl --user enable cs-autostart.service
```

## Project structure

```text
├── Cargo.toml
├── install.sh
├── systemd/
│   ├── steam-idled.service
│   └── cs-autostart.service.example
├── config/
│   └── config.example.toml
├── docs/
│   ├── architecture.md
│   ├── configuration.md
│   └── steam-protocol.md
└── crates/
    ├── protocol/      # IPC protocol, state machine, config, and game metadata
    ├── steam/         # SteamClient abstraction, RealSteamClient, and MockSteamClient
    ├── daemon/        # steam-idled daemon binary, process detector, worker runner
    └── cli/           # cs command-line tool
```

## Testing

```bash
cargo test --all
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Currently, 17 automated tests cover:
- TOML configuration parsing and validation fallbacks
- Explicit state machine transitions and invalid transition rejections
- Game database metadata resolution
- IPC framing and serialization over Unix sockets
- Mock Steam client lifecycle and disconnect simulation
- Process detection and debounce logic for real game binaries
- Auto-resume after real game exits
- Startup with Steam unavailable and 15-second retry timer
- Manual override (`cs stop` prevents auto-restart; `cs start` clears override)
- Multi-game concurrent idle and partial stops

## Limitations

- Requires the official Steam desktop client to be running and logged in.
- Linux only.
- The daemon does not launch or emulate the game executable; it reports presence through the Steam client connection.
- Games must be owned or free-to-play on the logged-in account (e.g., Counter-Strike 2 is free).
- Playtime accounting is governed by Valve's servers and the Steam client.

## License

[MIT](LICENSE)
