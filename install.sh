#!/usr/bin/env bash
set -euo pipefail

REPO="jonjomckay/radio-devil"

BIN_DIR="${HOME}/.local/bin"
SERVICE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/radio-devil"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

install_binaries() {
    mkdir -p "$BIN_DIR"

    local mode="${1:-latest}"

    if [ "$mode" = "--build" ]; then
        echo "Building from source..."
        cargo build --release
        cp target/release/radio-devild "$BIN_DIR/"
        cp target/release/radio-devil-ctl "$BIN_DIR/"
        return
    fi

    local version="$mode"
    local base_url

    if [ "$version" = "latest" ]; then
        base_url="https://github.com/${REPO}/releases/latest/download"
    else
        base_url="https://github.com/${REPO}/releases/download/${version}"
    fi

    echo "Downloading radio-devild..."
    if ! curl -fsSL "${base_url}/radio-devild" -o "${BIN_DIR}/radio-devild"; then
        echo -e "${RED}Failed to download radio-devild${NC}"
        exit 1
    fi
    chmod +x "${BIN_DIR}/radio-devild"

    echo "Downloading radio-devil-ctl..."
    if ! curl -fsSL "${base_url}/radio-devil-ctl" -o "${BIN_DIR}/radio-devil-ctl"; then
        echo -e "${RED}Failed to download radio-devil-ctl${NC}"
        exit 1
    fi
    chmod +x "${BIN_DIR}/radio-devil-ctl"
}

install_files() {
    mkdir -p "$SERVICE_DIR" "$DESKTOP_DIR" "$ICON_DIR"

    cp "${SCRIPT_DIR}/radio-devil.service" "$SERVICE_DIR/"
    echo "Installed ${SERVICE_DIR}/radio-devil.service"

    cp "${SCRIPT_DIR}/radio-devil.desktop" "$DESKTOP_DIR/"
    echo "Installed ${DESKTOP_DIR}/radio-devil.desktop"

    cp "${SCRIPT_DIR}/assets/radio-devil.svg" "$ICON_DIR/"
    echo "Installed ${ICON_DIR}/radio-devil.svg"

    if [ ! -f "$CONFIG_DIR/config.toml" ]; then
        mkdir -p "$CONFIG_DIR"
        cp "${SCRIPT_DIR}/config.example.toml" "$CONFIG_DIR/config.toml"
        echo "Installed example config to ${CONFIG_DIR}/config.toml"
    else
        echo "Config already exists at ${CONFIG_DIR}/config.toml (skipped)"
    fi

    systemctl --user daemon-reload
}

install_binaries "$@"
install_files

echo ""
echo -e "${GREEN}Installation complete.${NC}"
echo ""
echo "Start and enable the service:"
echo "  systemctl --user enable --now radio-devil"
echo ""
echo "Control the daemon:"
echo "  radio-devil-ctl play"
echo "  radio-devil-ctl pause"
echo "  radio-devil-ctl now-playing"
echo ""
echo "View logs:"
echo "  journalctl --user -u radio-devil -f"
