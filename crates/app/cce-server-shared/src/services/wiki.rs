//! Wiki Context Package (WCP) export service served through the HTTP surface.
//!
//! The CLI is a thin HTTP client and does not link this service in-process;
//! any remote caller reaches it over the same HTTP surface.
//!
//! The service assembles the deterministic static-analysis half of RepoWiki
//! generation from one relation snapshot plus the published metadata
//! generation: file inventory, entity catalog, file-level dependency graph,
//! external dependency listing, importance metrics, clusters, a reading
//! path, and optional Mermaid skeletons and source snippets. Narrative
//! generation stays with the downstream caller; nothing here calls an LLM.
//!
//! A companion diff classifies file-hash changes into page-level
//! invalidation lists so downstream wiki pages can be regenerated in
//! cascade instead of in full.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use cce_api::models::{
    WikiClusters, WikiContextResponse, WikiDepEdge, WikiDepGraph, WikiDiffResponse,
    WikiEntityEntry, WikiExternalDep, WikiExternalDeps, WikiFileChange, WikiFileEntry,
    WikiFileHash, WikiFileMetrics, WikiManifest, WikiMermaid, WikiReadingEntry, WikiReadingPath,
};
use cce_codegraph::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};
use cce_codegraph::index::view::RelationIndexView;
use cce_config::{ExternalDepsMode, WikiExportConfig};
use cce_orchestrator::query::GraphService;
use cce_storage_common::MetadataStorage;
use cce_storage_common::metadb::{EntityRecord, EntityWithFile};
use cce_types::{
    EntityKind, ExternalCallType, ResolvedRelation, StableSymbolKey, normalize_project_path,
};
use cce_utils::file::read_file_to_utf8_async;

use crate::context::{CachedGraphMetrics, SharedContext};

use super::relation::{RelationServiceError, relation_access, stable_id};

/// Maximum source lines of one snippet.
pub const MAX_SNIPPET_LINES: usize = 8;
/// Maximum characters of one snippet.
pub const MAX_SNIPPET_CHARS: usize = 512;
/// Maximum entity entries in one package before truncation.
pub const MAX_WCP_ENTITIES: usize = 50_000;
/// Maximum file-level dependency edges before truncation.
pub const MAX_WCP_DEP_EDGES: usize = 20_000;
/// Maximum external dependency entries before truncation.
pub const MAX_WCP_EXTERNAL_DEPS: usize = 2_000;
/// Maximum clusters before truncation.
pub const MAX_WCP_CLUSTERS: usize = 1_000;
/// Maximum edges per Mermaid skeleton.
pub const MAX_MERMAID_EDGES: usize = 300;

/// Export options resolved from configuration and per-request overrides.
#[derive(Debug, Clone, Copy)]
pub struct WikiExportOptions {
    pub external_deps_mode: ExternalDepsMode,
    pub include_snippets: bool,
    pub include_mermaid: bool,
}

/// Resolve export options: request overrides win over configuration.
pub fn resolve_export_options(
    config: &WikiExportConfig,
    external_deps_mode: Option<ExternalDepsMode>,
    include_snippets: Option<bool>,
    include_mermaid: Option<bool>,
) -> WikiExportOptions {
    WikiExportOptions {
        external_deps_mode: external_deps_mode.unwrap_or(config.external_deps_mode),
        include_snippets: include_snippets.unwrap_or(config.include_snippets),
        include_mermaid: include_mermaid.unwrap_or(config.include_mermaid),
    }
}

/// One external reference collected from file-level import relations.
pub struct ExternalUse {
    pub file: String,
    pub symbol: String,
    pub external_type: Option<ExternalCallType>,
    pub relation: String,
}

// ========== Pure helpers ==========

/// Best-effort package name of an external symbol (`serde::Serialize` →
/// `serde`). Deterministic string heuristic for symbols whose external
/// classification carries no package field.
pub fn package_of_symbol(symbol: &str) -> String {
    if let Some((head, _)) = symbol.split_once("::") {
        let head = head.trim();
        if !head.is_empty() {
            return head.to_string();
        }
    }
    if let Some((head, _)) = symbol.split_once('.') {
        let head = head.trim();
        if !head.is_empty() {
            return head.to_string();
        }
    }
    symbol.trim().to_string()
}

/// Whether the entry-point heuristic marks this path as a reading start.
///
/// The heuristic is intentionally shallow: entries are only a reference
/// starting point, the downstream reader keeps final judgement.
pub fn is_entry_candidate(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let name = normalized.rsplit('/').next().unwrap_or(&normalized);
    match name {
        "main.rs" | "main.go" | "main.py" | "main.c" | "main.cpp" | "main.ts" | "main.js"
        | "main.java" | "app.py" | "__main__.py" | "manage.py" | "cli.rs" | "lib.rs"
        | "server.rs" | "server.py" | "index.js" | "index.ts" | "index.jsx" | "index.tsx" => true,
        _ => {
            (normalized.starts_with("cmd/") && normalized.ends_with("/main.go"))
                || normalized.ends_with("/src/main.rs")
                || normalized.ends_with("/src/lib.rs")
        }
    }
}

/// Module label of a file path: its first path segment, or `root` when the
/// file sits at the project root.
pub fn module_of(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    match normalized.split_once('/') {
        Some((head, _)) if !head.is_empty() => head.to_string(),
        _ => "root".to_string(),
    }
}

