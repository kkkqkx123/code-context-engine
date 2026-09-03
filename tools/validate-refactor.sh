#!/bin/bash
# Validation test suite for mod.rs refactoring
# 
# Usage:
#   ./tools/validate-refactor.sh <crate_directory>
#
# Example:
#   ./tools/validate-refactor.sh crates/cce-cli

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

CRATE_DIR="$1"

if [ -z "$CRATE_DIR" ]; then
    echo "Usage: $0 <crate_directory>"
    echo "Example: $0 crates/cce-cli"
    exit 1
fi

if [ ! -d "$CRATE_DIR" ]; then
    echo "Error: Directory $CRATE_DIR does not exist"
    exit 1
fi

echo "========================================"
echo "Validation Suite for: $CRATE_DIR"
echo "========================================"
echo ""

PASS=0
FAIL=0
WARN=0

# 1. Check for orphaned mod.rs files
echo -n "Checking for orphaned mod.rs files... "
ORPHANED=$(find "$CRATE_DIR" -name "mod.rs" -type f \
    -not -path "*/target/*" \
    -not -path "*/benches/*" \
    -not -path "*/fixtures/*" \
    -not -path "*/.git/*" 2>/dev/null)
if [ -z "$ORPHANED" ]; then
    echo -e "${GREEN}PASS${NC} (no mod.rs files remain)"
    PASS=$((PASS + 1))
else
    ORPHANED_COUNT=$(echo "$ORPHANED" | wc -l)
    echo -e "${YELLOW}WARN${NC} ($ORPHANED_COUNT mod.rs files remain)"
    echo "$ORPHANED" | while IFS= read -r f; do echo "  $f"; done
    WARN=$((WARN + 1))
fi

# 2. Compilation check
echo -n "Running cargo check... "
PROJECT_DIR=$(git rev-parse --show-toplevel 2>/dev/null || echo "$CRATE_DIR/..")
CRATE_NAME=$(basename "$CRATE_DIR")

if [ -f "$CRATE_DIR/Cargo.toml" ]; then
    if cargo check -p "$CRATE_NAME" 2>&1; then
        echo -e "${GREEN}PASS${NC}"
        PASS=$((PASS + 1))
    else
        echo -e "${RED}FAIL${NC}"
        FAIL=$((FAIL + 1))
    fi
else
    echo -e "${YELLOW}SKIP${NC} (no Cargo.toml found)"
    WARN=$((WARN + 1))
fi

# 3. Test check
echo -n "Running cargo test (lib)... "
if [ -f "$CRATE_DIR/Cargo.toml" ]; then
    if cargo test -p "$CRATE_NAME" --lib 2>&1; then
        echo -e "${GREEN}PASS${NC}"
        PASS=$((PASS + 1))
    else
        echo -e "${RED}FAIL${NC}"
        FAIL=$((FAIL + 1))
    fi
else
    echo -e "${YELLOW}SKIP${NC} (no Cargo.toml found)"
    WARN=$((WARN + 1))
fi

# 4. Verify module structure consistency
echo -n "Checking module structure consistency... "
if [ -f "$CRATE_DIR/src/lib.rs" ]; then
    ROOT_MOD="$CRATE_DIR/src/lib.rs"
elif [ -f "$CRATE_DIR/src/main.rs" ]; then
    ROOT_MOD="$CRATE_DIR/src/main.rs"
else
    ROOT_MOD=""
fi

if [ -n "$ROOT_MOD" ]; then
    # Extract all module declarations from root
    MODS=$(grep -E '^pub mod |^mod ' "$ROOT_MOD" | sed 's/.*mod //;s/;//;s/ .*//' | tr -d ' ')
    MISSING=0
    for mod_name in $MODS; do
        # Check if the module file exists (either dir.rs or dir/mod.rs)
        if [ ! -f "$CRATE_DIR/src/$mod_name.rs" ] && [ ! -d "$CRATE_DIR/src/$mod_name" ]; then
            MISSING=$((MISSING + 1))
            echo "  Missing module: $mod_name"
        fi
    done
    if [ $MISSING -eq 0 ]; then
        echo -e "${GREEN}PASS${NC}"
        PASS=$((PASS + 1))
    else
        echo -e "${RED}FAIL${NC} ($MISSING modules missing)"
        FAIL=$((FAIL + 1))
    fi
else
    echo -e "${YELLOW}SKIP${NC} (no lib.rs or main.rs found)"
    WARN=$((WARN + 1))
fi

# 5. Clippy check
echo -n "Running cargo clippy... "
if [ -f "$CRATE_DIR/Cargo.toml" ]; then
    if cargo clippy -p "$CRATE_NAME" --all-targets 2>&1; then
        echo -e "${GREEN}PASS${NC}"
        PASS=$((PASS + 1))
    else
        echo -e "${RED}FAIL${NC}"
        FAIL=$((FAIL + 1))
    fi
else
    echo -e "${YELLOW}SKIP${NC} (no Cargo.toml found)"
    WARN=$((WARN + 1))
fi

echo ""
echo "========================================"
echo "Validation Results"
echo "========================================"
echo -e "${GREEN}Passed: $PASS${NC}"
echo -e "${RED}Failed: $FAIL${NC}"
echo -e "${YELLOW}Warnings: $WARN${NC}"
echo ""

if [ $FAIL -gt 0 ]; then
    echo -e "${RED}Validation FAILED${NC}"
    exit 1
else
    echo -e "${GREEN}Validation PASSED${NC}"
    exit 0
fi