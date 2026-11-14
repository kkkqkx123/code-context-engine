//! Relation-graph traversal and analysis.

use cce_api::models::{
    ComponentsQuery, CyclesQuery, EgoQuery, EntityImpactQuery, ExportQuery,
    GraphComponentsResponse, GraphCyclesResponse, GraphEntityImpactResponse, GraphImpactResponse,
    GraphModuleResponse, GraphPathQuery, GraphPathResponse, GraphStructuralResponse,
    GraphSubgraphResponse, ImpactQuery, ModuleQuery, StructuralQuery, SubgraphQuery,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Neighbourhood of one seed entity.
    pub async fn graph_ego(
        &self,
        project_id: i64,
        query: &EgoQuery,
    ) -> Result<GraphSubgraphResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_EGO_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Shortest relation path between two seed entities.
    pub async fn graph_path(
        &self,
        project_id: i64,
        query: &GraphPathQuery,
    ) -> Result<GraphPathResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_PATH_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Explicit entity set plus its induced edges.
    pub async fn graph_subgraph(
        &self,
        project_id: i64,
        query: &SubgraphQuery,
    ) -> Result<GraphSubgraphResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_SUBGRAPH_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Connected components of the relation graph.
    pub async fn graph_components(
        &self,
        project_id: i64,
        query: &ComponentsQuery,
    ) -> Result<GraphComponentsResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_COMPONENTS_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Whole-graph export bounded by the given page.
    pub async fn graph_export(
        &self,
        project_id: i64,
        query: &ExportQuery,
    ) -> Result<GraphSubgraphResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_EXPORT_TEMPLATE, project_id),
            query,
            self.long_task_budget(),
        )
        .await
    }

    /// Files and entities affected by a changed file.
    pub async fn graph_impact(
        &self,
        project_id: i64,
        query: &ImpactQuery,
    ) -> Result<GraphImpactResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_IMPACT_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Dependents of a changed entity.
    pub async fn graph_entity_impact(
        &self,
        project_id: i64,
        query: &EntityImpactQuery,
    ) -> Result<GraphEntityImpactResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_ENTITY_IMPACT_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Dependency cycles at entity or file granularity.
    pub async fn graph_cycles(
        &self,
        project_id: i64,
        query: &CyclesQuery,
    ) -> Result<GraphCyclesResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_CYCLES_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// One typed structural relation family rooted at an entity.
    pub async fn graph_structural(
        &self,
        project_id: i64,
        query: &StructuralQuery,
    ) -> Result<GraphStructuralResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_STRUCTURAL_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Module-level relations of a file.
    pub async fn graph_module(
        &self,
        project_id: i64,
        query: &ModuleQuery,
    ) -> Result<GraphModuleResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::GRAPH_MODULE_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }
}
