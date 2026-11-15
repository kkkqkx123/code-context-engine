#!/usr/bin/env bash
# Install Code Context Engine binaries, initialize configuration and
# optionally install a systemd service for the HTTP server.
#
# Usage:
#   deploy/install.sh [options]
#
# Options:
#   --build              Build before installing (default: auto)
#   --no-build           Skip build
#   --service            Install systemd service for cce server
#   --no-service         Do not install service (default)
#   --enable-autostart   Enable service at boot (requires --service)
#   --system             System-wide install (requires root; default: user)
#   --force, -f          Overwrite existing config files
#   -h, --help           Show help
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/build"

DO_BUILD="auto"
DO_SERVICE=0
DO_AUTOSTART=0
SCOPE="user"
FORCE=0

usage() { sed -n '2,/^$/p' "$0" | sed 's/^# \?//'; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --build) DO_BUILD=1; shift ;;
        --no-build|--skip-build) DO_BUILD=0; shift ;;
        --service) DO_SERVICE=1; shift ;;
        --no-service) DO_SERVICE=0; shift ;;
        --enable-autostart|--autostart) DO_AUTOSTART=1; shift ;;
        --system) SCOPE="system"; shift ;;
        --force|-f) FORCE=1; shift ;;
        -h|--help|help) usage; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

if [[ "$DO_AUTOSTART" -eq 1 && "$DO_SERVICE" -eq 0 ]]; then
    echo "Error: --enable-autostart requires --service" >&2
    exit 1
fi
if [[ "$SCOPE" == "system" && "$DO_SERVICE" -eq 0 ]]; then
    echo "Error: --system is only valid with --service" >&2
    exit 1
fi

if [[ "$DO_BUILD" == "auto" ]]; then
    if [[ -x "$BUILD/cce" && -x "$BUILD/cce-cli" ]]; then
        DO_BUILD=0
    else
        DO_BUILD=1
    fi
fi

if [[ "$DO_BUILD" -eq 1 ]]; then
    echo "==> Building..."
    bash "$ROOT/deploy/build.sh" all
fi

for b in cce cce-cli cce-mcp; do
    if [[ ! -x "$BUILD/$b" ]]; then
        echo "Error: missing binary $BUILD/$b (run with --build)" >&2
        exit 1
    fi
done

OS="$(uname -s 2>/dev/null || echo unknown)"

user_base() {
    if [[ -n "${XDG_CONFIG_HOME:-}" ]]; then
        printf '%s' "$XDG_CONFIG_HOME"
    else
        printf '%s' "$HOME/.config"
    fi
}

user_data() {
    if [[ -n "${XDG_DATA_HOME:-}" ]]; then
        printf '%s' "$XDG_DATA_HOME"
    else
        printf '%s' "$HOME/.local/share"
    fi
}

if [[ "$SCOPE" == "system" ]]; then
    BIN_DIR="/usr/local/bin"
    CONFIG_DIR="/etc/cce"
    DATA_DIR="/var/lib/cce"
    UNIT_DEST="/etc/systemd/system/cce-server.service"
    TARGET="multi-user.target"
    SUDO="sudo"
    if [[ "$(id -u)" -eq 0 ]]; then
        SUDO=""
    elif ! command -v sudo >/dev/null 2>&1; then
        echo "Error: --system requires root or sudo" >&2
        exit 1
    fi
else
    BIN_DIR="$HOME/.local/bin"
    CONFIG_DIR="$(user_base)/cce"
    DATA_DIR="$(user_data)/cce"
    UNIT_DEST="$(user_base)/systemd/user/cce-server.service"
    TARGET="default.target"
    SUDO=""
fi

run_root() {
    if [[ -n "$SUDO" ]]; then
        sudo "$@"
    else
        "$@"
    fi
}

copy_template() {
    local src="$1" dest="$2" label="$3"
    if [[ -f "$dest" && "$FORCE" -eq 0 ]]; then
        echo "    $label already exists, skipping (use --force): $dest"
        return 0
    fi
    run_root mkdir -p "$(dirname "$dest")"
    run_root cp "$src" "$dest"
    run_root chmod 600 "$dest"
    echo "    wrote: $dest"
}

