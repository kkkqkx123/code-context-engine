#!/usr/bin/env bash
# Build the frontend static bundle into frontend/build/.
#
# Usage:
#   deploy/build-frontend.sh [--skip-install] [--preview]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FRONT="$ROOT/frontend"
OUT="$FRONT/build"

SKIP_INSTALL=0
MODE="production"
for arg in "$@"; do
    case "$arg" in
        --skip-install) SKIP_INSTALL=1 ;;
        --preview) MODE="preview" ;;
        -h|--help|help)
            sed -n '2,6p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "Unknown argument: $arg" >&2; exit 2 ;;
    esac
done

if [[ ! -f "$FRONT/package.json" ]]; then
    echo "Error: frontend project not found at $FRONT" >&2
    exit 1
fi
if ! command -v npm >/dev/null 2>&1; then
    echo "Error: npm not found; install Node.js first" >&2
    exit 1
fi

if [[ "$SKIP_INSTALL" -eq 0 ]]; then
    echo "==> Installing frontend dependencies ..."
    (cd "$FRONT" && npm ci)
else
    echo "==> Skipping dependency install (--skip-install)"
fi

echo "==> Building frontend (mode=$MODE) ..."
if [[ "$MODE" == "preview" ]]; then
    (cd "$FRONT" && npm run build:preview)
else
    (cd "$FRONT" && npm run build)
fi

if [[ ! -f "$OUT/index.html" ]]; then
    echo "Error: frontend build produced no $OUT/index.html" >&2
    exit 1
fi
echo "==> Frontend build complete: $OUT"
