#!/usr/bin/env bash
set -euo pipefail

echo "=========================================="
echo " steam-idled & cs CLI - Installer"
echo "=========================================="

PREFIX="${HOME}/.local"
BIN_DIR="${PREFIX}/bin"
LIB_DIR="${PREFIX}/lib"
CONFIG_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/steam-idled"
SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"

echo "[1/5] Building release binaries..."
cargo build --release

echo "[2/5] Installing binaries to ${BIN_DIR}..."
mkdir -p "${BIN_DIR}"
install -m 755 target/release/steam-idled "${BIN_DIR}/steam-idled"
install -m 755 target/release/cs "${BIN_DIR}/cs"
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

echo "[4/5] Installing systemd user service..."
mkdir -p "${SYSTEMD_USER_DIR}"
cp systemd/steam-idled.service "${SYSTEMD_USER_DIR}/steam-idled.service"

if command -v systemctl >/dev/null 2>&1; then
    echo "[5/5] Reloading systemd user daemon..."
    systemctl --user daemon-reload

    if [[ "${1:-}" == "--enable" || "${1:-}" == "-e" ]]; then
        echo "Enabling and starting steam-idled service..."
        systemctl --user enable --now steam-idled.service
    else
        echo ""
        echo "To enable and start the daemon as a background service, run:"
        echo "  systemctl --user enable --now steam-idled.service"
    fi
fi

echo ""
echo "Installation complete!"
echo ""
echo "Ensure ${BIN_DIR} is in your PATH. If needed, add to ~/.bashrc or ~/.zshrc:"
echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "Usage:"
echo "  cs start      # Start idling Counter-Strike 2 (AppID 730)"
echo "  cs status     # Check daemon and idle status"
echo "  cs stop       # Stop idling"
echo "  cs games      # List configured games"
echo "  cs logs       # View recent daemon logs"
echo "=========================================="
