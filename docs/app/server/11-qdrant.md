# Qdrant 域 HTTP 端点
> 操作数：4
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/qdrant/process/restart` | `handle_qdrant_process_restart` |  |
| POST | `/api/qdrant/process/start` | `handle_qdrant_process_start` |  |
| GET | `/api/qdrant/process/status` | `handle_qdrant_process_status` |  |
| POST | `/api/qdrant/process/stop` | `handle_qdrant_process_stop` |  |