/// One-based line marker of a span, `L12` or `L12-L34`. Rows are zero-based
/// internally; `usize::MAX` marks an unknown position.
fn location_of(start_row: usize, end_row: usize) -> String {
    if start_row == usize::MAX {
        return String::new();
    }
    let start = start_row + 1;
    if end_row != usize::MAX && end_row > start_row {
        format!("L{start}-L{}", end_row + 1)
    } else {
        format!("L{start}")
    }
}

/// Machine-readable label of an external classification.
fn external_type_label(kind: &ExternalCallType) -> String {
    match kind {
        ExternalCallType::StandardLibrary { .. } => "standard_library".to_string(),
        ExternalCallType::ExternalLibrary { .. } => "external_library".to_string(),
        ExternalCallType::DevDependency { .. } => "dev_dependency".to_string(),
        ExternalCallType::LocalDependency { .. } => "local_dependency".to_string(),
        ExternalCallType::Unknown { .. } => "unknown".to_string(),
    }
}

/// Package name carried by an external classification, when any.
fn external_type_package(kind: &ExternalCallType) -> Option<String> {
    match kind {
        ExternalCallType::StandardLibrary { library } => Some(library.clone()),
        ExternalCallType::ExternalLibrary { package }
        | ExternalCallType::DevDependency { package }
        | ExternalCallType::LocalDependency { package } => Some(package.clone()),
        ExternalCallType::Unknown { .. } => None,
    }
}

/// Aggregate external import uses into the configured presentation mode.
pub fn aggregate_external_deps(
    uses: &[ExternalUse],
    mode: ExternalDepsMode,
    limit: usize,
) -> (Vec<WikiExternalDep>, bool) {
    let mut grouped: BTreeMap<String, WikiExternalDep> = BTreeMap::new();
    for entry in uses {
        let key = match mode {
            ExternalDepsMode::Symbol => entry.symbol.clone(),
            ExternalDepsMode::Package => entry
                .external_type
                .as_ref()
                .and_then(external_type_package)
                .unwrap_or_else(|| package_of_symbol(&entry.symbol)),
        };
        let slot = grouped
            .entry(key.clone())
            .or_insert_with(|| WikiExternalDep {
                name: key,
                external_type: entry.external_type.as_ref().map(external_type_label),
                used_by: Vec::new(),
                relations: Vec::new(),
            });
        slot.used_by.push(entry.file.clone());
        slot.relations.push(entry.relation.clone());
    }
    for dep in grouped.values_mut() {
        dep.used_by.sort();
        dep.used_by.dedup();
        dep.relations.sort();
        dep.relations.dedup();
    }
    let truncated = grouped.len() > limit;
    let entries: Vec<WikiExternalDep> = grouped.into_values().take(limit).collect();
    (entries, truncated)
}

/// Reading order: entry candidates first (by PageRank, then path), then the
/// remaining files by PageRank, then path.
pub fn compute_reading_path(
    files: &[(String, f64)],
    entry_candidates: &BTreeSet<String>,
) -> Vec<WikiReadingEntry> {
    let mut entries: Vec<WikiReadingEntry> = Vec::with_capacity(files.len());
    let mut sorted = files.to_vec();
    sorted.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    for (path, pagerank) in &sorted {
        let reason = if entry_candidates.contains(path) {
            "entry_candidate"
        } else {
            "pagerank_order"
        };
        entries.push(WikiReadingEntry {
            path: path.clone(),
            reason: reason.to_string(),
            pagerank: *pagerank,
        });
    }
    entries.sort_by(|left, right| {
        let left_entry = left.reason == "entry_candidate";
        let right_entry = right.reason == "entry_candidate";
        right_entry
            .cmp(&left_entry)
            .then_with(|| {
                right
                    .pagerank
                    .partial_cmp(&left.pagerank)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.path.cmp(&right.path))
    });
    entries
}

/// Extract a bounded source window for an entity span. Returns `None` when
/// the span is unknown or the window is empty.
pub fn snippet_window(content: &str, start_row: usize, end_row: usize) -> Option<String> {
    if start_row == usize::MAX {
        return None;
    }
    let end = if end_row == usize::MAX || end_row < start_row {
        start_row
    } else {
        end_row
    };
    let mut lines: Vec<&str> = Vec::new();
    for line in content
        .lines()
        .skip(start_row)
        .take((end - start_row + 1).min(MAX_SNIPPET_LINES))
    {
        lines.push(line);
    }
    if lines.is_empty() {
        return None;
    }
    let joined = lines.join("\n");
    if joined.chars().count() > MAX_SNIPPET_CHARS {
        let trimmed: String = joined.chars().take(MAX_SNIPPET_CHARS).collect();
        Some(format!("{trimmed}..."))
    } else {
        Some(joined)
    }
}

/// Deterministic Mermaid skeleton of a file-level dependency graph.
pub fn mermaid_file_graph(nodes: &[String], edges: &[(String, String)]) -> String {
    let mut ids: BTreeMap<&str, String> = BTreeMap::new();
    for (index, path) in nodes.iter().enumerate() {
        ids.insert(path.as_str(), format!("n{index}"));
    }
    let mut out = String::from("graph LR\n");
    let mut emitted = 0usize;
    for (from, to) in edges {
        let (Some(from_id), Some(to_id)) = (ids.get(from.as_str()), ids.get(to.as_str())) else {
            continue;
        };
        if emitted >= MAX_MERMAID_EDGES {
            out.push_str(&format!("%% edges truncated at {MAX_MERMAID_EDGES}\n"));
            break;
        }
        out.push_str(&format!(
            "  {from_id}[\"{}\"] --> {to_id}[\"{}\"]\n",
            from.replace('"', "'"),
            to.replace('"', "'")
        ));
        emitted += 1;
    }
    out
}

/// Deterministic Mermaid skeleton of a module-level dependency graph.
pub fn mermaid_module_graph(edges: &[(String, String)]) -> String {
    let mut modules: BTreeSet<String> = BTreeSet::new();
    let mut module_edges: BTreeSet<(String, String)> = BTreeSet::new();
    for (from, to) in edges {
        let from_module = module_of(from);
        let to_module = module_of(to);
        if from_module == to_module {
            continue;
        }
        modules.insert(from_module.clone());
        modules.insert(to_module.clone());
        module_edges.insert((from_module, to_module));
    }
    let ids: BTreeMap<&str, String> = modules
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), format!("m{index}")))
        .collect();
    let mut out = String::from("graph LR\n");
    for (index, name) in modules.iter().enumerate() {
        out.push_str(&format!("  m{index}[\"{}\"]\n", name.replace('"', "'")));
    }
    let mut emitted = 0usize;
    for (from, to) in &module_edges {
        if emitted >= MAX_MERMAID_EDGES {
            out.push_str(&format!("%% edges truncated at {MAX_MERMAID_EDGES}\n"));
            break;
        }
        let (Some(from_id), Some(to_id)) = (ids.get(from.as_str()), ids.get(to.as_str())) else {
            continue;
        };
        out.push_str(&format!("  {from_id} --> {to_id}\n"));
        emitted += 1;
    }
    out
}

