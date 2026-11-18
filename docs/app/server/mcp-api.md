# MCP 工具清单

`cce-mcp` 通过 stdio 或独立端口的 Streamable HTTP 提供 MCP 工具表面（端点 `POST /mcp` 为会话入口）。工具总数 14 个；可通过配置 `enabled_tools` 进行允许列表过滤。

| 工具名 | 说明 |
|---|---|
| `search` | 项目内混合（语义 + BM25）代码搜索，返回匹配块及文件路径、行号范围、分数 |
| `aggregated_search` | 聚合检索：多条子查询共用一份索引快照并行召回、按代码身份合并、只做一次重排；返回合并结果与逐子查询统计（候选数、贡献数、状态） |
| `file_search` | 按文件路径模糊搜索，可选返回大小与行数；远程项目报告不可用 |
| `keyword_search` | BM25 关键词搜索，返回原始源码片段、文件路径与行号范围 |
| `entity_callees` | 获取实体的直接被调用者（callees）及关系信息 |
| `entity_callers` | 获取实体的直接调用者（callers） |
| `call_chain` | 沿单方向遍历调用链（forward/down=callees，backward/up=callers） |
| `list_projects` | 列出所有已注册项目及其 ID、名称、根路径 |
| `get_project` | 获取单个项目元数据与生效配置 |
| `index_project` | 对项目执行完整索引（写操作） |
| `incremental_index` | 对项目执行显式文件变更的增量索引（写操作） |
| `fold` | 将原始源码折叠为符号骨架（无状态，无需 project_id） |
| `fold_batch` | 批量折叠多个源码为符号骨架（无状态） |
| `health` | 项目存储后端健康摘要（元数据存储、BM25、关系运行时） |

## 参考

- 工具定义：`crates/app/cce-mcp/src/handler.rs`
- 传输入口：`crates/app/cce-mcp/src/server.rs`
- MCP 文档：`docs/app/mcp`（如存在）
