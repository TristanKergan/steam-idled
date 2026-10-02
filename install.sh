#!/usr/bin/env bash
set -euo pipefail

echo "=========================================="
echo " steam-idled & cs CLI - Installer"
echo "=========================================="

PREFIX="${HOME}/.local"
BIN_DIR="${PREFIX}/bin"
CONFIG_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/steam-idled"
SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"

echo "[1/5] Building release binaries..."
cargo build --release

echo "[2/5] Installing binaries to ${BIN_DIR}..."
mkdir -p "${BIN_DIR}"
install -m 755 target/release/steam-idled "${BIN_DIR}/steam-idled"
install -m 755 target/release/cs "${BIN_DIR}/cs"
if [ -f target/release/steam-idled-bot ]; then
    install -m 755 target/release/steam-idled-bot "${BIN_DIR}/steam-idled-bot"
fi
if [ -f target/release/libsteam_api.so ]; then
    install -m 755 target/release/libsteam_api.so "${BIN_DIR}/libsteam_api.so"
fi

echo "[3/5] Setting up configuration at ${CONFIG_DIR}..."
mkdir -p "${CONFIG_DIR}"
if [ ! -f "${CONFIG_DIR}/config.toml" ]; then
    cp config/config.example.toml "${CONFIG_DIR}/config.toml"
    echo "  -> Created default config: ${CONFIG_DIR}/config.toml"
else
    echo "  -> Existing config retained: ${CONFIG_DIR}/config.toml"
fi
if [ ! -f "${CONFIG_DIR}/bot.toml" ] && [ -f config/bot.example.toml ]; then
    cp config/bot.example.toml "${CONFIG_DIR}/bot.example.toml"
    echo "  -> Example bot config available at: ${CONFIG_DIR}/bot.example.toml"
fi

echo "[4/5] Installing systemd user services..."
mkdir -p "${SYSTEMD_USER_DIR}"
cp systemd/steam-idled.service "${SYSTEMD_USER_DIR}/steam-idled.service"
if [ -f systemd/steam-idled-bot.service ]; then
    cp systemd/steam-idled-bot.service "${SYSTEMD_USER_DIR}/steam-idled-bot.service"
fi

if command -v systemctl >/dev/null 2>&1; then
    echo "[5/5] Configuring systemd user service..."
    systemctl --user daemon-reload
    if [[ "${1:-}" != "--no-start" ]]; then
        systemctl --user enable --now steam-idled.service
        echo "  -> Service steam-idled enabled and started for current user session!"
    else
        echo "  -> Run 'systemctl --user enable --now steam-idled.service' when ready."
    fi
fi

echo ""
echo "Installation complete!"
echo ""
echo "Ensure ${BIN_DIR} is in your PATH. If needed, add to ~/.bashrc or ~/.zshrc:"
echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "Usage:"
echo "  cs status         # Check daemon and idle status"
echo "  cs start          # Start idling Counter-Strike 2 (clears manual stop)"
echo "  cs stop           # Stop idling (sets manual override)"
echo "  cs games          # List configured games"
echo "  cs logs           # View recent daemon logs"
echo ""
echo "Telegram Bot Setup:"
echo "  1. Copy ${CONFIG_DIR}/bot.example.toml to ${CONFIG_DIR}/bot.toml"
echo "  2. Fill in bot_token and admin_user_id"
echo "  3. Start bot service: systemctl --user enable --now steam-idled-bot.service"
echo "=========================================="
