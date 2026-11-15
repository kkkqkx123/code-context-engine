#!/usr/bin/env bash
# Build the Code Context Engine binaries into ./build/.
#
# Usage:
#   deploy/build.sh [server|cli|mcp|all] [--debug] [--admission]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/build"
MODE="release"
TARGET="all"
FEATURES=""

for arg in "$@"; do
    case "$arg" in
        server|cli|mcp|all) TARGET="$arg" ;;
        --debug) MODE="debug" ;;
        --admission) FEATURES="${FEATURES:+$FEATURES,}admission" ;;
        -h|--help|help)
            sed -n '2,12p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "Unknown argument: $arg" >&2; exit 2 ;;
    esac
done

mkdir -p "$OUT"

CARGO_ARGS=(build)
if [[ "$MODE" == "release" ]]; then
    CARGO_ARGS+=(--release)
fi
if [[ -n "$FEATURES" ]]; then
    CARGO_ARGS+=(--features "$FEATURES")
fi

build_one() {
    local bin="$1"
    echo "==> Building $bin ($MODE${FEATURES:+, features=$FEATURES})"
    (
        cd "$ROOT"
        cargo "${CARGO_ARGS[@]}" --bin "$bin"
    )
    local src="$ROOT/target/$MODE/$bin"
    cp "$src" "$OUT/$bin"
    chmod 755 "$OUT/$bin"
    echo "    -> $OUT/$bin"
}

case "$TARGET" in
    server) build_one cce ;;
    cli) build_one cce-cli ;;
    mcp) build_one cce-mcp ;;
    all) build_one cce; build_one cce-cli; build_one cce-mcp ;;
esac

echo "==> Build complete: $OUT"