/// Parse a persisted entity-kind label back into `EntityKind`.
///
/// `EntityRecord.kind` stores the `EntityKind` `Display`/serde label
/// (snake_case), so a JSON string round-trip recovers the variant without a
/// bespoke match table. Unknown labels are rejected rather than guessed.
fn parse_entity_kind(raw: &str) -> Option<EntityKind> {
    serde_json::from_value(serde_json::Value::String(raw.to_string())).ok()
}

/// Recompute the stable symbol id of a metadata entity.
///
/// `StableSymbolKey::stable_id` is a pure function of schema version, file
/// path, scoped name, kind and overload discriminator — all persisted on
/// `EntityRecord`. Recomputing it lets a metadata-only entity (e.g. a type
/// definition absent from the relation snapshot) join the same id space as
/// relation-snapshot callable entities.
fn stable_id_of_record(path: &str, record: &EntityRecord) -> Option<String> {
    let kind = parse_entity_kind(&record.kind)?;
    let scoped_name = record
        .scoped_name
        .clone()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| record.name.clone());
    let key = StableSymbolKey {
        file_path: normalize_project_path(path),
        scoped_name,
        kind,
        overload_discriminator: record.overload_discriminator.clone(),
    };
    Some(key.stable_id().0)
}

/// Whether the metadata generation and the relation snapshot came from the
/// same publication. Mirrors the pairing stored on `ProjectIndexManifest`.
pub fn epochs_aligned(relation_epoch: i64, manifest_relation_epoch: i64) -> bool {
    relation_epoch == manifest_relation_epoch
}

/// Classify baseline hashes against the current visible hashes.
pub fn classify_changes(
    baseline: &[WikiFileHash],
    current: &[WikiFileHash],
) -> Vec<WikiFileChange> {
    let baseline_map: HashMap<&str, Option<&str>> = baseline
        .iter()
        .map(|entry| (entry.path.as_str(), entry.content_hash.as_deref()))
        .collect();
    let current_map: HashMap<&str, Option<&str>> = current
        .iter()
        .map(|entry| (entry.path.as_str(), entry.content_hash.as_deref()))
        .collect();
    let mut changes: Vec<WikiFileChange> = Vec::new();
    for (path, hash) in &current_map {
        match baseline_map.get(*path) {
            None => changes.push(WikiFileChange {
                path: (*path).to_string(),
                change: "added".to_string(),
            }),
            Some(previous) => {
                if previous != hash {
                    changes.push(WikiFileChange {
                        path: (*path).to_string(),
                        change: "changed".to_string(),
                    });
                }
            }
        }
    }
    for path in baseline_map.keys() {
        if !current_map.contains_key(*path) {
            changes.push(WikiFileChange {
                path: (*path).to_string(),
                change: "removed".to_string(),
            });
        }
    }
    changes.sort_by(|left, right| left.path.cmp(&right.path));
    changes
}

// ========== Service entry points ==========

