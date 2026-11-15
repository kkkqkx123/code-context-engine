#!/usr/bin/env bash
# Assemble the single-host bundle: one server binary plus one static dir.
#
# Layout of the bundle:
#   dist/cce-single/
#     cce                 # server binary (serves API + static on one port)
#     web/                # frontend static bundle (frontend/build content)
#     config.single.toml  # loopback + static-dir example
#     run-single.sh       # local start helper
#
# Usage:
#   deploy/package-single.sh [--debug] [--skip-frontend-install] [--preview]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist/cce-single"
WEB="$DIST/web"

MODE="release"
FRONT_ARGS=()
for arg in "$@"; do
    case "$arg" in
        --debug) MODE="debug" ;;
        --skip-frontend-install) FRONT_ARGS+=("--skip-install") ;;
        --preview) FRONT_ARGS+=("--preview") ;;
        -h|--help|help)
            sed -n '2,15p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "Unknown argument: $arg" >&2; exit 2 ;;
    esac
done

echo "==> Building backend ..."
bash "$ROOT/deploy/build.sh" server $([[ "$MODE" == "debug" ]] && echo "--debug")
if [[ ! -x "$ROOT/build/cce" ]]; then
    echo "Error: missing backend binary $ROOT/build/cce" >&2
    exit 1
fi

echo "==> Building frontend ..."
bash "$ROOT/deploy/build-frontend.sh" "${FRONT_ARGS[@]}"
if [[ ! -f "$ROOT/frontend/build/index.html" ]]; then
    echo "Error: missing frontend bundle $ROOT/frontend/build/index.html" >&2
    exit 1
fi

echo "==> Assembling single-host bundle ..."
rm -rf "$DIST"
mkdir -p "$WEB"
install -m 755 "$ROOT/build/cce" "$DIST/cce"
cp -r "$ROOT/frontend/build/." "$WEB/"
chmod -R a+r "$WEB"

cat > "$DIST/config.single.toml" <<'EOF'
# Single-host bundle example: loopback API + static on one port.
[server]
host = "127.0.0.1"
port = 9000

[server.frontend]
enabled = true
dir = "web"
EOF

cat > "$DIST/run-single.sh" <<'EOF'
#!/usr/bin/env bash
# Start the single-host bundle from its own directory.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"
CCE_CONFIG="${CCE_CONFIG:-$HERE/config.single.toml}" exec "$HERE/cce"
EOF
chmod 755 "$DIST/run-single.sh"
chmod 600 "$DIST/config.single.toml"

echo "==> Bundle ready: $DIST"
ls "$DIST"
echo "    start: $DIST/run-single.sh"
echo "    open:  http://127.0.0.1:9000/"
echo "    note:  [server.frontend] takes effect once the backend"
echo "           implements static hosting per docs/plan/single-binary-static-hosting-2026-11-15.md"
