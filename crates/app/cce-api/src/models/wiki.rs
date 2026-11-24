//! Wiki context export models
//!
//! Response shapes for the Wiki Context Package (WCP) export and the
//! incremental invalidation diff. The package carries the deterministic
//! static-analysis side of RepoWiki generation: file inventory, entity
//! catalog, file-level dependency graph, importance metrics, clusters,
//! a reading path, and optional Mermaid skeletons and source snippets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Query parameters of the wiki context export.
#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams)]
pub struct WikiContextQuery {
    /// External dependency presentation mode: `symbol` (default) or `package`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_deps_mode: Option<String>,
    /// Whether entity entries carry short source snippets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_snippets: Option<bool>,
    /// Whether the package carries deterministic Mermaid graph skeletons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_mermaid: Option<bool>,
}

/// One file's content hash as remembered by a previous export.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiFileHash {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// Export manifest: snapshot identity and package-level statistics.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiManifest {
    pub project_id: i64,
    pub root_path: String,
    /// Data epoch of the exported generation.
    pub index_epoch: i64,
    pub relation_epoch: i64,
    /// Epoch of the file inventory and entity catalog (the data generation).
    ///
    /// Type entities supplemented from metadata share this epoch with the
    /// file list, so downstream can join entity → file without a cross-epoch
    /// mismatch.
    pub entity_epoch: i64,
    /// Whether `index_epoch`/`entity_epoch` and `relation_epoch` belong to the
    /// same publication. `false` means the relation snapshot lagged the
    /// active metadata generation and the package spans two generations.
    pub epoch_aligned: bool,
    /// File counts per detected language.
    pub language_stats: BTreeMap<String, usize>,
    pub file_count: usize,
    pub entity_count: usize,
    /// External dependency presentation mode in effect (`symbol`/`package`).
    pub external_deps_mode: String,
    pub include_snippets: bool,
}

/// One file of the project.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiFileEntry {
    pub path: String,
    pub language: String,
    pub category: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Whether the entry-point heuristic marked this file as a reading start.
    pub entry_candidate: bool,
}

/// One entity (symbol) of the project.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiEntityEntry {
    /// Stable symbol id, joinable against the graph and relation tools.
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scoped_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub file_path: String,
    /// One-based line marker, `L12` or `L12-L34`.
    pub source_location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// Short source snippet; present only when `include_snippets` is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// One file-level dependency edge.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiDepEdge {
    pub from_file: String,
    /// Target file inside the project; empty for external targets.
    pub to_file: String,
    /// Raw relation target as written in the source.
    pub target: String,
    /// Relation type string (`dependency.import.standard`, ...).
    pub relation: String,
    /// Coarse relation domain of `relation`.
    pub domain: String,
    pub is_external: bool,
}

/// File-level dependency graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct WikiDepGraph {
    /// Sorted project file paths that participate in at least one edge.
    pub nodes: Vec<String>,
    pub edges: Vec<WikiDepEdge>,
    /// Exported stable symbol ids per file.
    pub exports: BTreeMap<String, Vec<String>>,
    pub truncated: bool,
}

/// One external dependency entry.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiExternalDep {
    /// Symbol name (symbol mode) or package name (package mode).
    pub name: String,
    /// External classification (`standard_library`, `external_library`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_type: Option<String>,
    /// Project files that reference the dependency, sorted.
    pub used_by: Vec<String>,
    /// Relation type strings seen for the dependency, sorted.
    pub relations: Vec<String>,
}

/// External dependency listing in the configured presentation mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct WikiExternalDeps {
    /// Mode in effect: `symbol` or `package`.
    pub mode: String,
    pub entries: Vec<WikiExternalDep>,
    pub truncated: bool,
}

/// File-level importance metrics aggregated from entity graph metrics.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiFileMetrics {
    pub path: String,
    /// Sum of entity PageRank scores inside the file.
    pub pagerank: f64,
    /// Mean entity degree centrality inside the file.
    pub degree_centrality: f64,
    pub entity_count: usize,
}

/// One connected component of the relation graph, projected to files.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiCluster {
    pub files: Vec<String>,
    pub entity_count: usize,
}

/// Connected-component clusters of the relation graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct WikiClusters {
    /// Largest component first.
    pub clusters: Vec<WikiCluster>,
    pub truncated: bool,
}

/// One reading-path recommendation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiReadingEntry {
    pub path: String,
    /// `entry_candidate` or `pagerank_order`.
    pub reason: String,
    pub pagerank: f64,
}

/// Suggested reading order derived from entry candidates and PageRank.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct WikiReadingPath {
    pub entries: Vec<WikiReadingEntry>,
}

/// Deterministic Mermaid skeletons of the dependency graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct WikiMermaid {
    /// File-level dependency graph.
    pub file_graph: String,
    /// Module-level (top directory) dependency graph.
    pub module_graph: String,
}

/// The full Wiki Context Package export response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiContextResponse {
    pub success: bool,
    pub manifest: WikiManifest,
    pub files: Vec<WikiFileEntry>,
    pub entities: Vec<WikiEntityEntry>,
    /// Whether the entity catalog was truncated at the export cap.
    pub entities_truncated: bool,
    pub dep_graph: WikiDepGraph,
    pub external_deps: WikiExternalDeps,
    pub metrics: Vec<WikiFileMetrics>,
    pub clusters: WikiClusters,
    pub reading_path: WikiReadingPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mermaid: Option<WikiMermaid>,
    pub relation_epoch: i64,
    /// Staleness payload when the relation index is degraded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Baseline hashes for an incremental invalidation diff.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct WikiDiffRequest {
    /// File hashes as recorded by a previous export or diff.
    pub baseline: Vec<WikiFileHash>,
}

/// One file-level change classification.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiFileChange {
    pub path: String,
    /// `added`, `changed` or `removed`.
    pub change: String,
}

/// Incremental invalidation result for wiki page regeneration.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WikiDiffResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub changes: Vec<WikiFileChange>,
    /// Changed files plus their one-hop dependency neighbours.
    pub affected_files: Vec<String>,
    /// Module labels of the affected files (wiki module pages).
    pub affected_modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}
