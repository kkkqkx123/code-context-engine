//! Graph analysis handlers over the relation snapshot.

use axum::extract::{Path, Query as QueryParams, State};

use cce_api::models::{
    CyclesQuery, EntityImpactQuery, ErrorResponse, GraphCycle, GraphCyclesResponse,
    GraphEntityImpactResponse, GraphImpactResponse, GraphModuleResponse, GraphStructuralResponse,
    ImpactQuery, ModuleQuery, ModuleRelation, StructuralQuery, StructuralRelation, error_codes,
};
use cce_codegraph::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};

use crate::api::handlers::entity::seed::resolve_symbol_seed;
use crate::api::response::ApiResult;

use super::{graph_context, stale_relation_info};

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
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    if params.file.trim().is_empty() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "file must not be empty".to_string(),
        ));
    }
    let impact = searcher.get_change_impact(&params.file);
    ApiResult::Success(GraphImpactResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        changed_file: impact.changed,
        direct_dependents: impact.direct_dependents,
        indirect_dependents: impact.indirect_dependents,
        impact_score: impact.impact_score,
        relation_info: stale_relation_info(&runtime).await,
    })
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
    const MAX_IMPACT_DEPTH: usize = 50;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    if params.max_depth == 0 || params.max_depth > MAX_IMPACT_DEPTH {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("max_depth must be within 1-{MAX_IMPACT_DEPTH}"),
        ));
    }
    let scope = params.scope.to_lowercase();
    if scope != "entity" && scope != "file" {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "scope must be one of entity, file".to_string(),
        ));
    }
    let entity_id = match resolve_symbol_seed(snapshot.index.as_ref(), &params.entity_id) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let impact = searcher.get_entity_impact(entity_id, params.max_depth, &scope);
    let name_of = |id: cce_types::EntityId| {
        snapshot
            .index
            .get_symbol_key_by_entity_id(id)
            .map(|key| key.stable_id().0)
            .unwrap_or_else(|| format!("entity:{}", id.0))
    };
    ApiResult::Success(GraphEntityImpactResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        changed_entity: name_of(entity_id),
        direct_dependents: impact
            .direct_dependents
            .iter()
            .map(|s| s.as_str().to_string())
            .collect(),
        indirect_dependents: impact
            .indirect_dependents
            .iter()
            .map(|s| s.as_str().to_string())
            .collect(),
        impact_score: impact.impact_score,
        relation_info: stale_relation_info(&runtime).await,
    })
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
    /// Hard ceiling; the detector stops early at this count.
    const MAX_CYCLES: usize = 5_000;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    let level = params.level.to_lowercase();
    if level != "entity" && level != "file" {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "level must be one of entity, file".to_string(),
        ));
    }
    if params.limit == 0 || params.limit > MAX_CYCLES {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("limit must be within 1-{MAX_CYCLES}"),
        ));
    }
    // Ask for one more than requested so `truncated` reflects reality instead
    // of assuming the page happened to be the last one.
    let probe = params.limit + 1;
    let cycles = if level == "entity" {
        searcher
            .find_entity_cycles(probe)
            .into_iter()
            .map(|members| GraphCycle {
                members: members
                    .into_iter()
                    .map(|id| {
                        snapshot
                            .index
                            .get_symbol_key_by_entity_id(id)
                            .map(|key| key.stable_id().0)
                            .unwrap_or_else(|| format!("entity:{}", id.0))
                    })
                    .collect(),
            })
            .collect::<Vec<_>>()
    } else {
        searcher
            .find_file_cycles(probe)
            .into_iter()
            .map(|members| GraphCycle { members })
            .collect::<Vec<_>>()
    };
    // The detector is probed with one extra slot so truncation is observed.
    // The count is a lower bound up to the probe, not the true project total.
    let truncated = cycles.len() > params.limit;
    let total_cycles = if truncated {
        params.limit + 1
    } else {
        cycles.len()
    };
    let cycles = cycles.into_iter().take(params.limit).collect();
    ApiResult::Success(GraphCyclesResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        level,
        cycles,
        total_cycles,
        truncated,
        relation_info: stale_relation_info(&runtime).await,
    })
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

    const MAX_STRUCTURAL: usize = 2_000;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    let kind = match StructuralKind::parse(&params.kind) {
        Some(kind) => kind,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!(
                    "unknown kind '{}'; expected one of: {}",
                    params.kind,
                    StructuralKind::labels().join(", ")
                ),
            ));
        }
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
    if params.limit == 0 || params.limit > MAX_STRUCTURAL {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("limit must be within 1-{MAX_STRUCTURAL}"),
        ));
    }
    let entity_id = match resolve_symbol_seed(snapshot.index.as_ref(), &params.entity_id) {
        Ok(id) => id,
        Err(e) => return ApiResult::Error(e),
    };
    let resolved = searcher.structural_relations(entity_id, kind, direction);
    let total_relations = resolved.len();
    let truncated = total_relations > params.limit;
    let relations = resolved
        .into_iter()
        .take(params.limit)
        .map(|relation| StructuralRelation {
            entity_id: snapshot
                .index
                .get_symbol_key_by_entity_id(relation.entity_id)
                .map(|key| key.stable_id().0)
                .unwrap_or_else(|| format!("entity:{}", relation.entity_id.0)),
            label: relation.label,
            relation: relation.relation_type.to_string(),
            domain: relation.relation_type.domain().to_string(),
            source_file: snapshot
                .index
                .as_ref()
                .get_file_path_by_entity(relation.entity_id)
                .unwrap_or_default(),
        })
        .collect();
    ApiResult::Success(GraphStructuralResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        kind: params.kind,
        direction: params.direction,
        relations,
        total_relations,
        truncated,
        relation_info: stale_relation_info(&runtime).await,
    })
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
    const MAX_MODULE_EDGES: usize = 5_000;
    let (snapshot, searcher, runtime) = match graph_context(&state, project_id).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiResult::Error(e),
    };
    if params.file.trim().is_empty() {
        return ApiResult::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "file must not be empty".to_string(),
        ));
    }
    let modules = searcher.get_module_relations(&params.file);
    let imports = modules
        .imports
        .into_iter()
        .take(MAX_MODULE_EDGES)
        .map(|relation| ModuleRelation {
            entity_id: relation
                .callee_id
                .map(|id| {
                    snapshot
                        .index
                        .get_symbol_key_by_entity_id(id)
                        .map(|key| key.stable_id().0)
                        .unwrap_or_else(|| format!("entity:{}", id.0))
                })
                .unwrap_or_default(),
            target: relation.callee_name,
            relation: relation.relation_type.to_string(),
            domain: relation.relation_type.domain().to_string(),
        })
        .collect();
    ApiResult::Success(GraphModuleResponse {
        success: true,
        relation_epoch: snapshot.relation_epoch,
        file: params.file,
        exports: modules
            .exports
            .into_iter()
            .map(|id| {
                snapshot
                    .index
                    .get_symbol_key_by_entity_id(id)
                    .map(|key| key.stable_id().0)
                    .unwrap_or_else(|| format!("entity:{}", id.0))
            })
            .collect(),
        caller_files: modules.callers,
        imports,
        relation_info: stale_relation_info(&runtime).await,
    })
}
