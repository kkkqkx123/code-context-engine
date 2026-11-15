# Server HTTP API 清单


> 范围：`crates/app/cce-server` HTTP 传输层暴露的全部端点。
> 来源：`tools/openapi-codegen/openapi.json`（OpenAPI 3.0 快照，79 个操作 / 72 个路径）。
> admission 构建（`--features admission`）额外 5 个操作 / 5 个路径。
> 基础前缀：`/api`（控制台日志与 WebSocket 单独列出）。

## 通用约定

- JSON 响应统一为 `{ "success": bool, "data": ..., "error": { "code", "message", "details?" } }`。
- 项目作用域端点使用路径参数 `{project_id}`（整数项目 ID）。
- admission 构建下，远程部署需要配置 `CCE_ADMISSION_TOKENS`；未配置时回退为本地路由。
- 控制台专用端点（日志、WebSocket）见 `console-api.md`，不在 OpenAPI 契约中。
- MCP 工具见 `mcp-api.md`。

## 文档索引

| 文件 | 域 | 操作数 |
|------|----|--------|
| [02-index.md](02-index.md) | Index | 3 |
| [03-summary.md](03-summary.md) | Summary | 1 |
| [04-storage.md](04-storage.md) | Storage | 6 |
| [05-project.md](05-project.md) | Project | 15 |
| [06-entity.md](06-entity.md) | Entity | 9 |
| [07-graph.md](07-graph.md) | Graph | 10 |
| [08-metrics.md](08-metrics.md) | Metrics | 4 |
| [09-watch.md](09-watch.md) | Watch | 3 |
| [10-config.md](10-config.md) | Config | 3 |
| [11-qdrant.md](11-qdrant.md) | Qdrant | 4 |
| [12-search.md](12-search.md) | Search | 3 |
| [13-tools.md](13-tools.md) | Tools | 9 |
| [14-health.md](14-health.md) | Health | 9 |
| [15-ingest.md](15-ingest.md) | Ingest | 4 |
| [16-admission.md](16-admission.md) | Admission | 1 |

## 补充文档

| 文件 | 说明 |
|------|------|
| [console-api.md](console-api.md) | 控制台专用日志与 WebSocket 端点 |
| [mcp-api.md](mcp-api.md) | MCP 工具表面 |

## 生成说明


本目录由 `docs/app/server/generate-api-list.py` 从 `tools/openapi-codegen/openapi*.json` 自动生成；端点变更后请重新运行该脚本。
