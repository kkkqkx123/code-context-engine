//! Service-layer graph query parameters shared by local and remote execution.

/// Domain operations covering project, index, retrieval, graph, and watch.
///
/// Parameters reuse the wire models so local and remote share one semantic
/// definition. Implementations differ only in transport: local calls the
/// engine directly, remote encodes the same models onto HTTP paths.
///
/// Graph parameters mirror the HTTP query surface field for field so the two
/// modes cannot drift. Comma-separated filters stay strings on both sides.
#[derive(Debug, Clone)]
pub struct GraphEgoParams {
    /// Symbol seed: stable id, file-scoped name, or bare name.
    pub entity_id: String,
    /// Traversal depth before project depth capping.
    pub depth: usize,
    /// Direction: in, out, or both.
    pub direction: String,
    /// Pagination offset over nodes.
    pub offset: usize,
    /// Pagination limit over nodes.
    pub limit: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep edges pointing outside the project.
    pub include_external: bool,
}

/// Two-point path parameters.
#[derive(Debug, Clone)]
pub struct GraphPathParams {
    /// Start symbol seed.
    pub start: String,
    /// End symbol seed.
    pub end: String,
    /// Maximum search depth before project depth capping.
    pub max_depth: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep external edges.
    pub include_external: bool,
}

/// Explicit subgraph parameters.
#[derive(Debug, Clone)]
pub struct GraphSubgraphParams {
    /// Symbol seeds in encounter order.
    pub ids: Vec<String>,
    /// Pagination offset over nodes.
    pub offset: usize,
    /// Pagination limit over nodes.
    pub limit: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep external edges.
    pub include_external: bool,
}

/// Connected components parameters.
#[derive(Debug, Clone)]
pub struct GraphComponentsParams {
    /// Pagination offset over components.
    pub offset: usize,
    /// Maximum components to report.
    pub limit: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep external edges.
    pub include_external: bool,
}

/// Full export parameters.
#[derive(Debug, Clone)]
pub struct GraphExportParams {
    /// Maximum nodes to materialize.
    pub limit: usize,
    /// Pagination offset over nodes.
    pub offset: usize,
    /// Comma-separated relation domains.
    pub domains: String,
    /// Comma-separated relation types.
    pub relation_types: String,
    /// Whether to keep external edges.
    pub include_external: bool,
}

/// File impact parameters.
#[derive(Debug, Clone)]
pub struct GraphImpactParams {
    /// Changed file path.
    pub file: String,
}

/// Entity impact parameters.
#[derive(Debug, Clone)]
pub struct GraphEntityImpactParams {
    /// Changed symbol seed.
    pub entity_id: String,
    /// Maximum dependent depth.
    pub max_depth: usize,
    /// Impact scope: entity or file.
    pub scope: String,
}

/// Dependency cycle parameters.
#[derive(Debug, Clone)]
pub struct GraphCyclesParams {
    /// Cycle level: entity or file.
    pub level: String,
    /// Maximum cycles to report.
    pub limit: usize,
}

/// Structural relation parameters.
#[derive(Debug, Clone)]
pub struct GraphStructuralParams {
    /// Symbol seed.
    pub entity_id: String,
    /// Relation family label.
    pub kind: String,
    /// Direction: out or in.
    pub direction: String,
    /// Maximum relations.
    pub limit: usize,
}

/// Module relation parameters.
#[derive(Debug, Clone)]
pub struct GraphModuleParams {
    /// File path.
    pub file: String,
}
