# Config 域 HTTP 端点
> 操作数：3
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/config` | `handle_config_info` |  |
| POST | `/api/config/reload` | `handle_config_reload` | project_id (q) |
| GET | `/api/config/validate` | `handle_config_validate` |  |
