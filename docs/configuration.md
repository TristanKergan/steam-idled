# Configuration & Management

`steam-idled` configuration is stored in TOML format at:
```text
~/.config/steam-idled/config.toml
```

---

## 1. Configuration Options

```toml
# Start idling automatically when daemon begins running
enabled = false

# Default list of Steam AppIDs to idle when "cs start" is called without arguments
# 730 = Counter-Strike 2
# 570 = Dota 2
# 440 = Team Fortress 2
games = [730]

# Auto-resume feature:
# Automatically suspend idle presence when the real CS2 game is launched,
# and automatically restore idle presence when the real CS2 game exits.
auto_resume = true

# Debounce delay in seconds after real game closes before resuming idle
resume_delay_seconds = 10

# Optional custom Unix Domain Socket path
# Defaults to $XDG_RUNTIME_DIR/steam-idled.sock (or ~/.run/steam-idled.sock)
# socket_path = "/run/user/1000/steam-idled.sock"

# Optional custom log path
# Defaults to ~/.local/state/steam-idled/steam-idled.log
# log_path = "/home/user/.local/state/steam-idled/steam-idled.log"
```

---

## 2. CLI Commands (`cs`)

| Command | Description |
|---|---|
| `cs start [APPID...]` | Start idling default or specified AppIDs |
| `cs stop [APPID...]` | Stop idling all or specified AppIDs |
| `cs status` | Display current daemon and Steam status |
| `cs restart` | Restart current idle session |
| `cs games` | View configured and active games |
| `cs logs [-n lines]` | Display recent lines from daemon log |
| `cs version` | Show CLI and protocol version |
| `cs daemon status` | Ping daemon connectivity |
| `cs daemon stop` | Stop daemon service |

---

## 3. Systemd User Service

To enable and start the daemon with your user session:
```bash
systemctl --user enable --now steam-idled.service
```

To view service status:
```bash
systemctl --user status steam-idled.service
```

To view systemd journal logs:
```bash
journalctl --user -u steam-idled.service -f
```
