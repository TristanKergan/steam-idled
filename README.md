# Steam Idle Daemon (`steam-idled`) & CLI (`cs`)

A modern, standalone Linux daemon and command-line interface written in Rust for managing Steam game presence and playtime tracking without downloading or launching game assets.

> **Note:**
> The daemon does not launch the game executable.
> It manages Steam game presence through the Steam client connection.

---

## Features

- **Automatic Session Idle**: Starts automatically on Linux user login via `systemd --user`. Idle for Counter-Strike 2 begins immediately without needing manual intervention.
- **15-Second Steam Retry Loop**: If Steam is not running at system startup or exits, the daemon waits patiently and checks every 15 seconds without busy-polling or high CPU usage.
- **Explicit Manual Override**:
  - `cs stop` immediately stops idling and temporarily disables automatic idle, giving you full control to launch the real CS2 game.
  - `cs start` clears the manual override and re-enables idling.
- **Safe Real Game Detection (`auto_resume`)**: Non-invasive `/proc` scanner temporarily suspends idle when the real `cs2` binary starts, and resumes it after exit with a debounce delay.
- **Multi-Game Support**: Concurrently idle Counter-Strike 2 (730), Dota 2 (570), Team Fortress 2 (440), or any Steam AppID.
- **Secure Local IPC**: Strict Unix Domain Socket communication (`0600` permissions), restricted strictly to your local user account. No TCP or remote ports.

---

## Quick Start

### 1. Installation & Service Activation

```bash
git clone https://github.com/TryDkg/Farm.git steam-idled
cd steam-idled
./install.sh
```

The installer builds release binaries, installs them to `~/.local/bin/`, sets up default configuration, reloads systemd, and enables the user service.

Check service status anytime:

```bash
$ systemctl --user status steam-idled.service
● steam-idled.service - Steam Idle Daemon
     Loaded: loaded (/home/user/.config/systemd/user/steam-idled.service; enabled)
     Active: active (running)
```

### 2. Basic Usage

Check current status:

```bash
$ cs status

Steam Idle
────────────────────────
Daemon:          RUNNING
Steam:           CONNECTED
Idle:            ACTIVE
Game:            Counter-Strike 2
AppID:           730
Session:         01:27:42
Real process:    NOT RUNNING
Auto-resume:     ENABLED
```

Temporarily stop idling (e.g. before playing real CS2):

```bash
$ cs stop

Steam Idle
────────────────────────
✓ Idle stopped
✓ Automatic idle temporarily disabled
```

Status with manual override active:

```bash
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

```bash
$ cs start

Steam Idle
────────────────────────
✓ Manual stop cleared
✓ Connected to Steam
✓ Game presence enabled
✓ Counter-Strike 2
  AppID: 730
```

---

## Automatic Startup & Steam Retry

When your Linux user session starts:

```text
PC boot / Login
      ↓
user session started
      ↓
steam-idled launches via systemd --user
      ↓
check Steam
      │
      ├── Steam is running ──► connect ──► start idle (AppID 730)
      │
      └── Steam NOT running
              ↓
          WAIT 15 seconds
              ↓
          check Steam again
              ↓
          (repeats every 15s until Steam appears)
              ↓
          Steam detected ──► connect ──► start idle
```

Log output during retry:

```text
INFO daemon started
INFO checking Steam connection
WARN Steam is not running
INFO retrying Steam connection in 15 seconds
INFO checking Steam connection
INFO Steam detected
INFO connected to Steam
INFO starting idle for appid=[730]
INFO idle active
```

---

## Manual Override (`cs stop` vs `auto_resume`)

| Trigger | What Happens | Will Idle Auto-Resume? |
|---|---|---|
| `cs stop` | Manual stop activated; game presence cleared | **NO**. Remains stopped until you run `cs start`. |
| `cs start` | Manual stop cleared; game presence enabled | **YES**. Active and running. |
| Real CS2 launched | Idle suspended (`auto_resume`) | **YES**. Automatically resumes after real CS2 closes + debounce delay. |

---

## Configuration

Configuration is located at `~/.config/steam-idled/config.toml`:

```toml
# Automatically start idling on daemon / user session launch
enabled = true

# Default list of Steam AppIDs to idle
games = [730]

# Auto-resume when real game process launches and exits
auto_resume = true

# Debounce delay (in seconds) after real game exits before resuming idle
resume_delay_seconds = 10

# Seconds to wait between retries when Steam is not running
steam_retry_seconds = 15
```

---

## CLI Reference (`cs`)

| Command | Example | Description |
|---|---|---|
| `cs status` | `cs status` | Display daemon, Steam, and idle status |
| `cs start` | `cs start` | Clear manual stop and start idling default game |
| `cs start [APPID...]` | `cs start 730 570` | Idle specific AppIDs |
| `cs stop` | `cs stop` | Stop all active idle games and set manual override |
| `cs stop [APPID...]` | `cs stop 570` | Stop idling a specific AppID |
| `cs restart` | `cs restart` | Restart current idle session |
| `cs games` | `cs games` | View configured and active games |
| `cs logs` | `cs logs -n 50` | View recent daemon logs |
| `cs version` | `cs version` | Print version information |
| `cs daemon status` | `cs daemon status` | Ping daemon process |
| `cs daemon stop` | `cs daemon stop` | Request clean daemon termination |

---

## Systemd User Service

```bash
# Enable on user login
systemctl --user enable steam-idled.service

# Start or restart
systemctl --user restart steam-idled.service

# Stop
systemctl --user stop steam-idled.service

# Check service status
systemctl --user status steam-idled.service
```

---

## License

MIT License.