/// Assemble the Wiki Context Package of a project from one snapshot.
pub async fn export_context(
    state: &SharedContext,
    project_id: i64,
    external_deps_mode: Option<ExternalDepsMode>,
    include_snippets: Option<bool>,
    include_mermaid: Option<bool>,
) -> Result<WikiContextResponse, RelationServiceError> {
    if project_id <= 0 {
        return Err(RelationServiceError::InvalidInput(
            "project_id must be positive".to_string(),
        ));
    }
    let access = relation_access(state, project_id).await?;
    let config = wiki_config(state, project_id).await?;
    let options = resolve_export_options(
        &config,
        external_deps_mode,
        include_snippets,
        include_mermaid,
    );

    let store = state
        .engine
        .metadata_store()
        .ok_or_else(|| RelationServiceError::Execution("metadata store unavailable".to_string()))?;
    let manifest = store
        .manifest_active(project_id)
        .await
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?
        .ok_or_else(|| RelationServiceError::Execution("no active index manifest".to_string()))?;
    let file_rows = store
        .files_list(project_id, &[manifest.data_epoch])
        .await
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?;

    // G: anchor every metadata-side section (files + entity catalog) to one
    // data epoch, and record whether it pairs with the served relation
    // snapshot. A mismatch means the package spans two generations.
    let entity_epoch = manifest.data_epoch;
    let aligned = epochs_aligned(access.snapshot.relation_epoch, manifest.relation_epoch);

    let root_path = project_root(state, project_id).await?;

    // Entity catalog and per-file PageRank inputs come from the relation
    // snapshot, which is the same source the graph tools serve.
    let index = access.snapshot.index.as_ref();
    let mut entities: Vec<WikiEntityEntry> = Vec::new();
    // Stable ids already present in the catalog; used to de-duplicate the
    // metadata-supplied type entities against relation-snapshot callables.
    let mut seen_entity_ids: BTreeSet<String> = BTreeSet::new();
    // Entity base fields are collected synchronously from the snapshot; source
    // snippets are filled in a second, async pass so export never blocks a
    // tokio worker on disk IO.
    let mut snippet_targets: Vec<(usize, String, usize, usize)> = Vec::new();
    let mut entities_truncated = false;
    index.for_each_function(|id, entity| {
        if entities.len() >= MAX_WCP_ENTITIES {
            entities_truncated = true;
            return;
        }
        let key = index.get_symbol_key_by_entity_id(id);
        let stable = key
            .as_ref()
            .map(|key| key.stable_id().0)
            .unwrap_or_else(|| format!("entity:{}", id.0));
        let file_path = index.get_file_path_by_entity(id).unwrap_or_default();
        let signature = (!entity.signature.is_empty()).then(|| entity.signature.clone());
        let start_row = entity.span.start_position.row;
        let end_row = entity.span.end_position.row;
        if options.include_snippets && start_row != usize::MAX {
            snippet_targets.push((entities.len(), file_path.clone(), start_row, end_row));
        }
        seen_entity_ids.insert(stable.clone());
        entities.push(WikiEntityEntry {
            id: stable,
            name: entity.name.clone(),
            kind: entity.kind.to_string(),
            scoped_name: Some(
                key.as_ref()
                    .map(|key| key.scoped_name.clone())
                    .unwrap_or_else(|| entity.name.clone()),
            ),
            signature,
            file_path,
            source_location: location_of(start_row, end_row),
            doc_comment: entity.doc_comment.clone(),
            snippet: None,
        });
    });

    // F: the relation snapshot stores only callable entities. Supplement the
    // catalog with type definitions (class/struct/enum/trait/...) from the
    // metadata generation, which is the same epoch as `file_rows`. Their
    // stable ids are recomputed so downstream joins stay consistent, and
    // `seen_entity_ids` prevents duplicates when a type is also callable.
    if !entities_truncated {
        let meta_entities = store
            .entities_with_files(project_id, entity_epoch)
            .await
            .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
        for EntityWithFile {
            entity: record,
            file_path,
        } in meta_entities
        {
            if entities.len() >= MAX_WCP_ENTITIES {
                entities_truncated = true;
                break;
            }
            let Some(kind) = parse_entity_kind(&record.kind) else {
                continue;
            };
            if !kind.is_type_definition() {
                continue;
            }
            let Some(stable) = stable_id_of_record(&file_path, &record) else {
                continue;
            };
            if !seen_entity_ids.insert(stable.clone()) {
                continue;
            }
            let start_row = record
                .span_start_row
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(usize::MAX);
            let end_row = record
                .span_end_row
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(usize::MAX);
            if options.include_snippets && start_row != usize::MAX {
                snippet_targets.push((entities.len(), file_path.clone(), start_row, end_row));
            }
            let scoped_name = record
                .scoped_name
                .clone()
                .filter(|name| !name.is_empty())
                .or_else(|| Some(record.name.clone()));
            let signature = record.signature.clone().filter(|value| !value.is_empty());
            entities.push(WikiEntityEntry {
                id: stable,
                name: record.name.clone(),
                kind: record.kind.clone(),
                scoped_name,
                signature,
                file_path,
                source_location: location_of(start_row, end_row),
                doc_comment: record.doc_comment.clone(),
                snippet: None,
            });
        }
    }
    if options.include_snippets {
        let mut snippet_cache: HashMap<String, Option<String>> = HashMap::new();
        for (entity_index, file_path, start_row, end_row) in snippet_targets {
            let content = if let Some(cached) = snippet_cache.get(&file_path) {
                cached.clone()
            } else {
                let loaded = read_project_file(&root_path, &file_path).await;
                snippet_cache.insert(file_path.clone(), loaded.clone());
                loaded
            };
            if let Some(content) = content {
                entities[entity_index].snippet = snippet_window(&content, start_row, end_row);
            }
        }
    }
    entities.sort_by(|left, right| {
        left.file_path
            .cmp(&right.file_path)
            .then_with(|| left.id.cmp(&right.id))
    });

    // File-level dependency graph from module relations.
    let mut dep_edges: Vec<WikiDepEdge> = Vec::new();
    let mut dep_nodes: BTreeSet<String> = BTreeSet::new();
    let mut exports: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut external_uses: Vec<ExternalUse> = Vec::new();
    let mut seen_edges: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    let mut dep_truncated = false;
    for row in &file_rows {
        let module = access.searcher.get_module_relations(&row.path);
        let mut file_exports: Vec<String> = module
            .exports
            .iter()
            .filter_map(|id| stable_id(index, *id))
            .collect();
        file_exports.sort();
        if !file_exports.is_empty() {
            exports.insert(row.path.clone(), file_exports);
        }
        for relation in &module.imports {
            let mut buffers = ImportEdgeBuffers {
                seen_edges: &mut seen_edges,
                dep_edges: &mut dep_edges,
                dep_nodes: &mut dep_nodes,
                external_uses: &mut external_uses,
            };
            push_import_edge(&mut buffers, &row.path, relation, index);
            if dep_edges.len() >= MAX_WCP_DEP_EDGES {
                dep_truncated = true;
                break;
            }
        }
        for caller_file in &module.callers {
            if caller_file == &row.path {
                continue;
            }
            let key = (
                caller_file.clone(),
                row.path.clone(),
                row.path.clone(),
                "module.caller".to_string(),
            );
            if seen_edges.insert(key) {
                dep_nodes.insert(caller_file.clone());
                dep_nodes.insert(row.path.clone());
                dep_edges.push(WikiDepEdge {
                    from_file: caller_file.clone(),
                    to_file: row.path.clone(),
                    target: row.path.clone(),
                    relation: "module.caller".to_string(),
                    domain: "call".to_string(),
                    is_external: false,
                });
                if dep_edges.len() >= MAX_WCP_DEP_EDGES {
                    dep_truncated = true;
                    break;
                }
            }
        }
    }

    let (external_entries, external_truncated) = aggregate_external_deps(
        &external_uses,
        options.external_deps_mode,
        MAX_WCP_EXTERNAL_DEPS,
    );

    // H: reuse graph-derived metrics across exports of the same snapshot.
    // The cache key includes the relation epoch, so a newer snapshot computes
    // fresh results without explicit invalidation.
    let graph = GraphService::new(access.searcher.clone());
    let cache_key = (project_id, access.snapshot.relation_epoch);
    let cached = if let Some(hit) = state.wiki_graph_cache.get(&cache_key).await {
        hit
    } else {
        let entity_metrics = graph.compute_metrics();
        let components = graph
            .connected_components()
            .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
        let value = Arc::new(CachedGraphMetrics {
            entity_metrics,
            components,
        });
        state
            .wiki_graph_cache
            .insert(cache_key, value.clone())
            .await;
        value
    };
    let entity_metrics = &cached.entity_metrics;

    // Importance metrics aggregated per file from entity graph metrics.
    let mut per_file: HashMap<String, (f64, f64, usize)> = HashMap::new();
    for (id, metrics) in entity_metrics {
        if let Some(file) = index.get_file_path_by_entity(*id) {
            let slot = per_file.entry(file).or_insert((0.0, 0.0, 0));
            slot.0 += metrics.pagerank;
            slot.1 += metrics.degree_centrality;
            slot.2 += 1;
        }
    }
    let mut file_metrics: Vec<WikiFileMetrics> = per_file
        .iter()
        .map(|(path, (pagerank, degree, count))| WikiFileMetrics {
            path: path.clone(),
            pagerank: *pagerank,
            degree_centrality: if *count > 0 {
                degree / (*count as f64)
            } else {
                0.0
            },
            entity_count: *count,
        })
        .collect();
    file_metrics.sort_by(|left, right| left.path.cmp(&right.path));

    // Clusters: connected components projected to files, largest first.
    let components = &cached.components;
    let clusters_truncated = components.len() > MAX_WCP_CLUSTERS;
    let clusters: Vec<_> = components
        .iter()
        .take(MAX_WCP_CLUSTERS)
        .map(|component| {
            let mut files: BTreeSet<String> = BTreeSet::new();
            for id in component {
                if let Some(file) = index.get_file_path_by_entity(*id) {
                    files.insert(file);
                }
            }
            WikiClusterFiles {
                files: files.into_iter().collect(),
                entity_count: component.len(),
            }
        })
        .collect();

    // Reading path over every project file, with PageRank from the metrics.
    let entry_candidates: BTreeSet<String> = file_rows
        .iter()
        .filter(|row| is_entry_candidate(&row.path))
        .map(|row| row.path.clone())
        .collect();
    let pagerank_by_path: HashMap<String, f64> = file_metrics
        .iter()
        .map(|entry| (entry.path.clone(), entry.pagerank))
        .collect();
    let reading_files: Vec<(String, f64)> = file_rows
        .iter()
        .map(|row| {
            let rank = pagerank_by_path.get(&row.path).copied().unwrap_or(0.0);
            (row.path.clone(), rank)
        })
        .collect();
    let reading_path = WikiReadingPath {
        entries: compute_reading_path(&reading_files, &entry_candidates),
    };

    // Language statistics and file inventory.
    let mut language_stats: BTreeMap<String, usize> = BTreeMap::new();
    let mut files: Vec<WikiFileEntry> = Vec::with_capacity(file_rows.len());
    for row in &file_rows {
        *language_stats.entry(row.language.clone()).or_insert(0usize) += 1;
        files.push(WikiFileEntry {
            path: row.path.clone(),
            language: row.language.clone(),
            category: row.category,
            content_hash: row.content_hash.clone(),
            entry_candidate: entry_candidates.contains(&row.path),
        });
    }

    let mermaid = if options.include_mermaid {
        let file_pairs: Vec<(String, String)> = dep_edges
            .iter()
            .filter(|edge| !edge.is_external && !edge.to_file.is_empty())
            .map(|edge| (edge.from_file.clone(), edge.to_file.clone()))
            .collect();
        Some(WikiMermaid {
            file_graph: mermaid_file_graph(
                &dep_nodes.iter().cloned().collect::<Vec<_>>(),
                &file_pairs,
            ),
            module_graph: mermaid_module_graph(&file_pairs),
        })
    } else {
        None
    };

    let stamp = access.stamp().await;
    Ok(WikiContextResponse {
        success: true,
        manifest: WikiManifest {
            project_id,
            root_path,
            index_epoch: manifest.data_epoch,
            relation_epoch: stamp.relation_epoch,
            entity_epoch,
            epoch_aligned: aligned,
            language_stats,
            file_count: files.len(),
            entity_count: entities.len(),
            external_deps_mode: options.external_deps_mode.as_str().to_string(),
            include_snippets: options.include_snippets,
        },
        files,
        entities,
        entities_truncated,
        dep_graph: WikiDepGraph {
            nodes: dep_nodes.into_iter().collect(),
            edges: dep_edges,
            exports,
            truncated: dep_truncated,
        },
        external_deps: WikiExternalDeps {
            mode: options.external_deps_mode.as_str().to_string(),
            entries: external_entries,
            truncated: external_truncated,
        },
        metrics: file_metrics,
        clusters: WikiClusters {
            clusters: clusters
                .into_iter()
                .map(|cluster| cce_api::models::WikiCluster {
                    files: cluster.files,
                    entity_count: cluster.entity_count,
                })
                .collect(),
            truncated: clusters_truncated,
        },
        reading_path,
        mermaid,
        relation_epoch: stamp.relation_epoch,
        relation_info: stamp.stale,
    })
}

