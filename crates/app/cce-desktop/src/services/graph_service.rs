//! Relation-graph commands for the graph panel.
//!
//! Each command binds one operation family: the panel only supplies the seed,
//! the depth and the page size. External endpoints stay included, matching the
//! contract default, so an exploration does not silently hide cross-project
//! edges the server would have returned.

use cce_api::models::{
    ComponentsQuery, CyclesQuery, EgoQuery, EntityImpactQuery, ExportQuery,
    GraphComponentsResponse, GraphCyclesResponse, GraphEntityImpactResponse, GraphImpactResponse,
    GraphModuleResponse, GraphPathQuery, GraphPathResponse, GraphStructuralResponse,
    GraphSubgraphResponse, ImpactQuery, ModuleQuery, StructuralQuery, SubgraphQuery,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

/// Direction of an ego traversal as the panel offers it.
pub const EGO_DIRECTIONS: [&str; 3] = ["both", "out", "in"];

/// Level a cycle query runs at.
pub const CYCLE_LEVELS: [&str; 2] = ["entity", "file"];

/// Structural relation families the panel can ask for.
pub const STRUCTURAL_KINDS: [&str; 8] = [
    "trait_bound",
    "child_elements",
    "parent_element",
    "event_handlers",
    "handler_elements",
    "parameter_bindings",
    "template_references",
    "template_ref_owners",
];

pub fn ego(
    cx: &App,
    client: Client,
    project_id: i64,
    entity_id: String,
    depth: usize,
    direction: String,
    limit: usize,
) -> Task<Result<GraphSubgraphResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let query = EgoQuery {
            entity_id,
            depth,
            direction,
            offset: 0,
            limit,
            domains: String::new(),
            relation_types: String::new(),
            include_external: true,
        };
        client.graph_ego(project_id, &query).await
    })
}

pub fn path(
    cx: &App,
    client: Client,
    project_id: i64,
    start: String,
    end: String,
    max_depth: usize,
) -> Task<Result<GraphPathResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let query = GraphPathQuery {
            start,
            end,
            max_depth,
            domains: String::new(),
            relation_types: String::new(),
            include_external: true,
        };
        client.graph_path(project_id, &query).await
    })
}

/// Subgraph of an explicit seed set; ids arrive as one field of stable ids.
pub fn subgraph(
    cx: &App,
    client: Client,
    project_id: i64,
    ids: Vec<String>,
    limit: usize,
) -> Task<Result<GraphSubgraphResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let query = SubgraphQuery {
            ids: ids.join(","),
            offset: 0,
            limit,
            domains: String::new(),
            relation_types: String::new(),
            include_external: true,
        };
        client.graph_subgraph(project_id, &query).await
    })
}

pub fn components(
    cx: &App,
    client: Client,
    project_id: i64,
    limit: usize,
) -> Task<Result<GraphComponentsResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let query = ComponentsQuery {
            offset: 0,
            limit,
            domains: String::new(),
            relation_types: String::new(),
            include_external: true,
        };
        client.graph_components(project_id, &query).await
    })
}

pub fn export(
    cx: &App,
    client: Client,
    project_id: i64,
    limit: usize,
) -> Task<Result<GraphSubgraphResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let query = ExportQuery {
            limit,
            offset: 0,
            domains: String::new(),
            relation_types: String::new(),
            include_external: true,
        };
        client.graph_export(project_id, &query).await
    })
}

pub fn impact(
    cx: &App,
    client: Client,
    project_id: i64,
    file: String,
) -> Task<Result<GraphImpactResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.graph_impact(project_id, &ImpactQuery { file }).await
    })
}

/// Dependents of one stable symbol, at entity granularity.
pub fn entity_impact(
    cx: &App,
    client: Client,
    project_id: i64,
    entity_id: String,
    max_depth: usize,
) -> Task<Result<GraphEntityImpactResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client
            .graph_entity_impact(
                project_id,
                &EntityImpactQuery {
                    entity_id,
                    max_depth,
                    scope: "entity".to_string(),
                },
            )
            .await
    })
}

pub fn cycles(
    cx: &App,
    client: Client,
    project_id: i64,
    level: String,
    limit: usize,
) -> Task<Result<GraphCyclesResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let query = CyclesQuery { level, limit };
        client.graph_cycles(project_id, &query).await
    })
}

pub fn structural(
    cx: &App,
    client: Client,
    project_id: i64,
    entity_id: String,
    kind: String,
) -> Task<Result<GraphStructuralResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let query = StructuralQuery {
            entity_id,
            kind,
            direction: "out".to_string(),
            limit: 200,
        };
        client.graph_structural(project_id, &query).await
    })
}

pub fn module(
    cx: &App,
    client: Client,
    project_id: i64,
    file: String,
) -> Task<Result<GraphModuleResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.graph_module(project_id, &ModuleQuery { file }).await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_choices_follow_the_contract() {
        assert_eq!(EGO_DIRECTIONS, ["both", "out", "in"]);
        assert_eq!(CYCLE_LEVELS, ["entity", "file"]);
    }

    #[test]
    fn structural_families_are_the_typed_set() {
        assert_eq!(STRUCTURAL_KINDS.len(), 8);
        assert!(STRUCTURAL_KINDS.contains(&"trait_bound"));
        assert!(STRUCTURAL_KINDS.contains(&"template_ref_owners"));
    }
}
