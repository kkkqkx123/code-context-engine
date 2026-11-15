# Watch 域 HTTP 端点
> 操作数：3
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/project/{project_id}/watch/start` | `handle_start_watch` | project_id (p), body (StartWatchRequest) |
| GET | `/api/project/{project_id}/watch/status` | `handle_watch_status` | project_id (p) |
| POST | `/api/project/{project_id}/watch/stop` | `handle_stop_watch` | project_id (p) |
