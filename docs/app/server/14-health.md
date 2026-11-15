# Health 域 HTTP 端点
> 操作数：9
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/health` | `handle_health` |  |
| GET | `/api/health/bm25` | `handle_bm25_health` |  |
| GET | `/api/health/embedding` | `handle_embedding_health` |  |
| GET | `/api/health/qdrant` | `handle_qdrant_health` |  |
| DELETE | `/api/retry-queue` | `handle_retry_queue_clear` | project_id (q) |
| GET | `/api/retry-queue` | `handle_retry_queue_status` | project_id (q) |
| DELETE | `/api/retry-queue/dead` | `handle_retry_queue_dead_clear` | project_id (q) |
| GET | `/api/retry-queue/dead` | `handle_retry_queue_dead_list` | project_id (q) |
| POST | `/api/retry-queue/process` | `handle_retry_queue_process` | project_id (q) |
