#!/usr/bin/env bash
# Publish the CCE workspace crates to crates.io in dependency order
# (dependencies before dependents).
#
# Sub-repository crates are NOT published here; each sub-repo ships its
# own publish script and must be published first:
#   - simvec, encoding-guess, llm-suite: already published
#   - tantivy (as the `kkkqkx-tantivy` family), tree-sitter-svelte and
#     tree-sitter-vue (as `kkkqkx-tree-sitter-*`): run the publish script
#     inside each sub-repo before this one.
# Crates marked `publish = false` (cce-e2e-tests) are excluded from the
# list below.
#
# Usage:
#   ./scripts/publish.sh           # real publish
#   ./scripts/publish.sh --dry-run # cargo publish --dry-run
set -euo pipefail

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
    DRY_RUN=true
fi

# Ordered so that every internal dependency appears before its dependents.
CRATES=(
    "crates/core/cce-metrics"
    "crates/core/cce-utils"
    "crates/app/cce-bench-kit"
    "crates/app/cce-api"
    "crates/app/cce-admission"
    "crates/infra/cce-circuit-breaker"
    "crates/core/cce-types"
    "crates/core/cce-text"
    "crates/core/cce-plugin"
    "crates/parser/cce-parser-core"
    "crates/parser/cce-stdlib"
    "crates/core/cce-config"
    "crates/app/cce-plugin-sdk"
    "crates/core/cce-llm"
    "crates/app/cce-plugin-runtime"
    "crates/infra/cce-llm-client"
    "crates/infra/cce-scanner"
    "crates/app/cce-gateway"
    "crates/infra/cce-storage-common"
    "crates/infra/cce-storage-vector-local"
    "crates/infra/cce-storage-vector-qdrant"
    "crates/infra/cce-storage-metadb-sqlite"
    "crates/infra/cce-storage-metadb-pg"
    "crates/infra/cce-storage-bm25"
    "crates/parser/cce-document"
    "crates/parser/cce-parser"
    "crates/parser/cce-codegraph"
    "crates/app/cce-orchestrator"
    "crates/app/cce-server-shared"
    "crates/app/cce-mcp"
    "crates/app/cce-server"
    "crates/app/cce-cli"
)

for crate in "${CRATES[@]}"; do
    name=$(basename "$crate")
    # Extract the package version; fall back to the workspace version
    # for crates using `version.workspace = true`.
    version=$(sed -n 's/^version *= *"\(.*\)".*/\1/p' "$crate/Cargo.toml" | head -1)
    if [[ -z "$version" ]]; then
        version=$(sed -n 's/^version *= *"\(.*\)".*/\1/p' Cargo.toml | head -1)
    fi
    echo "=== Publishing $name ==="
    # Skip if this version is already on crates.io (User-Agent is
    # required by the crates.io API).
    if curl -sf -A "code-context-engine-publish-script" -o /dev/null "https://crates.io/api/v1/crates/$name/$version"; then
        echo "Skipping $name v$version: already published."
        echo ""
        continue
    fi
    set +e
    if $DRY_RUN; then
        (cd "$crate" && cargo publish --dry-run --allow-dirty --registry crates-io)
    else
        (cd "$crate" && cargo publish --allow-dirty --registry crates-io)
    fi
    rc=$?
    set -e
    if [[ $rc -ne 0 ]]; then
        # In dry-run mode, unpublished internal dependencies cannot be
        # resolved from the real index yet; warn instead of aborting.
        if $DRY_RUN; then
            echo "WARNING: dry-run failed for $name (likely unpublished internal deps); continuing."
        else
            exit $rc
        fi
    fi
    echo ""
done

echo "All crates published successfully."
