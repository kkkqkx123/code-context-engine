# 控制台专用 API 清单

以下端点仅服务于 Web 控制台，位于 `cce-server::api::log_paths`，**不在** `cce-api` 共享契约与 OpenAPI 文档中，也无 Rust 客户端调用。

## 日志管理

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/api/logs` | 获取日志内容（查询参数控制条数/级别） |
| GET | `/api/logs/files` | 列出日志文件 |
| GET | `/api/logs/files/{name}/download` | 下载指定日志文件 |
| DELETE | `/api/logs/files/{name}` | 删除指定日志文件 |
| POST | `/api/logs/rotate` | 手动触发日志轮转 |
| GET | `/api/logs/config` | 获取日志配置 |
| PUT | `/api/logs/config` | 更新日志配置 |

## WebSocket

| 方法 | 路径 | 说明 |
|---|---|---|
| GET (WS) | `/ws/logs` | 实时日志流（WebSocket 升级） |

## 参考

- 路由定义：`crates/app/cce-server/src/api/router.rs`
- 路径常量：`crates/app/cce-server/src/api/log_paths.rs`