/// Classify file-hash changes into page-level invalidation lists.
///
/// Affected files are the changed set plus their one-hop dependency
/// neighbours (imports and importers); affected modules are the module
/// labels of those files.
pub async fn diff(
    state: &SharedContext,
    project_id: i64,
    baseline: &[cce_api::models::WikiFileHash],
) -> Result<WikiDiffResponse, RelationServiceError> {
    if project_id <= 0 {
        return Err(RelationServiceError::InvalidInput(
            "project_id must be positive".to_string(),
        ));
    }
    let access = relation_access(state, project_id).await?;
    let store = state
        .engine
        .metadata_store()
        .ok_or_else(|| RelationServiceError::Execution("metadata store unavailable".to_string()))?;
    let visible = store
        .files_visible_hashes(project_id)
        .await
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
    let current: Vec<cce_api::models::WikiFileHash> = visible
        .into_iter()
        .map(|entry| WikiFileHash {
            path: entry.path,
            content_hash: entry.content_hash,
        })
        .collect();
    let changes = classify_changes(baseline, &current);

    let index = access.snapshot.index.as_ref();
    let mut affected: BTreeSet<String> = BTreeSet::new();
    for change in &changes {
        affected.insert(change.path.clone());
        for neighbour in index.dependencies_of(&change.path) {
            affected.insert(neighbour);
        }
        for neighbour in index.dependents_of(&change.path) {
            affected.insert(neighbour);
        }
    }
    let affected_modules: BTreeSet<String> = affected.iter().map(|path| module_of(path)).collect();

    let stamp = access.stamp().await;
    Ok(WikiDiffResponse {
        success: true,
        relation_epoch: stamp.relation_epoch,
        changes,
        affected_files: affected.into_iter().collect(),
        affected_modules: affected_modules.into_iter().collect(),
        relation_info: stamp.stale,
    })
}

