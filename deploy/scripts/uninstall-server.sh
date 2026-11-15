#!/usr/bin/env bash
# Uninstall the cce-server service (keeps data and config by default).
set -euo pipefail

SCOPE="user"
PURGE=0
for arg in "$@"; do
    case "$arg" in
        --system) SCOPE="system" ;;
        --purge) PURGE=1 ;;
        -h|--help) echo "Usage: $0 [--system] [--purge]"; exit 0 ;;
        *) echo "Unknown argument: $arg" >&2; exit 2 ;;
    esac
done

if [[ "$SCOPE" == "system" ]]; then
    UNIT="/etc/systemd/system/cce-server.service"
    BIN_DIR="/usr/local/bin"
    CONFIG_DIR="/etc/cce"
    DATA_DIR="/var/lib/cce"
    SUDO="sudo"
    [[ "$(id -u)" -eq 0 ]] && SUDO=""
else
    UNIT="$HOME/.config/systemd/user/cce-server.service"
    BIN_DIR="$HOME/.local/bin"
    CONFIG_DIR="$HOME/.config/cce"
    DATA_DIR="$HOME/.local/share/cce"
    SUDO=""
fi

run_root() { if [[ -n "$SUDO" ]]; then sudo "$@"; else "$@"; fi; }

if [[ -f "$UNIT" ]]; then
    if [[ -n "$SUDO" ]]; then sudo systemctl stop cce-server.service 2>/dev/null || true; sudo systemctl disable cce-server.service 2>/dev/null || true
    else systemctl --user stop cce-server.service 2>/dev/null || true; systemctl --user disable cce-server.service 2>/dev/null || true; fi
    run_root rm -f "$UNIT"
    if [[ -n "$SUDO" ]]; then sudo systemctl daemon-reload; else systemctl --user daemon-reload; fi
    echo "Removed unit: $UNIT"
fi

for b in cce cce-cli cce-mcp; do
    if [[ -f "$BIN_DIR/$b" ]]; then
        run_root rm -f "$BIN_DIR/$b"
        echo "Removed binary: $BIN_DIR/$b"
    fi
done

if [[ "$PURGE" -eq 1 ]]; then
    run_root rm -rf "$CONFIG_DIR" "$DATA_DIR"
    echo "Purged config and data directories"
else
    echo "Config and data preserved: $CONFIG_DIR, $DATA_DIR"
fi
