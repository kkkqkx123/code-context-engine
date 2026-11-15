# Entity 域 HTTP 端点
> 操作数：9
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/project/{project_id}/call-chain/{id}` | `handle_call_chain` | direction (q), max_depth (q), offset (q), limit (q), exclude_tests (q), directory_prefix (q), excluded_files (q), domains (q), include_external (q), project_id (p), id (p) |
| GET | `/api/project/{project_id}/call-path` | `handle_call_path` | start_id (q), end_id (q), max_depth (q), project_id (p) |
| GET | `/api/project/{project_id}/class/{id}/implementations` | `handle_class_implementations` | project_id (p), id (p) |
| GET | `/api/project/{project_id}/class/{id}/inheritance` | `handle_class_inheritance` | project_id (p), id (p) |
| GET | `/api/project/{project_id}/function/{id}` | `handle_function_detail` | project_id (p), id (p) |
| GET | `/api/project/{project_id}/function/{id}/callers` | `handle_function_callers` | max_depth (q), offset (q), limit (q), exclude_tests (q), directory_prefix (q), excluded_files (q), domains (q), include_external (q), project_id (p), id (p) |
| GET | `/api/project/{project_id}/function/{id}/calls` | `handle_function_calls` | max_depth (q), offset (q), limit (q), exclude_tests (q), directory_prefix (q), excluded_files (q), domains (q), include_external (q), project_id (p), id (p) |
| GET | `/api/project/{project_id}/relations/classification/stats` | `get_classification_stats` | project_id (p) |
| GET | `/api/project/{project_id}/relations/classification/{classification}` | `get_relations_by_classification` | limit (q), project_id (p), classification (p) |
