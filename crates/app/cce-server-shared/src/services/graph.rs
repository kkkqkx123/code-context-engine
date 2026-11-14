//! Graph query service shared by the HTTP and MCP surfaces.
//!
//! The service reuses the relation searcher's graph layer for traversal and
//! materialization, then describes every result with stable ids so transports
//! never touch the runtime entity numbering. Input validation that guards
//! project-wide scans lives here as well, keeping both surfaces on the same
//! bounds.

use cce_codegraph::index::SnapshotEntityQueryOps;
use cce_orchestrator::query::{
    GraphDirection, GraphEdge, GraphFilter, GraphNode, GraphPagination, GraphService,
    StructuralDirection, StructuralKind,
};

use crate::context::SharedContext;

use super::relation::{
    RelationServiceError, RelationStamp, entity_ref, relation_access, relation_config,
    resolve_seed_in, stable_id,
};

/// Maximum number of seeds accepted by an explicit subgraph request.
pub const MAX_SUBGRAPH_IDS: usize = 200;
/// Maximum component page size.
pub const MAX_COMPONENTS: usize = 5_000;
/// Maximum node budget of a full export.
pub const MAX_EXPORT_NODES: usize = 10_000;
/// Maximum entity-impact traversal depth.
pub const MAX_ENTITY_IMPACT_DEPTH: usize = 50;
/// Maximum cycle page size.
pub const MAX_CYCLES: usize = 5_000;
/// Maximum structural relation page size.
pub const MAX_STRUCTURAL: usize = 2_000;
/// Maximum module edges reported per file relation.
pub const MAX_MODULE_EDGES: usize = 5_000;

/// Relation filter described without transport types.
#[derive(Debug, Clone, Default)]
pub struct GraphFilterSpec {
    /// Keep only relations in these coarse domains. Empty means no filtering.
    pub domains: Vec<String>,
    /// Keep only relations whose exact type is in this set.
    pub relation_types: Vec<String>,
    /// Whether to keep edges pointing outside the indexed project.
    pub include_external: bool,
}

impl GraphFilterSpec {
    fn to_filter(&self) -> GraphFilter {
        GraphFilter {
            relation_domains: self.domains.clone(),
            relation_types: self.relation_types.clone(),
            include_external: self.include_external,
        }
    }
}

/// Node pagination described without transport types.
#[derive(Debug, Clone, Copy)]
pub struct GraphPageSpec {
    pub offset: usize,
    pub limit: usize,
}

impl GraphPageSpec {
    fn to_pagination(self) -> GraphPagination {
        GraphPagination {
            offset: self.offset,
            limit: self.limit,
        }
    }
}

/// Traversal direction accepted on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphDirectionSpec {
    Forward,
    Backward,
    Both,
}

impl GraphDirectionSpec {
    /// Parse a direction label, case-insensitively.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "in" | "backward" | "up" => Some(Self::Backward),
            "out" | "forward" | "down" => Some(Self::Forward),
            "both" | "bidirectional" => Some(Self::Both),
            _ => None,
        }
    }

    fn into_direction(self) -> GraphDirection {
        match self {
            Self::Forward => GraphDirection::Forward,
            Self::Backward => GraphDirection::Backward,
            Self::Both => GraphDirection::Both,
        }
    }
}

/// Scope of an entity-impact analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactScope {
    /// Follow entity-level relation edges.
    Entity,
    /// Follow the file-level dependency graph.
    File,
}

impl ImpactScope {
    /// Parse an impact scope label, case-insensitively.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "entity" => Some(Self::Entity),
            "file" => Some(Self::File),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::File => "file",
        }
    }
}

/// Materialized graph page with totals before pagination.
#[derive(Debug, Clone)]
pub struct GraphView {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub stamp: RelationStamp,
}

impl GraphView {
    /// Whether pagination dropped any node or edge.
    pub fn truncated(&self) -> bool {
        self.total_nodes > self.nodes.len() || self.total_edges > self.edges.len()
    }
}

/// A shortest path answer; a missing path is not an error.
#[derive(Debug, Clone)]
pub struct GraphPath {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub found: bool,
    pub stamp: RelationStamp,
}

/// Connected components described by stable symbol ids.
#[derive(Debug, Clone)]
pub struct ComponentGroups {
    pub components: Vec<Vec<String>>,
    pub total_components: usize,
    pub stamp: RelationStamp,
}

