# Storage 域 HTTP 端点
> 操作数：6
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| DELETE | `/api/index` | `handle_clear_index` | body (ClearIndexRequest) |
| DELETE | `/api/index/batch` | `handle_batch_delete` | project_id (q), body (BatchDeleteRequest) |
| DELETE | `/api/index/entity/{id}` | `handle_delete_entity` | project_id (q), id (p) |
| DELETE | `/api/index/file/{path}` | `handle_delete_file` | project_id (q), path (p) |
| GET | `/api/index/stats` | `handle_index_stats` | project_id (q) |
| GET | `/api/storage/status` | `handle_storage_status` |  |
