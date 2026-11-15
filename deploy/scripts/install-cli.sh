#!/usr/bin/env bash
# Install only CLI binaries into the user bin directory.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/build"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
mkdir -p "$BIN_DIR"
for b in cce-cli cce cce-mcp; do
    if [[ -x "$BUILD/$b" ]]; then
        install -m 755 "$BUILD/$b" "$BIN_DIR/$b"
        echo "installed: $BIN_DIR/$b"
    fi
done