/// Dependency cycles described by stable symbol ids or project paths.
#[derive(Debug, Clone)]
pub struct CycleReport {
    pub cycles: Vec<Vec<String>>,
    pub total: usize,
    pub truncated: bool,
    pub stamp: RelationStamp,
}

/// Impact of a file or entity change.
#[derive(Debug, Clone)]
pub struct ImpactView {
    pub changed: String,
    pub direct_dependents: Vec<String>,
    pub indirect_dependents: Vec<String>,
    pub impact_score: f64,
    pub stamp: RelationStamp,
}

/// One structural relation endpoint.
#[derive(Debug, Clone)]
pub struct StructuralEdgeView {
    pub entity_id: String,
    pub label: String,
    pub relation: String,
    pub domain: String,
    pub source_file: String,
}

/// Structural relations of one entity with the pre-pagination total.
#[derive(Debug, Clone)]
pub struct StructuralView {
    pub relations: Vec<StructuralEdgeView>,
    pub total_relations: usize,
    pub truncated: bool,
    pub stamp: RelationStamp,
}

/// One module-level import relation.
#[derive(Debug, Clone)]
pub struct ModuleEdgeView {
    pub entity_id: String,
    pub target: String,
    pub relation: String,
    pub domain: String,
}

/// Module relations of one file.
#[derive(Debug, Clone)]
pub struct ModuleView {
    pub file: String,
    pub imports: Vec<ModuleEdgeView>,
    pub exports: Vec<String>,
    pub caller_files: Vec<String>,
    pub stamp: RelationStamp,
}

/// Ego neighborhood of one entity.
pub async fn ego(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
    depth: usize,
    direction: GraphDirectionSpec,
    filter: &GraphFilterSpec,
    page: GraphPageSpec,
) -> Result<GraphView, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let max_depth = relation_config(state, project_id).await?.max_call_depth;
    let entity_id = resolve_seed_in(&access.snapshot, seed)?;
    let stamp = access.stamp().await;
    let paged = GraphService::new(access.searcher)
        .ego_graph_with_options(
            entity_id,
            depth.min(max_depth),
            direction.into_direction(),
            &filter.to_filter(),
            page.to_pagination(),
        )
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
    Ok(GraphView {
        total_nodes: paged.total_nodes,
        total_edges: paged.total_edges,
        nodes: paged.nodes,
        edges: paged.edges,
        stamp,
    })
}

/// Shortest path between two entities.
pub async fn path(
    state: &SharedContext,
    project_id: i64,
    start_seed: &str,
    end_seed: &str,
    max_depth: usize,
    filter: &GraphFilterSpec,
) -> Result<GraphPath, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let limit = relation_config(state, project_id).await?.max_call_depth;
    let start = resolve_seed_in(&access.snapshot, start_seed)?;
    let end = resolve_seed_in(&access.snapshot, end_seed)?;
    let stamp = access.stamp().await;
    let path = GraphService::new(access.searcher)
        .shortest_path_with_options(start, end, max_depth.min(limit), &filter.to_filter())
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
    let (found, nodes, edges) = match path {
        Some(graph) => (true, graph.nodes, graph.edges),
        None => (false, Vec::new(), Vec::new()),
    };
    Ok(GraphPath {
        found,
        nodes,
        edges,
        stamp,
    })
}

/// Induced subgraph over an explicit seed set.
pub async fn subgraph(
    state: &SharedContext,
    project_id: i64,
    seeds: &[String],
    filter: &GraphFilterSpec,
    page: GraphPageSpec,
) -> Result<GraphView, RelationServiceError> {
    if seeds.is_empty() || seeds.len() > MAX_SUBGRAPH_IDS {
        return Err(RelationServiceError::InvalidInput(format!(
            "ids must list 1-{MAX_SUBGRAPH_IDS} stable symbol IDs"
        )));
    }
    let access = relation_access(state, project_id).await?;
    let entity_ids = seeds
        .iter()
        .map(|seed| resolve_seed_in(&access.snapshot, seed))
        .collect::<Result<Vec<_>, _>>()?;
    let stamp = access.stamp().await;
    let paged = GraphService::new(access.searcher)
        .subgraph_with_options(&entity_ids, &filter.to_filter(), page.to_pagination())
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
    Ok(GraphView {
        total_nodes: paged.total_nodes,
        total_edges: paged.total_edges,
        nodes: paged.nodes,
        edges: paged.edges,
        stamp,
    })
}

