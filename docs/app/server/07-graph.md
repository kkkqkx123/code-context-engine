# Graph 域 HTTP 端点
> 操作数：10
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/project/{project_id}/graph/components` | `handle_graph_components` | offset (q), limit (q), domains (q), relation_types (q), include_external (q), project_id (p) |
| GET | `/api/project/{project_id}/graph/cycles` | `handle_graph_cycles` | level (q), limit (q), project_id (p) |
| GET | `/api/project/{project_id}/graph/ego` | `handle_graph_ego` | entity_id (q), depth (q), direction (q), offset (q), limit (q), domains (q), relation_types (q), include_external (q), project_id (p) |
| GET | `/api/project/{project_id}/graph/entity-impact` | `handle_graph_entity_impact` | entity_id (q), max_depth (q), scope (q), project_id (p) |
| GET | `/api/project/{project_id}/graph/export` | `handle_graph_export` | limit (q), offset (q), domains (q), relation_types (q), include_external (q), project_id (p) |
| GET | `/api/project/{project_id}/graph/impact` | `handle_graph_impact` | file (q), project_id (p) |
| GET | `/api/project/{project_id}/graph/module` | `handle_graph_module` | file (q), project_id (p) |
| GET | `/api/project/{project_id}/graph/path` | `handle_graph_path` | start (q), end (q), max_depth (q), domains (q), relation_types (q), include_external (q), project_id (p) |
| GET | `/api/project/{project_id}/graph/structural` | `handle_graph_structural` | entity_id (q), kind (q), direction (q), limit (q), project_id (p) |
| GET | `/api/project/{project_id}/graph/subgraph` | `handle_graph_subgraph` | ids (q), offset (q), limit (q), domains (q), relation_types (q), include_external (q), project_id (p) |
