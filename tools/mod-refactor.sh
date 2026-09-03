#!/bin/bash
# mod.rs Refactoring Tool
# Converts traditional mod.rs file structure to modern Rust convention.
#
# Transformation:
#   src/foo/mod.rs  →  src/foo.rs
#   src/foo/bar/mod.rs  →  src/foo/bar.rs
#
# This tool does NOT modify parent module declarations (mod foo;),
# because Rust resolves mod foo; by looking for foo.rs first,
# then foo/mod.rs as fallback.
#
# Usage:
#   ./tools/mod-refactor.sh -p <package_name> [--execute]
#   ./tools/mod-refactor.sh <crate_directory> [--execute]
#
# Examples:
#   ./tools/mod-refactor.sh -p cce-cli --execute
#   ./tools/mod-refactor.sh crates/cce_core --dry-run
#
# Output:
#   Prints a mapping of old_path → new_path for each renamed file.
#   Use --csv to output machine-readable format for import path adjustment.

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# ── Argument Parsing ──────────────────────────────────────────────────

CRATE_DIR=""
DRY_RUN=true
CSV=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        -p)
            shift
            PACKAGE_NAME="$1"
            # Resolve package name to directory path
            if [ -d "crates/$1" ]; then
                CRATE_DIR="crates/$1"
            elif [ -d "$1" ]; then
                CRATE_DIR="$1"
            else
                # Try to find it from workspace
                PACKAGE_DIR=$(grep -l "name.*=.*\"$1\"" Cargo.toml crates/*/Cargo.toml 2>/dev/null | head -1 | xargs dirname 2>/dev/null)
                if [ -n "$PACKAGE_DIR" ]; then
                    CRATE_DIR="$PACKAGE_DIR"
                else
                    echo "Error: Cannot find package '$1'"
                    exit 1
                fi
            fi
            ;;
        --execute)
            DRY_RUN=false
            ;;
        --dry-run)
            DRY_RUN=true
            ;;
        --csv)
            CSV=true
            ;;
        -*)
            echo "Unknown option: $1"
            echo "Usage: $0 [-p <package> | <path>] [--execute] [--csv]"
            exit 1
            ;;
        *)
            CRATE_DIR="$1"
            ;;
    esac
    shift
done

if [ -z "$CRATE_DIR" ]; then
    echo "Usage: $0 [-p <package> | <path>] [--execute] [--csv]"
    echo ""
    echo "Examples:"
    echo "  $0 -p cce-cli --dry-run          # Preview changes for cce-cli"
    echo "  $0 -p cce-cli --execute           # Apply changes for cce-cli"
    echo "  $0 crates/cce_core --execute      # Apply changes by path"
    echo "  $0 -p cce-cli --execute --csv     # Apply and output CSV mapping"
    exit 1
fi

if [ ! -d "$CRATE_DIR" ]; then
    echo "Error: Directory $CRATE_DIR does not exist"
    exit 1
fi

# ── Find mod.rs files ────────────────────────────────────────────────

find_mod_files() {
    find "$CRATE_DIR" -name "mod.rs" -type f \
        -not -path "*/target/*" \
        -not -path "*/benches/*" \
        -not -path "*/fixtures/*" \
        -not -path "*/.git/*" \
        | sort
}

# ── Print header ─────────────────────────────────────────────────────

echo "=============================================="
echo "  mod.rs Refactoring Tool"
echo "=============================================="
echo "  Target:   $CRATE_DIR"
echo "  Mode:     $(if $DRY_RUN; then echo 'DRY RUN'; else echo 'EXECUTE'; fi)"
echo ""

# ── Analysis ─────────────────────────────────────────────────────────

MOD_FILES=$(find_mod_files)
TOTAL=$(echo "$MOD_FILES" | grep -c . || true)

if [ "$TOTAL" -eq 0 ]; then
    echo "  No mod.rs files found in $CRATE_DIR"
    exit 0
fi

echo "  Found $TOTAL mod.rs files"
echo ""

CONFLICTS=()
REFACTORABLE=0
CONFLICT_COUNT=0
MAPPING=()

echo "  ┌─────────────────────────────────────────────────────────────────┐"
echo "  │  Changes                                                       │"
echo "  └─────────────────────────────────────────────────────────────────┘"

while IFS= read -r mod_file; do
    [ -z "$mod_file" ] && continue

    parent_dir=$(dirname "$mod_file")
    grandparent_dir=$(dirname "$parent_dir")
    dir_name=$(basename "$parent_dir")
    new_file="$grandparent_dir/$dir_name.rs"

    if [ -f "$new_file" ]; then
        CONFLICTS+=("$mod_file")
        CONFLICT_COUNT=$((CONFLICT_COUNT + 1))
        echo -e "  ${YELLOW}⚠  CONFLICT${NC}  $mod_file"
        echo -e "  ${YELLOW}   → already exists: $new_file${NC}"
    else
        REFACTORABLE=$((REFACTORABLE + 1))
        MAPPING+=("$mod_file → $new_file")
        echo -e "  ${GREEN}✓${NC}  $mod_file"
        echo -e "     → $new_file"
    fi
done <<< "$MOD_FILES"

echo ""
echo "  ┌─────────────────────────────────────────────────────────────────┐"
echo "  │  Summary                                                       │"
echo "  └─────────────────────────────────────────────────────────────────┘"
echo "  Total:      $TOTAL"
echo "  Rename:     $REFACTORABLE"
echo "  Conflicts:  $CONFLICT_COUNT"
echo ""

if [ $CONFLICT_COUNT -gt 0 ]; then
    echo -e "  ${YELLOW}⚠  Conflicts detected (inline module pattern).${NC}"
    echo "  These need manual handling:"
    echo "  - Merge mod.rs content into the existing <dir>.rs file"
    echo "  - Or rename the existing <dir>.rs to disambiguate"
    echo ""
fi

# ── Execution ────────────────────────────────────────────────────────

if ! $DRY_RUN && [ $REFACTORABLE -gt 0 ]; then
    echo "  ┌─────────────────────────────────────────────────────────────────┐"
    echo "  │  Applying Changes                                              │"
    echo "  └─────────────────────────────────────────────────────────────────┘"

    MODIFIED=()
    ERRORS=()

    while IFS= read -r mod_file; do
        [ -z "$mod_file" ] && continue

        parent_dir=$(dirname "$mod_file")
        grandparent_dir=$(dirname "$parent_dir")
        dir_name=$(basename "$parent_dir")
        new_file="$grandparent_dir/$dir_name.rs"

        if [ -f "$new_file" ]; then
            continue
        fi

        if git mv "$mod_file" "$new_file" 2>/dev/null; then
            echo -e "  ${GREEN}✓${NC}  $(basename $(dirname $CRATE_DIR))/$(echo $new_file | sed "s|$CRATE_DIR/||")"
            MODIFIED+=("$mod_file → $new_file")
        elif mv "$mod_file" "$new_file"; then
            echo -e "  ${GREEN}✓${NC}  $(basename $(dirname $CRATE_DIR))/$(echo $new_file | sed "s|$CRATE_DIR/||") (mv)"
            MODIFIED+=("$mod_file → $new_file")
        else
            ERRORS+=("$mod_file")
            echo -e "  ${RED}✗${NC}  Failed to rename: $mod_file"
        fi
    done <<< "$MOD_FILES"

    echo ""
    echo "  Successfully renamed: ${#MODIFIED[@]}"
    echo "  Errors:              ${#ERRORS[@]}"
    echo ""

    # ── Output CSV mapping ──────────────────────────────────────────────────
    if $CSV && [ ${#MODIFIED[@]} -gt 0 ]; then
        echo "  ┌─────────────────────────────────────────────────────────────────┐"
        echo "  │  File Mapping (CSV)                                            │"
        echo "  └─────────────────────────────────────────────────────────────────┘"
        echo "  old_path,new_path"
        for entry in "${MODIFIED[@]}"; do
            old="${entry%% → *}"
            new="${entry##* → }"
            echo "  $old,$new"
        done
        echo ""
    fi
fi

# ── Post-migration steps ─────────────────────────────────────────────

echo "  ┌─────────────────────────────────────────────────────────────────┐"
echo "  │  Post-Migration Verification                                   │"
echo "  └─────────────────────────────────────────────────────────────────┘"
echo "  1. cargo check -p <package>"
echo "  2. cargo test -p <package> --lib"
echo "  3. git diff --stat"
echo ""

# Remove empty directories
if ! $DRY_RUN; then
    EMPTY_DIRS=$(find "$CRATE_DIR" -type d -empty -not -path "*/target/*" 2>/dev/null)
    if [ -n "$EMPTY_DIRS" ]; then
        echo "  Empty directories (safe to remove):"
        echo "$EMPTY_DIRS" | while IFS= read -r d; do
            echo "    $d"
        done
        echo ""
    fi
fi