/// Connected components of the filtered graph.
pub async fn components(
    state: &SharedContext,
    project_id: i64,
    filter: &GraphFilterSpec,
    page: GraphPageSpec,
) -> Result<ComponentGroups, RelationServiceError> {
    if page.limit == 0 || page.limit > MAX_COMPONENTS {
        return Err(RelationServiceError::InvalidInput(format!(
            "limit must be within 1-{MAX_COMPONENTS}"
        )));
    }
    let access = relation_access(state, project_id).await?;
    let stamp = access.stamp().await;
    let paged = GraphService::new(access.searcher)
        .connected_components_with_options(&filter.to_filter(), page.to_pagination())
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
    let components = paged
        .components
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .map(|id| {
                    stable_id(access.snapshot.index.as_ref(), id)
                        .unwrap_or_else(|| format!("entity:{}", id.0))
                })
                .collect()
        })
        .collect();
    Ok(ComponentGroups {
        components,
        total_components: paged.total_components,
        stamp,
    })
}

/// Full project export, hub-first and capped at `limit` nodes.
pub async fn export(
    state: &SharedContext,
    project_id: i64,
    limit: usize,
    filter: &GraphFilterSpec,
    page: GraphPageSpec,
) -> Result<GraphView, RelationServiceError> {
    if limit == 0 || limit > MAX_EXPORT_NODES {
        return Err(RelationServiceError::InvalidInput(format!(
            "limit must be within 1-{MAX_EXPORT_NODES}"
        )));
    }
    let access = relation_access(state, project_id).await?;
    let stamp = access.stamp().await;
    let paged = GraphService::new(access.searcher)
        .export_full_with_options(limit, &filter.to_filter(), page.to_pagination())
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
    Ok(GraphView {
        total_nodes: paged.total_nodes,
        total_edges: paged.total_edges,
        nodes: paged.nodes,
        edges: paged.edges,
        stamp,
    })
}

/// Impact of changing one file.
pub async fn file_impact(
    state: &SharedContext,
    project_id: i64,
    file: &str,
) -> Result<ImpactView, RelationServiceError> {
    if file.trim().is_empty() {
        return Err(RelationServiceError::InvalidInput(
            "file must not be empty".to_string(),
        ));
    }
    let access = relation_access(state, project_id).await?;
    let impact = access.searcher.get_change_impact(file);
    Ok(ImpactView {
        changed: impact.changed,
        direct_dependents: impact.direct_dependents,
        indirect_dependents: impact.indirect_dependents,
        impact_score: impact.impact_score,
        stamp: access.stamp().await,
    })
}

/// Impact of changing one entity.
pub async fn entity_impact(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
    max_depth: usize,
    scope: ImpactScope,
) -> Result<ImpactView, RelationServiceError> {
    if max_depth == 0 || max_depth > MAX_ENTITY_IMPACT_DEPTH {
        return Err(RelationServiceError::InvalidInput(format!(
            "max_depth must be within 1-{MAX_ENTITY_IMPACT_DEPTH}"
        )));
    }
    let access = relation_access(state, project_id).await?;
    let entity_id = resolve_seed_in(&access.snapshot, seed)?;
    let impact = access
        .searcher
        .get_entity_impact(entity_id, max_depth, scope.as_str());
    Ok(ImpactView {
        changed: entity_ref(&access.snapshot, entity_id).stable_id,
        direct_dependents: impact.direct_dependents,
        indirect_dependents: impact.indirect_dependents,
        impact_score: impact.impact_score,
        stamp: access.stamp().await,
    })
}