// ========== Internals ==========

/// Local alias used while building clusters before response mapping.
struct WikiClusterFiles {
    files: Vec<String>,
    entity_count: usize,
}

/// Load the effective wiki export configuration of a project.
async fn wiki_config(
    state: &SharedContext,
    project_id: i64,
) -> Result<WikiExportConfig, RelationServiceError> {
    state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
        .map(|entry| entry.config.wiki.clone())
        .map_err(|error| RelationServiceError::Project(error.to_string()))
}

/// Registered root path of a project.
async fn project_root(
    state: &SharedContext,
    project_id: i64,
) -> Result<String, RelationServiceError> {
    state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
        .map(|entry| entry.metadata.root_path.clone())
        .map_err(|error| RelationServiceError::Project(error.to_string()))
}

/// Read one project source file for snippet extraction; `None` on any IO
/// failure so export never breaks on a missing or unreadable file.
async fn read_project_file(root: &str, relative: &str) -> Option<String> {
    let path = if Path::new(relative).is_absolute() {
        Path::new(relative).to_path_buf()
    } else {
        Path::new(root).join(relative)
    };
    read_file_to_utf8_async(&path).await.ok()
}

/// Mutable output buffers shared while accumulating file-level dependency
/// edges from module import relations.
struct ImportEdgeBuffers<'a> {
    seen_edges: &'a mut BTreeSet<(String, String, String, String)>,
    dep_edges: &'a mut Vec<WikiDepEdge>,
    dep_nodes: &'a mut BTreeSet<String>,
    external_uses: &'a mut Vec<ExternalUse>,
}

