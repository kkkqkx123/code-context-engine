//! Graph analysis handlers over the relation snapshot.
//!
//! Every handler decodes its wire parameters and delegates to the shared graph
//! service; only response mapping stays here.

use axum::extract::{Path, Query as QueryParams, State};

use cce_api::models::{
    CyclesQuery, EntityImpactQuery, ErrorResponse, GraphCycle, GraphCyclesResponse,
    GraphEntityImpactResponse, GraphImpactResponse, GraphModuleResponse, GraphStructuralResponse,
    ImpactQuery, ModuleQuery, ModuleRelation, StructuralQuery, StructuralRelation, error_codes,
};
use cce_server_shared::services::graph::{self, ImpactScope};

use crate::api::response::{ApiResult, relation_error};

/// Handle file impact request.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/impact", tag = "Graph",
    params(ImpactQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphImpactResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_impact(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<ImpactQuery>,
) -> ApiResult<GraphImpactResponse> {
    match graph::file_impact(&state, project_id, &params.file).await {
        Ok(impact) => ApiResult::Success(GraphImpactResponse {
            success: true,
            relation_epoch: impact.stamp.relation_epoch,
            changed_file: impact.changed,
            direct_dependents: impact.direct_dependents,
            indirect_dependents: impact.indirect_dependents,
            impact_score: impact.impact_score,
            relation_info: impact.stamp.stale,
        }),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

/// Handle entity impact request.
///
/// Reports which entities break when one entity changes, split into disjoint
/// first-hop callers and deeper callers.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/entity-impact", tag = "Graph",
    params(EntityImpactQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphEntityImpactResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_entity_impact(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<EntityImpactQuery>,
) -> ApiResult<GraphEntityImpactResponse> {
    let Some(scope) = ImpactScope::parse(&params.scope) else {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "scope must be one of entity, file".to_string(),
        ));
    };
    match graph::entity_impact(
        &state,
        project_id,
        &params.entity_id,
        params.max_depth,
        scope,
    )
    .await
    {
        Ok(impact) => ApiResult::Success(GraphEntityImpactResponse {
            success: true,
            relation_epoch: impact.stamp.relation_epoch,
            changed_entity: impact.changed,
            direct_dependents: impact.direct_dependents,
            indirect_dependents: impact.indirect_dependents,
            impact_score: impact.impact_score,
            relation_info: impact.stamp.stale,
        }),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

/// Handle dependency cycle request.
///
/// Reports cycles in the call graph (`level=entity`) or in the file dependency
/// graph (`level=file`).
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/cycles", tag = "Graph",
    params(CyclesQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphCyclesResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_cycles(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<CyclesQuery>,
) -> ApiResult<GraphCyclesResponse> {
    let level = params.level.to_lowercase();
    let file_level = match level.as_str() {
        "entity" => false,
        "file" => true,
        _ => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "level must be one of entity, file".to_string(),
            ));
        }
    };
    match graph::cycles(&state, project_id, file_level, params.limit).await {
        Ok(report) => ApiResult::Success(GraphCyclesResponse {
            success: true,
            relation_epoch: report.stamp.relation_epoch,
            level,
            cycles: report
                .cycles
                .into_iter()
                .map(|members| GraphCycle { members })
                .collect(),
            total_cycles: report.total,
            truncated: report.truncated,
            relation_info: report.stamp.stale,
        }),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

/// Handle structural / frontend relation request.
///
/// Typed access to the relation families that the generic `domains` filter can
/// only approximate: Rust trait bounds and the markup relation set (element
/// containment, event callbacks, parameter bindings, template references).
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/structural", tag = "Graph",
    params(StructuralQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphStructuralResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_structural(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<StructuralQuery>,
) -> ApiResult<GraphStructuralResponse> {
    use cce_orchestrator::query::{StructuralDirection, StructuralKind};

    let Some(kind) = StructuralKind::parse(&params.kind) else {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!(
                "unknown kind '{}'; expected one of: {}",
                params.kind,
                StructuralKind::labels().join(", ")
            ),
        ));
    };
    let direction = match params.direction.to_lowercase().as_str() {
        "out" | "outgoing" | "forward" => StructuralDirection::Outgoing,
        "in" | "incoming" | "backward" => StructuralDirection::Incoming,
        other => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("direction must be out or in, got '{other}'"),
            ));
        }
    };
    match graph::structural(
        &state,
        project_id,
        &params.entity_id,
        kind,
        direction,
        params.limit,
    )
    .await
    {
        Ok(view) => ApiResult::Success(GraphStructuralResponse {
            success: true,
            relation_epoch: view.stamp.relation_epoch,
            kind: params.kind,
            direction: params.direction,
            relations: view
                .relations
                .into_iter()
                .map(|relation| StructuralRelation {
                    entity_id: relation.entity_id,
                    label: relation.label,
                    relation: relation.relation,
                    domain: relation.domain,
                    source_file: relation.source_file,
                })
                .collect(),
            total_relations: view.total_relations,
            truncated: view.truncated,
            relation_info: view.stamp.stale,
        }),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}

/// Handle file module relations request.
///
/// Reports what a file pulls in (module-level imports/uses), what it exposes
/// (exports), and which files reach into it.
#[utoipa::path(
    get, path = "/api/project/{project_id}/graph/module", tag = "Graph",
    params(ModuleQuery, ("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = GraphModuleResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 503, body = ErrorResponse, description = "Index unavailable"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_graph_module(
    State(state): State<cce_server_shared::context::SharedContext>,
    Path(project_id): Path<i64>,
    QueryParams(params): QueryParams<ModuleQuery>,
) -> ApiResult<GraphModuleResponse> {
    match graph::module(&state, project_id, &params.file).await {
        Ok(module) => ApiResult::Success(GraphModuleResponse {
            success: true,
            relation_epoch: module.stamp.relation_epoch,
            file: module.file,
            exports: module.exports,
            caller_files: module.caller_files,
            imports: module
                .imports
                .into_iter()
                .map(|relation| ModuleRelation {
                    entity_id: relation.entity_id,
                    target: relation.target,
                    relation: relation.relation,
                    domain: relation.domain,
                })
                .collect(),
            relation_info: module.stamp.stale,
        }),
        Err(error) => ApiResult::Error(relation_error(error)),
    }
}
