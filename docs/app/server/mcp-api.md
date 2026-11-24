# MCP 工具清单

`cce-mcp` 通过 stdio 或独立端口的 Streamable HTTP 提供 MCP 工具表面（端点 `POST /mcp` 为会话入口）。工具总数 19 个；可见工具由能力 profile 与配置 `enabled_tools` 共同决定（取交集，详见下文）。

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
| `read_file` | 按行窗口读取项目内单个源码文件，带路径防护与截断标记（只读） |
| `file_outline` | 单文件符号骨架：名称、类型、行范围与签名（只读） |
| `entity_detail` | 按稳定符号 ID 查询实体定义详情（只读） |
| `graph_module` | 单文件模块关系：imports、导出符号与调用方文件（只读） |
| `graph_ego` | 实体在关系图中的有界邻域（只读） |

## 能力 profile

`[mcp] profile` 取值：

- `full`（默认）：暴露全部工具，行为与历史一致。
- `wiki_read`：只读画像，仅暴露只读工具，`index_project` 与 `incremental_index` 被排除，且**强制开启**调用预算。

`enabled_tools` 保留原有语义，作为 profile 之上的可选收窄：最终可见工具 = profile 允许集 ∩ `enabled_tools`。即使在 `wiki_read` 下用 `enabled_tools` 显式列出写工具，也无法越过只读边界。工具能力由 `crates/app/cce-mcp/src/profile.rs` 的能力表声明，并有单测锁定能力表键集合与路由一致。

## 有限步预算

只读工具在提供 run 上下文时受服务端强制的三级预算约束。run 上下文通过 `tools/call` 的 `_meta.wiki_run` 传入（首选），也可通过顶层公共参数 `run_id` / `page_id` / `snapshot_epoch` 传入：

```json
{ "run_id": "gen-abc", "page_id": "module-parser", "snapshot_epoch": 42 }
```

- 每次执行前检查 run 总步数、页面步数与尝试次数；超限返回结构化错误 `BUDGET_EXHAUSTED`，`remaining = 0`，`fallback = "wcp_only"`，不再执行工具。
- 执行失败退还占用的步数但计入尝试次数；尝试次数超页上限同样拒绝。
- 同一 run 内对相同工具、规范化参数、相同 `snapshot_epoch` 的确定性只读查询命中缓存，返回 `cached = true` 且不扣步。
- 成功的只读响应在 payload 顶层附带 `_budget` 元数据：`run_id`、`calls_used`、`calls_remaining`、`page_calls_used`、`page_calls_remaining`、`snapshot_epoch`、`cached`。

预算配置位于 `[mcp.budget]`（`enabled`、`max_calls_per_run`、`max_calls_per_page`、`max_return_items`、`max_read_file_lines`、`max_active_runs`、`cache_enabled`、`cache_ttl_secs`），均有安全默认值。未提供 run 上下文的普通调用不纳入预算，行为不变。

## 参考

- 工具定义与拦截层：`crates/app/cce-mcp/src/handler.rs`
- 能力表与 profile 解析：`crates/app/cce-mcp/src/profile.rs`
- `_meta` 解析与 `_budget` 注入：`crates/app/cce-mcp/src/budget.rs`
- 预算计数核心：`crates/app/cce-server-shared/src/services/budget.rs`
- 传输入口：`crates/app/cce-mcp/src/server.rs`
- 设计方案：`docs/plan/wiki-read-profile-and-budget-design.md`