mkdir -p "$CONFIG_DIR"
run_root mkdir -p "$DATA_DIR"

echo "==> Installing binaries to $BIN_DIR ..."
run_root mkdir -p "$BIN_DIR"
for b in cce cce-cli cce-mcp; do
    run_root install -m 755 "$BUILD/$b" "$BIN_DIR/$b"
    echo "    installed: $BIN_DIR/$b"
done

echo "==> Initializing configuration ..."
copy_template "$ROOT/deploy/configs/config.example.toml" "$CONFIG_DIR/config.toml" "config.toml"
copy_template "$ROOT/deploy/configs/.env.example" "$CONFIG_DIR/.env" ".env" 2>/dev/null || \
    copy_template "$ROOT/.env.example" "$CONFIG_DIR/.env" ".env" 2>/dev/null || true

echo "    config: $CONFIG_DIR/config.toml"
echo "    env:    $CONFIG_DIR/.env"
echo "    data:   $DATA_DIR"

if [[ "$DO_SERVICE" -eq 0 ]]; then
    if [[ "$OS" != "Linux" ]]; then
        echo "==> Non-Linux system: service not installed."
        echo "    Start manually: CCE_CONFIG=$CONFIG_DIR/config.toml $BIN_DIR/cce"
    else
        echo "==> Service not installed. Start manually:"
        echo "    CCE_CONFIG=$CONFIG_DIR/config.toml $BIN_DIR/cce"
    fi
    exit 0
fi

if [[ "$OS" != "Linux" ]]; then
    echo "==> Non-Linux system: skipping systemd installation."
    echo "    Start manually: CCE_CONFIG=$CONFIG_DIR/config.toml $BIN_DIR/cce"
    exit 0
fi

if ! command -v systemctl >/dev/null 2>&1; then
    echo "Error: systemctl not found" >&2
    exit 1
fi

echo "==> Rendering systemd unit ..."
run_root mkdir -p "$(dirname "$UNIT_DEST")"
unit_tmp="$(mktemp)"
sed \
    -e "s|@BIN@|$BIN_DIR|g" \
    -e "s|@CONFIG@|$CONFIG_DIR/config.toml|g" \
    -e "s|@DATA_DIR@|$DATA_DIR|g" \
    -e "s|@ENV_FILE@|$CONFIG_DIR/.env|g" \
    -e "s|@TARGET@|$TARGET|g" \
    "$ROOT/deploy/cce-server.service" > "$unit_tmp"
run_root install -m 644 "$unit_tmp" "$UNIT_DEST"
rm -f "$unit_tmp"
echo "    unit: $UNIT_DEST"

if [[ -n "$SUDO" ]]; then
    sudo systemctl daemon-reload
else
    systemctl --user daemon-reload
fi

if [[ "$DO_AUTOSTART" -eq 1 ]]; then
    echo "==> Enabling autostart ..."
    if [[ -n "$SUDO" ]]; then
        sudo systemctl enable cce-server.service
    else
        systemctl --user enable cce-server.service
        if command -v loginctl >/dev/null 2>&1; then
            if loginctl enable-linger "$(id -un)" >/dev/null 2>&1; then
                echo "    linger enabled"
            else
                echo "    user-level autostart enabled (starts after login)"
                echo "    for boot autostart run: sudo loginctl enable-linger $(id -un)"
            fi
        fi
    fi
else
    echo "    autostart not enabled (add --enable-autostart)"
fi

echo "==> Done. Common commands:"
if [[ -n "$SUDO" ]]; then
    echo "    sudo systemctl start cce-server.service"
    echo "    sudo systemctl status cce-server.service"
    echo "    sudo journalctl -u cce-server.service -f"
else
    echo "    systemctl --user start cce-server.service"
    echo "    systemctl --user status cce-server.service"
    echo "    journalctl --user -u cce-server.service -f"
    echo "    (ensure $BIN_DIR is in PATH)"
fi
