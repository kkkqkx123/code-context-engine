# Metrics 域 HTTP 端点
> 操作数：4
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/metrics` | `handle_get_metrics` |  |
| DELETE | `/api/metrics/cleanup` | `handle_cleanup_metrics` | all (q), before (q) |
| GET | `/api/metrics/history` | `handle_get_metrics_history` | from (q), to (q), metric (q), project_id (q), operation_type (q) |
| GET | `/api/metrics/json` | `handle_get_metrics_json` |  |
