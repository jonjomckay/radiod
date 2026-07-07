#!/usr/bin/env bash
set -euo pipefail

REPO="jonjomckay/radiod"

BIN_DIR="${HOME}/.local/bin"
SERVICE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/radiod"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

install_binaries() {
    mkdir -p "$BIN_DIR"

    if systemctl --user is-active --quiet radiod 2>/dev/null; then
        echo "Stopping radiod before updating binary..."
        systemctl --user stop radiod
    fi

    local mode="${1:-latest}"

    if [ "$mode" = "--build" ]; then
        echo "Building from source..."
        cargo build --release
        cp target/release/radiod "$BIN_DIR/"
        cp target/release/radiod-ctl "$BIN_DIR/"
        return
    fi

    local version="$mode"
    local base_url

    if [ "$version" = "latest" ]; then
        base_url="https://github.com/${REPO}/releases/latest/download"
        version=$(curl -sS "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep -o '"tag_name": "[^"]*"' | cut -d'"' -f4 || echo "latest")
    else
        base_url="https://github.com/${REPO}/releases/download/${version}"
    fi

    echo "Downloading radiod (${version})..."
    if ! curl -fsSL "${base_url}/radiod" -o "${BIN_DIR}/radiod"; then
        echo -e "${RED}Failed to download radiod${NC}"
        exit 1
    fi
    chmod +x "${BIN_DIR}/radiod"

    echo "Downloading radiod-ctl..."
    if ! curl -fsSL "${base_url}/radiod-ctl" -o "${BIN_DIR}/radiod-ctl"; then
        echo -e "${RED}Failed to download radiod-ctl${NC}"
        exit 1
    fi
    chmod +x "${BIN_DIR}/radiod-ctl"
}

install_files() {
    mkdir -p "$SERVICE_DIR" "$DESKTOP_DIR" "$ICON_DIR"

    local raw="https://raw.githubusercontent.com/${REPO}/main/radiod.service"
    if [ -f "${SCRIPT_DIR}/radiod.service" ]; then
        cp "${SCRIPT_DIR}/radiod.service" "$SERVICE_DIR/"
    else
        curl -fsSL "${raw}" -o "${SERVICE_DIR}/radiod.service"
    fi
    echo "Installed ${SERVICE_DIR}/radiod.service"

    raw="https://raw.githubusercontent.com/${REPO}/main/radiod.desktop"
    if [ -f "${SCRIPT_DIR}/radiod.desktop" ]; then
        cp "${SCRIPT_DIR}/radiod.desktop" "$DESKTOP_DIR/"
    else
        curl -fsSL "${raw}" -o "${DESKTOP_DIR}/radiod.desktop"
    fi
    echo "Installed ${DESKTOP_DIR}/radiod.desktop"

    raw="https://raw.githubusercontent.com/${REPO}/main/assets/radiod.svg"
    if [ -f "${SCRIPT_DIR}/assets/radiod.svg" ]; then
        cp "${SCRIPT_DIR}/assets/radiod.svg" "$ICON_DIR/"
    else
        curl -fsSL "${raw}" -o "${ICON_DIR}/radiod.svg"
    fi
    echo "Installed ${ICON_DIR}/radiod.svg"

    if [ ! -f "$CONFIG_DIR/config.toml" ]; then
        mkdir -p "$CONFIG_DIR"
        raw="https://raw.githubusercontent.com/${REPO}/main/config.example.toml"
        if [ -f "${SCRIPT_DIR}/config.example.toml" ]; then
            cp "${SCRIPT_DIR}/config.example.toml" "$CONFIG_DIR/config.toml"
        else
            curl -fsSL "${raw}" -o "$CONFIG_DIR/config.toml"
        fi
        echo "Installed example config to ${CONFIG_DIR}/config.toml"
    else
        echo "Config already exists at ${CONFIG_DIR}/config.toml (skipped)"
    fi

    systemctl --user daemon-reload
}

check_dependencies() {
    if ldconfig -p 2>/dev/null | grep -q libmpv; then
        return 0
    fi

    echo ""
    echo -e "${RED}Warning: libmpv not found.${NC}"
    echo "libmpv is required for audio playback. Install the missing packages:"
    echo ""

    if command -v pacman &>/dev/null; then
        echo "  sudo pacman -S mpv"
    elif command -v apt &>/dev/null; then
        echo "  sudo apt install libmpv-dev mpv"
    elif command -v dnf &>/dev/null; then
        echo "  sudo dnf install mpv-libs mpv"
    elif command -v zypper &>/dev/null; then
        echo "  sudo zypper install mpv"
    else
        echo "  Required: mpv (provides libmpv.so)"
    fi
}

install_binaries "$@"
install_files
check_dependencies

echo ""
echo -e "${GREEN}Installation complete.${NC}"
echo ""
echo "Start and enable the service:"
echo "  systemctl --user enable --now radiod"
echo ""
echo "Control the daemon:"
echo "  radiod-ctl play"
echo "  radiod-ctl pause"
echo "  radiod-ctl now-playing"
echo ""
echo "View logs:"
echo "  journalctl --user -u radiod -f"