/// Dependency cycles at entity or file level.
///
/// `level` is the canonical label already validated by the transport.
pub async fn cycles(
    state: &SharedContext,
    project_id: i64,
    file_level: bool,
    limit: usize,
) -> Result<CycleReport, RelationServiceError> {
    if limit == 0 || limit > MAX_CYCLES {
        return Err(RelationServiceError::InvalidInput(format!(
            "limit must be within 1-{MAX_CYCLES}"
        )));
    }
    let access = relation_access(state, project_id).await?;
    // Ask for one more than requested so `truncated` reflects reality instead
    // of assuming the page happened to be the last one.
    let probe = limit + 1;
    let mut cycles = if file_level {
        access
            .searcher
            .find_file_cycles(probe)
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        access
            .searcher
            .find_entity_cycles(probe)
            .into_iter()
            .map(|members| {
                members
                    .into_iter()
                    .map(|id| {
                        stable_id(access.snapshot.index.as_ref(), id)
                            .unwrap_or_else(|| format!("entity:{}", id.0))
                    })
                    .collect()
            })
            .collect::<Vec<_>>()
    };
    let truncated = cycles.len() > limit;
    let total = if truncated { probe } else { cycles.len() };
    cycles.truncate(limit);
    Ok(CycleReport {
        cycles,
        total,
        truncated,
        stamp: access.stamp().await,
    })
}

/// Typed structural relations of one entity.
pub async fn structural(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
    kind: StructuralKind,
    direction: StructuralDirection,
    limit: usize,
) -> Result<StructuralView, RelationServiceError> {
    if limit == 0 || limit > MAX_STRUCTURAL {
        return Err(RelationServiceError::InvalidInput(format!(
            "limit must be within 1-{MAX_STRUCTURAL}"
        )));
    }
    let access = relation_access(state, project_id).await?;
    let entity_id = resolve_seed_in(&access.snapshot, seed)?;
    let resolved = access
        .searcher
        .structural_relations(entity_id, kind, direction);
    let total_relations = resolved.len();
    let relations = resolved
        .into_iter()
        .take(limit)
        .map(|relation| StructuralEdgeView {
            entity_id: stable_id(access.snapshot.index.as_ref(), relation.entity_id)
                .unwrap_or_else(|| format!("entity:{}", relation.entity_id.0)),
            label: relation.label,
            relation: relation.relation_type.to_string(),
            domain: relation.relation_type.domain().to_string(),
            source_file: access
                .snapshot
                .index
                .get_file_path_by_entity(relation.entity_id)
                .unwrap_or_default(),
        })
        .collect();
    Ok(StructuralView {
        relations,
        total_relations,
        truncated: total_relations > limit,
        stamp: access.stamp().await,
    })
}

/// Module relations (imports, exports, caller files) of one file.
pub async fn module(
    state: &SharedContext,
    project_id: i64,
    file: &str,
) -> Result<ModuleView, RelationServiceError> {
    if file.trim().is_empty() {
        return Err(RelationServiceError::InvalidInput(
            "file must not be empty".to_string(),
        ));
    }
    let access = relation_access(state, project_id).await?;
    let module = access.searcher.get_module_relations(file);
    let imports = module
        .imports
        .into_iter()
        .take(MAX_MODULE_EDGES)
        .map(|relation| ModuleEdgeView {
            entity_id: relation
                .callee_id
                .and_then(|id| stable_id(access.snapshot.index.as_ref(), id))
                .unwrap_or_default(),
            target: relation.callee_name,
            relation: relation.relation_type.to_string(),
            domain: relation.relation_type.domain().to_string(),
        })
        .collect();
    Ok(ModuleView {
        file: file.to_string(),
        imports,
        exports: module
            .exports
            .into_iter()
            .map(|id| {
                stable_id(access.snapshot.index.as_ref(), id)
                    .unwrap_or_else(|| format!("entity:{}", id.0))
            })
            .collect(),
        caller_files: module.callers,
        stamp: access.stamp().await,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_direction_spelling() {
        for raw in ["in", "backward", "up", "UP"] {
            assert_eq!(
                GraphDirectionSpec::parse(raw),
                Some(GraphDirectionSpec::Backward)
            );
        }
        for raw in ["out", "forward", "down"] {
            assert_eq!(
                GraphDirectionSpec::parse(raw),
                Some(GraphDirectionSpec::Forward)
            );
        }
        assert_eq!(
            GraphDirectionSpec::parse("both"),
            Some(GraphDirectionSpec::Both)
        );
        assert_eq!(GraphDirectionSpec::parse("sideways"), None);
    }

    #[test]
    fn parses_impact_scope() {
        assert_eq!(ImpactScope::parse("Entity"), Some(ImpactScope::Entity));
        assert_eq!(ImpactScope::parse("file"), Some(ImpactScope::File));
        assert_eq!(ImpactScope::parse("project"), None);
    }
}
