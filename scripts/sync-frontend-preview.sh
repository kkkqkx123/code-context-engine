#!/bin/bash
# sync-frontend-preview.sh
# Sync source files from frontend to frontend-preview
# Preserves mock data and mock-enabled client
#
# Synced trees are MIRRORED: files removed from frontend are also removed
# from frontend-preview, except for the preserved preview-only files.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
FRONTEND_DIR="$ROOT_DIR/frontend"
PREVIEW_DIR="$ROOT_DIR/frontend-preview"

# Mirror-delete files under $1 (preview tree) that have no counterpart under
# $2 (frontend tree). Relative paths are matched against $3 (space-separated
# basenames to preserve). Empty preview dirs left behind are pruned.
prune_deleted() {
    local preview_tree="$1" frontend_tree="$2" preserve="$3"
    (cd "$preview_tree" && find . -type f) | while read -r rel; do
        local base
        base="$(basename "$rel")"
        case " $preserve " in
            *" $base "*) continue ;;
        esac
        if [ ! -e "$frontend_tree/$rel" ]; then
            rm "$preview_tree/$rel"
            echo "  removed (deleted in frontend): ${rel#./}"
        fi
    done
    find "$preview_tree" -type d -empty -delete 2>/dev/null || true
}

echo "=== Syncing frontend-preview ==="

# Check source directory exists
if [ ! -d "$FRONTEND_DIR" ]; then
    echo "Error: frontend directory not found at $FRONTEND_DIR"
    exit 1
fi

# Check preview directory exists
if [ ! -d "$PREVIEW_DIR" ]; then
    echo "Error: frontend-preview directory not found at $PREVIEW_DIR"
    echo "Run this script from the project root or create frontend-preview first."
    exit 1
fi

# Sync static assets (favicon, images, etc.)
echo "Syncing static assets..."
mkdir -p "$PREVIEW_DIR/static"
cp -r "$FRONTEND_DIR/static/." "$PREVIEW_DIR/static/"
prune_deleted "$PREVIEW_DIR/static" "$FRONTEND_DIR/static" ""

# Sync global styles, HTML template, and ambient type declarations
echo "Syncing global styles, HTML template, and ambient type declarations..."
cp "$FRONTEND_DIR/src/app.html" "$PREVIEW_DIR/src/"
cp "$FRONTEND_DIR/src/app.css" "$PREVIEW_DIR/src/"
cp "$FRONTEND_DIR/src/app.d.ts" "$PREVIEW_DIR/src/"

# Sync all components (ui, index, search, entities, tools, graph)
echo "Syncing components..."
mkdir -p "$PREVIEW_DIR/src/lib/components"
cp -r "$FRONTEND_DIR/src/lib/components/." "$PREVIEW_DIR/src/lib/components/"
prune_deleted "$PREVIEW_DIR/src/lib/components" "$FRONTEND_DIR/src/lib/components" ""

# Sync stores
echo "Syncing stores..."
mkdir -p "$PREVIEW_DIR/src/lib/stores"
cp -r "$FRONTEND_DIR/src/lib/stores/." "$PREVIEW_DIR/src/lib/stores/"
prune_deleted "$PREVIEW_DIR/src/lib/stores" "$FRONTEND_DIR/src/lib/stores" ""

# Sync shared utils (graph presentation model, formatters)
echo "Syncing utils..."
mkdir -p "$PREVIEW_DIR/src/lib/utils"
cp -r "$FRONTEND_DIR/src/lib/utils/." "$PREVIEW_DIR/src/lib/utils/"
prune_deleted "$PREVIEW_DIR/src/lib/utils" "$FRONTEND_DIR/src/lib/utils" ""

# Sync API modules (but preserve mock-enabled client.ts)
echo "Syncing API modules..."
mkdir -p "$PREVIEW_DIR/src/lib/api"
for file in "$FRONTEND_DIR/src/lib/api/"*.ts; do
    [ -e "$file" ] || continue
    basename=$(basename "$file")
    # Skip client.ts - we have a mock-enabled version
    if [ "$basename" != "client.ts" ]; then
        cp "$file" "$PREVIEW_DIR/src/lib/api/"
    fi
done
prune_deleted "$PREVIEW_DIR/src/lib/api" "$FRONTEND_DIR/src/lib/api" "client.ts"

# Sync ambient type declarations (e.g. third-party module shims)
echo "Syncing types..."
mkdir -p "$PREVIEW_DIR/src/types"
for file in "$FRONTEND_DIR/src/types/"*.d.ts; do
    [ -e "$file" ] || continue
    cp "$file" "$PREVIEW_DIR/src/types/"
done
prune_deleted "$PREVIEW_DIR/src/types" "$FRONTEND_DIR/src/types" ""

# Sync routes (but NOT the preview-specific mock files)
echo "Syncing routes..."
mkdir -p "$PREVIEW_DIR/src/routes"
cp -r "$FRONTEND_DIR/src/routes/." "$PREVIEW_DIR/src/routes/"
prune_deleted "$PREVIEW_DIR/src/routes" "$FRONTEND_DIR/src/routes" ""

# Sync package.json dependencies (preserve preview name and mock .env)
echo "Syncing dependencies..."
PREVIEW_NAME=$(jq -r '.name' "$PREVIEW_DIR/package.json")
jq -s '.[0] * { dependencies: .[1].dependencies, devDependencies: .[1].devDependencies }' "$PREVIEW_DIR/package.json" "$FRONTEND_DIR/package.json" > "$PREVIEW_DIR/package.json.tmp"
jq --arg name "$PREVIEW_NAME" '.name = $name' "$PREVIEW_DIR/package.json.tmp" > "$PREVIEW_DIR/package.json"
rm "$PREVIEW_DIR/package.json.tmp"

echo "=== Sync complete ==="
echo ""
echo "Files synced:"
echo "  - static/**/* (all static assets, e.g. favicon.png)"
echo "  - src/app.html, src/app.css, src/app.d.ts"
echo "  - src/lib/components/**/* (all components)"
echo "  - src/lib/stores/*.ts (all stores)"
echo "  - src/lib/utils/*.ts (all utils)"
echo "  - src/lib/api/*.ts (except client.ts - mock-enabled)"
echo "  - src/types/*.d.ts (ambient declarations)"
echo "  - src/routes/**/* (all routes)"
echo "  - dependencies and devDependencies from package.json"
echo ""
echo "Preserved files (not overwritten):"
echo "  - src/lib/api/client.ts (mock-enabled version)"
echo "  - src/lib/mock/* (mock data files)"
echo "  - .env (mock mode config)"
echo ""
echo "Run 'cd frontend-preview && npm install' to update dependencies."
