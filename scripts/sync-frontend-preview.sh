#!/bin/bash
# sync-frontend-preview.sh
# Sync source files from frontend to frontend-preview
# Preserves mock data and mock-enabled client

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
FRONTEND_DIR="$ROOT_DIR/frontend"
PREVIEW_DIR="$ROOT_DIR/frontend-preview"

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

# Sync global styles and HTML template
echo "Syncing global styles and HTML template..."
cp "$FRONTEND_DIR/src/app.html" "$PREVIEW_DIR/src/"
cp "$FRONTEND_DIR/src/app.css" "$PREVIEW_DIR/src/"

# Sync all components (ui, index, search, entities, tools)
echo "Syncing components..."
mkdir -p "$PREVIEW_DIR/src/lib/components"
cp -r "$FRONTEND_DIR/src/lib/components/"* "$PREVIEW_DIR/src/lib/components/"

# Sync stores
echo "Syncing stores..."
mkdir -p "$PREVIEW_DIR/src/lib/stores"
cp "$FRONTEND_DIR/src/lib/stores/"*.ts "$PREVIEW_DIR/src/lib/stores/"

# Sync API modules (but preserve mock-enabled client.ts)
echo "Syncing API modules..."
mkdir -p "$PREVIEW_DIR/src/lib/api"
for file in "$FRONTEND_DIR/src/lib/api/"*.ts; do
    basename=$(basename "$file")
    # Skip client.ts - we have a mock-enabled version
    if [ "$basename" != "client.ts" ]; then
        cp "$file" "$PREVIEW_DIR/src/lib/api/"
    fi
done

# Sync routes (but NOT the preview-specific mock files)
echo "Syncing routes..."
mkdir -p "$PREVIEW_DIR/src/routes"
cp -r "$FRONTEND_DIR/src/routes/"* "$PREVIEW_DIR/src/routes/"

# Sync package.json devDependencies (preserve preview name and mock .env)
echo "Syncing devDependencies..."
PREVIEW_NAME=$(jq -r '.name' "$PREVIEW_DIR/package.json")
jq -s '.[0] * { devDependencies: .[1].devDependencies }' "$PREVIEW_DIR/package.json" "$FRONTEND_DIR/package.json" > "$PREVIEW_DIR/package.json.tmp"
jq --arg name "$PREVIEW_NAME" '.name = $name' "$PREVIEW_DIR/package.json.tmp" > "$PREVIEW_DIR/package.json"
rm "$PREVIEW_DIR/package.json.tmp"

echo "=== Sync complete ==="
echo ""
echo "Files synced:"
echo "  - src/app.html, src/app.css"
echo "  - src/lib/components/**/* (all components)"
echo "  - src/lib/stores/*.ts (all stores)"
echo "  - src/lib/api/*.ts (except client.ts - mock-enabled)"
echo "  - src/routes/**/* (all routes)"
echo "  - devDependencies from package.json"
echo ""
echo "Preserved files (not overwritten):"
echo "  - src/lib/api/client.ts (mock-enabled version)"
echo "  - src/lib/mock/* (mock data files)"
echo "  - .env (mock mode config)"
echo ""
echo "Run 'cd frontend-preview && npm install' to update dependencies."