/// Record one import relation as a file-level edge and an external use.
fn push_import_edge(
    buffers: &mut ImportEdgeBuffers<'_>,
    from_file: &str,
    relation: &ResolvedRelation,
    index: &impl SnapshotEntityQueryOps,
) {
    let relation_name = relation.relation_type.to_string();
    let domain = relation.relation_type.domain().to_string();
    let to_file = if relation.is_external {
        String::new()
    } else {
        relation
            .callee_id
            .and_then(|id| index.get_file_path_by_entity(id))
            .unwrap_or_default()
    };
    if relation.is_external || to_file.is_empty() {
        buffers.external_uses.push(ExternalUse {
            file: from_file.to_string(),
            symbol: relation.callee_name.clone(),
            external_type: relation.external_type.clone(),
            relation: relation_name.clone(),
        });
    }
    if to_file == from_file {
        return;
    }
    let key = (
        from_file.to_string(),
        to_file.clone(),
        relation.callee_name.clone(),
        relation_name.clone(),
    );
    if buffers.seen_edges.insert(key) {
        buffers.dep_nodes.insert(from_file.to_string());
        if !to_file.is_empty() {
            buffers.dep_nodes.insert(to_file.clone());
        }
        buffers.dep_edges.push(WikiDepEdge {
            from_file: from_file.to_string(),
            to_file,
            target: relation.callee_name.clone(),
            relation: relation_name,
            domain,
            is_external: relation.is_external,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_record(kind: &str, scoped: &str) -> EntityRecord {
        serde_json::from_value(serde_json::json!({
            "id": 1,
            "name": scoped,
            "kind": kind,
            "file_id": 1,
            "signature": None::<String>,
            "span_start_row": 4_i64,
            "span_end_row": 9_i64,
            "span_start_column": 0_i64,
            "span_end_column": 0_i64,
            "span_start_byte": 0_i64,
            "span_end_byte": 0_i64,
            "scoped_name": scoped,
            "overload_discriminator": "",
            "depth": 0_i64,
            "parent_id": None::<i64>,
            "metadata": None::<String>,
            "parameters_json": None::<String>,
            "return_type": None::<String>,
            "doc_comment": None::<String>,
            "modifiers_json": None::<String>,
            "project_id": 1,
            "epoch": 1,
            "batch_id": 1,
            "rank": 0.0_f32
        }))
        .expect("entity record deserializes")
    }

    #[test]
    fn parse_entity_kind_round_trips_type_definitions() {
        assert_eq!(parse_entity_kind("struct"), Some(EntityKind::Struct));
        assert_eq!(parse_entity_kind("enum"), Some(EntityKind::Enum));
        assert_eq!(parse_entity_kind("trait"), Some(EntityKind::Trait));
        assert_eq!(parse_entity_kind("not_a_kind"), None);
    }

    #[test]
    fn stable_id_of_record_is_deterministic_and_kind_sensitive() {
        let record = type_record("struct", "crate::Config");
        let a = stable_id_of_record("src/config.rs", &record);
        let b = stable_id_of_record("src/config.rs", &record);
        assert_eq!(a, b);
        assert!(a.as_ref().expect("id exists").starts_with("sym_"));
        let as_enum = type_record("enum", "crate::Config");
        assert_ne!(a, stable_id_of_record("src/config.rs", &as_enum));
        let other_path = stable_id_of_record("src/other.rs", &record);
        assert_ne!(a, other_path);
    }

    #[test]
    fn metadata_stable_id_matches_snapshot_key_for_same_identity() {
        // Contract: a metadata entity and a relation-snapshot entity with the
        // same logical identity must produce the same stable id, so a type
        // entity supplemented from metadata joins the graph id space.
        let path = "src/config.rs";
        let key = StableSymbolKey {
            file_path: normalize_project_path(path),
            scoped_name: "crate::Config".to_string(),
            kind: EntityKind::Struct,
            overload_discriminator: String::new(),
        };
        let record = type_record("struct", "crate::Config");
        assert_eq!(stable_id_of_record(path, &record), Some(key.stable_id().0));
    }

    #[test]
    fn epochs_aligned_detects_cross_generation_packages() {
        assert!(epochs_aligned(7, 7));
        assert!(!epochs_aligned(6, 7));
    }

    #[test]
    fn export_options_use_config_defaults_and_request_overrides() {
        let config = WikiExportConfig {
            external_deps_mode: ExternalDepsMode::Package,
            include_snippets: true,
            include_mermaid: false,
        };
        let resolved = resolve_export_options(&config, None, None, None);
        assert_eq!(resolved.external_deps_mode, ExternalDepsMode::Package);
        assert!(resolved.include_snippets);
        assert!(!resolved.include_mermaid);

        let overridden = resolve_export_options(
            &config,
            Some(ExternalDepsMode::Symbol),
            Some(false),
            Some(true),
        );
        assert_eq!(overridden.external_deps_mode, ExternalDepsMode::Symbol);
        assert!(!overridden.include_snippets);
        assert!(overridden.include_mermaid);
    }

    #[test]
    fn default_export_config_keeps_mermaid_enabled() {
        let config = WikiExportConfig::default();
        assert_eq!(config.external_deps_mode, ExternalDepsMode::Symbol);
        assert!(!config.include_snippets);
        assert!(config.include_mermaid);
    }

    #[test]
    fn package_of_symbol_prefers_path_segments() {
        assert_eq!(package_of_symbol("serde::Serialize"), "serde");
        assert_eq!(package_of_symbol("lodash.map"), "lodash");
        assert_eq!(package_of_symbol("react"), "react");
        assert_eq!(package_of_symbol(""), "");
    }

    #[test]
    fn entry_candidate_covers_well_known_names() {
        assert!(is_entry_candidate("src/main.rs"));
        assert!(is_entry_candidate("cmd/server/main.go"));
        assert!(is_entry_candidate("app.py"));
        assert!(!is_entry_candidate("src/utils/helpers.rs"));
    }

    #[test]
    fn module_of_uses_first_segment() {
        assert_eq!(module_of("src/lib.rs"), "src");
        assert_eq!(module_of("README.md"), "root");
        assert_eq!(module_of("a/b/c.rs"), "a");
    }

    #[test]
    fn location_of_marks_ranges_and_unknowns() {
        assert_eq!(location_of(usize::MAX, 0), "");
        assert_eq!(location_of(4, 4), "L5");
        assert_eq!(location_of(4, 9), "L5-L10");
    }

    #[test]
    fn external_deps_switch_presentation_mode() {
        let uses = vec![
            ExternalUse {
                file: "a.rs".to_string(),
                symbol: "serde::Serialize".to_string(),
                external_type: Some(ExternalCallType::external_library("serde")),
                relation: "dependency.import.named".to_string(),
            },
            ExternalUse {
                file: "b.rs".to_string(),
                symbol: "serde::Deserialize".to_string(),
                external_type: Some(ExternalCallType::external_library("serde")),
                relation: "dependency.import.named".to_string(),
            },
        ];
        let (symbol_mode, truncated) = aggregate_external_deps(&uses, ExternalDepsMode::Symbol, 10);
        assert!(!truncated);
        assert_eq!(symbol_mode.len(), 2);
        let (package_mode, _) = aggregate_external_deps(&uses, ExternalDepsMode::Package, 10);
        assert_eq!(package_mode.len(), 1);
        assert_eq!(package_mode[0].name, "serde");
        assert_eq!(package_mode[0].used_by, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn reading_path_puts_entry_candidates_first() {
        let files = vec![
            ("src/lib.rs".to_string(), 0.5),
            ("src/main.rs".to_string(), 0.1),
            ("src/util.rs".to_string(), 0.9),
        ];
        let entries: BTreeSet<String> = ["src/main.rs".to_string(), "src/lib.rs".to_string()]
            .into_iter()
            .collect();
        let path = compute_reading_path(&files, &entries);
        assert_eq!(path[0].path, "src/lib.rs");
        assert_eq!(path[0].reason, "entry_candidate");
        assert_eq!(path[1].path, "src/main.rs");
        assert_eq!(path[2].path, "src/util.rs");
        assert_eq!(path[2].reason, "pagerank_order");
    }

    #[test]
    fn snippet_window_bounds_lines_and_chars() {
        let content = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj";
        let window = snippet_window(content, 1, 5).expect("window exists");
        assert_eq!(window, "b\nc\nd\ne\nf");
        assert!(snippet_window(content, usize::MAX, 1).is_none());
        let long = "x".repeat(MAX_SNIPPET_CHARS + 50);
        let trimmed = snippet_window(&long, 0, 0).expect("window exists");
        assert!(trimmed.ends_with("..."));
    }

    #[test]
    fn classify_changes_detects_add_remove_change() {
        let baseline = vec![
            WikiFileHash {
                path: "a.rs".to_string(),
                content_hash: Some("1".to_string()),
            },
            WikiFileHash {
                path: "b.rs".to_string(),
                content_hash: Some("2".to_string()),
            },
        ];
        let current = vec![
            WikiFileHash {
                path: "a.rs".to_string(),
                content_hash: Some("1".to_string()),
            },
            WikiFileHash {
                path: "c.rs".to_string(),
                content_hash: Some("3".to_string()),
            },
        ];
        let changes = classify_changes(&baseline, &current);
        let as_map: HashMap<&str, &str> = changes
            .iter()
            .map(|change| (change.path.as_str(), change.change.as_str()))
            .collect();
        assert_eq!(as_map.get("b.rs"), Some(&"removed"));
        assert_eq!(as_map.get("c.rs"), Some(&"added"));
        assert_eq!(as_map.get("a.rs"), None);
    }

    #[test]
    fn mermaid_graphs_are_deterministic() {
        let nodes = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        let edges = vec![("src/a.rs".to_string(), "src/b.rs".to_string())];
        let file_graph = mermaid_file_graph(&nodes, &edges);
        assert!(file_graph.contains("graph LR"));
        assert!(file_graph.contains("\"src/a.rs\"] --> n1"));
        let module_graph = mermaid_module_graph(&edges);
        assert!(module_graph.contains("graph LR"));
    }
}
