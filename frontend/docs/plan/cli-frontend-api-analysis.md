# CLI 和 Frontend API 对比分析报告

> 基于 server 包 (`crates/cce_server`) 的完整 API 实现，对照分析 `cce-cli` 包和 `frontend` 包的 API 调用存在的不足，并提出修复方案。

---

## 目录

1. [分析范围](#1-分析范围)
2. [CLI 包问题分析](#2-cli-包问题分析)
3. [Frontend 包问题分析](#3-frontend-包问题分析)
4. [修复优先级和时间线](#4-修复优先级和时间线)
5. [附录：Server API 完整清单](#5-附录server-api-完整清单)

---

## 1. 分析范围

### 参照基准

- Server 路由定义：`crates/cce_server/src/api/router.rs`（40+ 端点）
- Server 请求/响应模型：`crates/cce_server/src/api/handlers/models.rs`
- Server 各 handler 实现：`crates/cce_server/src/api/handlers/` 目录

### 被分析对象

- **CLI 包**：`cce-cli/` 目录，主要文件在 `cce-cli/src/commands/` 和 `cce-cli/src/types.rs`
- **Frontend 包**：`frontend/src/lib/api/` 目录，已完成 Phase 0-2 修复

### 已知前置修复

Frontend 包已在之前轮次完成以下修复（本报告不再重复分析）：
- Phase 0：Entity/Watch API 路径添加 `{project_id}` 前缀、ID 类型 `number` → `string`、添加 `relation_epoch` 字段
- Phase 1：组件级别的类型签名对齐
- Phase 2：Qdrant 进程管理 UI、配置管理页面、摘要生成页面、项目管理页面

---

## 2. CLI 包问题分析

### P0 — 功能完全不可用（Critical）

#### P0-1：Entity API 路径缺少 `{project_id}` 前缀

**现状**：CLI 所有实体端点使用 `/api/function/{id}`、`/api/call-chain/{id}`、`/api/class/{id}/inheritance` 等路径，不包含 `{project_id}` 路径段。

**Server 期望**：
```
GET  /api/project/{project_id}/function/{id}
GET  /api/project/{project_id}/function/{id}/calls
GET  /api/project/{project_id}/function/{id}/callers
GET  /api/project/{project_id}/call-chain/{id}
GET  /api/project/{project_id}/call-path
GET  /api/project/{project_id}/class/{id}/inheritance
GET  /api/project/{project_id}/class/{id}/implementations
```

**影响命令**：`entity function`、`entity calls`、`entity callers`、`entity call-chain`、`entity call-path`、`entity inheritance`、`entity implementations`

**修复方案**：
1. 实体子命令添加 `--project-id` 参数
2. 所有 API 路径改为 `/api/project/{project_id}/...`
3. 参考：`cce-cli/src/commands/entity.rs` 第 45、88、122、151、156、198、206、237、243、283、289 行

#### P0-2：Entity ID 类型 `u32` → `String`

**现状**：CLI 使用 `u32` 作为函数/实体 ID 类型，但 Server 使用 **stable symbol ID**（字符串类型）。

**影响字段**：
- `FunctionInfo.id: u32` → `String`
- `CallChainNode.function_id: u32` → `String`
- `CallPathResponse.start_function_id: u32` → `String`
- `CallPathResponse.end_function_id: u32` → `String`
- `delete_entity` 参数 `id: u32` → `String`
- `BatchDeleteRequest.entity_ids: Vec<u32>` → `Vec<String>`

**影响文件**：
- `cce-cli/src/types.rs`：`FunctionInfo`（第 208 行）、`CallChainNode`（第 250 行）、`CallPathResponse`（第 272-273 行）
- `cce-cli/src/commands/entity.rs`：所有函数签名（第 40、83、117、151、191 行等）
- `cce-cli/src/commands/storage.rs`：`delete_entity`（第 172 行）、`batch_delete`（第 200 行）

#### P0-3：Watch API 路径缺少 `{project_id}` 前缀

**现状**：CLI 使用 `/api/watch/start`、`/api/watch/stop`、`/api/watch/status`，且 Watch 子命令没有 `--project-id` 参数。

**Server 期望**：
```
POST /api/project/{project_id}/watch/start
POST /api/project/{project_id}/watch/stop
GET  /api/project/{project_id}/watch/status
```

**修复方案**：
1. Watch 子命令添加 `--project-id` 参数
2. API 路径改为 `/api/project/{project_id}/watch/...`

#### P0-4：清除索引请求格式完全错误

**现状**：CLI 发送 `ClearIndexRequest { clear_vectors, clear_bm25, clear_relations, clear_cache }`，但 Server 期望的重置是清空整个项目索引，只需要 `{ project_id: i64 }`。

**Server 期望**（`crates/cce_server/src/api/handlers/models.rs` 第 347 行）：
```rust
pub struct ClearIndexRequest {
    pub project_id: i64,
}
```

**修复方案**：
1. `ClearIndexRequest` 改为 `{ project_id: i64 }` 格式
2. `storage clear` 子命令添加 `--project-id` 参数
3. 移除 `vectors`、`bm25`、`relations`、`cache` 子命令选项

#### P0-5：增量索引请求缺少 `project_id`

**现状**：CLI 的 `IncrementalIndexRequest`（`cce-cli/src/types.rs` 第 25 行）不包含 `project_id: i64`。

**Server 期望**（`crates/cce_server/src/api/handlers/models.rs` 第 703 行）：
```rust
pub struct IncrementalIndexRequest {
    pub project_id: i64,
    pub files_to_index: Vec<String>,
    pub files_to_remove: Vec<String>,
    pub force_reindex: bool,
}
```

#### P0-6：Index Stats 缺少 `project_id` 查询参数

**现状**：CLI 调用 `GET /api/index/stats` 不带查询参数（`cce-cli/src/commands/storage.rs` 第 97 行）。

**Server 期望**：`GET /api/index/stats?project_id={project_id}`

#### P0-7：Config Reload 缺少 `project_id` 查询参数

**现状**：CLI 执行 `POST /api/config/reload` 时发送空 body，不携带 `project_id` 查询参数（`cce-cli/src/commands/config.rs` 第 34 行）。

**Server 期望**：`POST /api/config/reload?project_id={project_id}`

#### P0-8：Tools Compress 请求格式完全错误

**现状**：CLI 发送 `CompressRequest { code: String, language: Option<String> }`，但 Server 期望基于文件路径的压缩。

**Server 期望**（`crates/cce_server/src/api/handlers/tools/compression.rs` 第 15 行）：
```rust
pub struct CompressRequest {
    pub file_path: String,
    pub include_entities: bool,
    pub include_groups: bool,
}
```

**修复方案**：
1. `CompressRequest` 改为 `{ file_path, include_entities, include_groups }`
2. `compress` 子命令改为 `--file-path` 参数
3. Server 响应改为 `CompressApiResponse { success, data, error }` 格式

#### P0-9：Tools Batch Compress 请求格式完全错误

**现状**：CLI 发送 `codes` 和 `languages` 列表，但 Server 期望 `file_paths` 列表。

**Server 期望**（`crates/cce_server/src/api/handlers/tools/compression.rs` 第 40 行）：
```rust
pub struct CompressBatchRequest {
    pub file_paths: Vec<String>,
    pub include_entities: bool,
    pub include_groups: bool,
    pub max_concurrency: usize,
}
```

#### P0-10：Tools References/Definition 请求格式完全错误

**现状**：CLI 发送 `ReferencesRequest { symbol, file_path }` 和 `DefinitionRequest { symbol, file_path }`，但 Server 期望 `project_id` + `path` + `line` + `column` 定位。

**Server 期望**：
```rust
// POST /api/tools/references
pub struct FindRefsRequest {
    pub project_id: i64,
    pub path: String,
    pub line: usize,
    pub column: Option<usize>,
    pub symbol: Option<String>,
    pub context_lines: Option<usize>,
    pub include_snippet: Option<bool>,
    pub include_entity_info: Option<bool>,
}

// POST /api/tools/definition
pub struct GotoDefRequest {
    pub project_id: i64,
    pub path: String,
    pub line: usize,
    pub column: Option<usize>,
    pub symbol: Option<String>,
    pub include_body: bool,
}
```

**Server 响应格式**（包含 `success`, `result`, `error`, `relation_info` 字段，而非 CLI 当前的 `references`/`definition` 直接字段）：

```rust
// Find references response
pub struct FindRefsApiResponse {
    pub success: bool,
    pub result: Option<FindReferencesResponse>,
    pub error: Option<String>,
    pub relation_info: Option<serde_json::Value>,
}

// Goto definition response
pub struct GotoDefApiResponse {
    pub success: bool,
    pub result: Option<GotoDefinitionResponse>,
    pub error: Option<String>,
    pub relation_info: Option<serde_json::Value>,
}
```

#### P0-11：Tools Symbols 请求格式完全错误

**现状**：CLI 发送 `SymbolsRequest { file_path, language }`，但 Server 期望 `project_id` + `paths`。

**Server 期望**（`crates/cce_server/src/api/handlers/tools/symbol.rs` 第 157 行）：
```rust
pub struct GetSymsRequest {
    pub project_id: i64,
    pub paths: Vec<String>,
}
```

**Server 响应格式**：
```rust
pub struct GetSymsApiResponse {
    pub success: bool,
    pub result: Option<GetSymbolsResponse>,
    pub error: Option<String>,
    pub relation_info: Option<serde_json::Value>,
}
```

#### P0-12：Tools Diagnose 请求格式与 Server 接口不匹配

**现状**：CLI 发送 `DiagnoseRequest { code, language }`，但 Server 的 diagnose 接口...（需要确认 Server 是否有对应的 diagnose 端点）。

**建议**：先确认 Server 是否实现了 `/api/tools/diagnose` 端点。如果不存在，CLI 的 `tool diagnose` 命令将无法工作。

---

### P1 — 类型不匹配 / 缺少字段（Major）

#### P1-1：Entity 响应缺少 `relation_epoch` 字段

**现状**：以下 CLI 响应类型缺少 `relation_epoch: i64` 字段：
- `FunctionCallsResponse`（`cce-cli/src/types.rs` 第 231 行）
- `FunctionCallersResponse`（第 240 行）
- `CallChainResponse`（第 261 行）
- `CallPathResponse`（第 270 行）
- `ClassInheritanceResponse`（第 282 行）
- `ClassImplementationsResponse`（第 299 行）

**Server 期望**：所有上述响应类型均包含 `relation_epoch: i64`。

#### P1-2：Entity 响应缺少 `function_id`/`class_id`/`interface_id` 字段

**现状**：
- `FunctionCallsResponse` 缺少 `function_id: String`
- `FunctionCallersResponse` 缺少 `function_id: String`
- `ClassInheritanceResponse` 缺少 `class_id: String`
- `ClassRelation` 缺少 `class_id: String`
- `ClassImplementationsResponse` 缺少 `class_id: String`
- `InterfaceRelation` 缺少 `interface_id: String`

#### P1-3：SearchResultItem 缺少 `id: u64` 字段

**现状**：CLI 的 `SearchResultItem`（`cce-cli/src/types.rs` 第 183 行）缺少 `id: u64`。

**Server 期望**（`crates/cce_server/src/api/handlers/models.rs`）：
```rust
pub struct SearchResultItem {
    pub id: u64,
    pub score: f32,
    // ...其余字段
}
```

#### P1-4：EntitySearchResult 缺少多个字段

**现状**：CLI 的 `EntitySearchResult`（`cce-cli/src/types.rs` 第 632 行）只包含 `{ name, kind, signature, span_start_row, span_end_row }`，缺少 `id: i64`、`file_id: i64`、`depth: Option<i64>`、`parent_id: Option<i64>`、`project_id: i64`、`rank: f32`。

**Server 期望**：上述全部字段。

#### P1-5：AggregatedSearch 请求/响应类型不匹配

**请求差异**：
- CLI 的 `AggregatedSearchRequest.project_id: i64`（必填），Server 的 `project_id: Option<i64>`（可选）
- CLI 有 `file_extensions`、`entity_types`、`languages` 字段，Server 的 `AggregatedSearchRequest` 没有这些字段

**响应差异**：
- CLI 使用 `SearchResponse` 作为聚合搜索响应，但 Server 返回 `AggregatedSearchResponse`，额外包含 `sub_queries_count: usize`

#### P1-6：KeywordSearchItem 缺少字段

**现状**：CLI 的 `KeyWordSearchItem`（`cce-cli/src/types.rs` 第 668 行）包含 `{ score, file_path, title, start_line, end_line }`，缺少 `chunk_id: String`、`snippet: String`、`highlighted_snippet: String`。

**Server 期望**（或 Frontend 已实现的字段）：`{ chunk_id, file_path, score, snippet, highlighted_snippet }`。

#### P1-7：Config Reload 响应类型不匹配

**现状**：CLI 期望 `ConfigReloadResponse { success, message, project_root, processors_count }`，但 Server 返回 `{ success: bool, message: String }`。

**修复方案**：移除 `project_root` 和 `processors_count` 字段的反序列化期望。

#### P1-8：Health Embedding 响应类型完全错误

**现状**：CLI 期望 `EmbeddingHealthResponse { healthy: bool, provider_count: usize, providers: Vec<ProviderHealth> }`，但 Server 返回 `{ healthy: bool, model_name: Option<String>, message: String }`。

**Server 实际响应**（`crates/cce_server/src/api/handlers/health.rs`）：
```rust
pub struct EmbeddingHealthDetail {
    pub healthy: bool,
    pub model_name: Option<String>,
    pub message: String,
}
```

#### P1-9：Parse 响应中 `EntityInfo.id` 类型不匹配

**现状**：CLI 的 `EntityInfo.id: u32`，但 Server 的 parse 响应中实体 ID 是 `i64`。

---

### P2 — 功能缺失（Minor）

#### P2-1：缺少 Qdrant 进程管理命令

**Server 端点**：
```
GET  /api/qdrant/process/status
POST /api/qdrant/process/start
POST /api/qdrant/process/stop
POST /api/qdrant/process/restart
```

**建议**：添加 `qdrant process status|start|stop|restart` 子命令。

#### P2-2：缺少 Config Info 命令

**Server 端点**：`GET /api/config`

**建议**：添加 `config info` 子命令，显示当前配置信息。

#### P2-3：缺少 Config Validate 命令

**Server 端点**：`GET /api/config/validate`

**建议**：添加 `config validate` 子命令，显示配置验证结果。

#### P2-4：缺少 Summary 生成命令

**Server 端点**：`POST /api/summary`

**建议**：添加 `summary generate` 子命令，支持文件/目录路径输入。

#### P2-5：缺少 Aggregated Search 请求格式同步

CLI 的 `AggregatedSearchRequest` 包含 `file_extensions`、`entity_types`、`languages` 字段，但 Server 的 `AggregatedSearchRequest` 不包含这些字段。需要移除这些字段以匹配 Server 定义。

---

### P3 — 改进建议（Enhancement）

#### P3-1：缺少 `project_id` 查询参数的一致化

以下 CLI 命令缺少 `--project-id` 参数：
- `entity function`、`entity calls`、`entity callers`、`entity call-chain`、`entity call-path`、`entity inheritance`、`entity implementations`
- `watch start`、`watch stop`、`watch status`
- `storage stats`、`storage clear`、`storage delete-file`、`storage delete-entity`、`storage batch-delete`
- `tool compress`、`tool batch-compress`、`tool diagnose`、`tool symbols`、`tool references`、`tool definition`、`tool keyword-search`
- `config reload`
- `search query`

**建议**：统一在 CLI 根级别或命令组级别添加 `--project-id` 全局参数，避免每个子命令重复定义。

#### P3-2：响应反序列化统一使用 `serde_json::Value` 兜底

当前部分 CLI 命令（如 `storage stats`、`delete_file`、`delete_entity`）使用 `serde_json::Value` 手动解析，另一些使用强类型 struct。建议统一使用强类型方式，减少运行时错误。

#### P3-3：缺少 `QdrantProcessInfo.status` 的枚举类型

CLI 的 `QdrantProcessInfo.status: String` 应为枚举类型，对应 Server 的 `QdrantProcessStatus`：
```rust
pub enum QdrantProcessStatus {
    Idle,
    Starting,
    Running,
    Stopping,
    Crashed,
    Stopped,
    Failed(String),
}
```

---

## 3. Frontend 包问题分析

### 已修复问题（不再重复）

以下问题已在 Phase 0-2 修复：
- Entity API 路径添加 `{project_id}` 前缀 ✅
- Watch API 路径添加 `{project_id}` 前缀 ✅
- Entity ID 类型 `number` → `string` ✅
- 添加 `relation_epoch` 字段 ✅
- 添加 `function_id`/`class_id`/`interface_id` 字段 ✅
- Index Stats 添加 `project_id` 查询参数 ✅
- Clear Index 请求格式修正 ✅
- Incremental Index 添加 `project_id` ✅
- Config Reload 添加 `project_id` 查询参数 ✅
- Qdrant 进程管理 UI ✅
- 配置管理页面 ✅
- 摘要生成页面 ✅
- 项目管理页面 ✅

### P0 — 功能完全不可用（Critical）

#### FE-P0-1：Tools References/Definition 请求格式完全错误

**现状**：Frontend 的 `toolsApi.findReferences` 和 `toolsApi.getDefinition` 发送 `{ symbol, file: filePath }`，但 Server 期望 `project_id` + `path` + `line` + `column` 定位。

**Frontend 当前代码**（`frontend/src/lib/api/tools.ts` 第 122-127 行）：
```typescript
findReferences: (symbol: string, filePath?: string) =>
    apiClient.post('/api/tools/references', { symbol, file: filePath }),
getDefinition: (symbol: string, filePath?: string) =>
    apiClient.post('/api/tools/definition', { symbol, file: filePath }),
```

**Server 期望**：
```typescript
// POST /api/tools/references
interface FindRefsRequest {
    project_id: number;
    path: string;
    line: number;
    column?: number;
    symbol?: string;
    context_lines?: number;
    include_snippet?: boolean;
    include_entity_info?: boolean;
}

// POST /api/tools/definition
interface GotoDefRequest {
    project_id: number;
    path: string;
    line: number;
    column?: number;
    symbol?: string;
    include_body?: boolean;
}
```

**修复方案**：重写这两个 API 函数，接受 `project_id`、`path`、`line`、`column` 等参数。

#### FE-P0-2：Tools Symbols 请求格式完全错误

**现状**：Frontend 的 `toolsApi.getSymbols` 发送 `{ file, language }`，但 Server 期望 `{ project_id, paths }`。

**Frontend 当前代码**（`frontend/src/lib/api/tools.ts` 第 118-119 行）：
```typescript
getSymbols: (file: string, language?: string) =>
    apiClient.post<SymbolsResponse>('/api/tools/symbols', { file, language } as SymbolsRequest),
```

**Server 期望**：
```typescript
interface GetSymsRequest {
    project_id: number;
    paths: string[];
}
```

#### FE-P0-3：Tools 响应类型与 Server 不匹配

**现状**：Frontend 的 `findReferences` 和 `getDefinition` 没有定义响应类型（返回 `any`），且 Server 的响应格式为 `{ success, result, error, relation_info }`，与 Frontend 期望的格式不同。

**Server 响应格式**：
```typescript
interface FindRefsApiResponse {
    success: boolean;
    result?: FindReferencesResponse;
    error?: string;
    relation_info?: Record<string, unknown>;
}
```

#### FE-P0-4：AggregatedSearch 返回类型使用错误

**现状**：Frontend 的 `searchApi.aggregatedSearch` 返回 `SearchResponse`，但 Server 返回 `AggregatedSearchResponse`（额外包含 `sub_queries_count`）。

**Frontend 当前代码**（`frontend/src/lib/api/search.ts` 第 92-93 行）：
```typescript
aggregatedSearch: (request: AggregatedSearchRequest) =>
    apiClient.post<SearchResponse>('/api/search/aggregated', request),
```

**修复方案**：改为 `apiClient.post<AggregatedSearchResponse>`。

---

### P1 — 类型不匹配 / 缺少字段（Major）

#### FE-P1-1：SearchResultItem 缺少 `id` 字段

**现状**：Frontend 的 `SearchResultItem`（`frontend/src/lib/api/search.ts` 第 59 行）包含 `id: number`，但 Server 的 `SearchResultItem` 包含 `id: u64`。

**Server 期望**：`id: number` 类型正确，但需要确认字段是否存在。

#### FE-P1-2：EntitySearchResult 的 ID 类型应为 `string`

**现状**：Frontend 的 entity search 内联类型中 `id: number`、`file_id: number`、`parent_id: number`，但 Server 使用 stable symbol ID（字符串类型）。

**修复方案**：`id: number` → `string`，`file_id: number` → `string`，`parent_id: number` → `string`。

#### FE-P1-3：EntitySearchResult 缺少 `project_id` 字段

**现状**：Frontend 的 entity search 内联类型包含 `project_id?: number`，需要确认 Server 是否始终返回此字段。

---

### P2 — 功能缺失（Minor）

#### FE-P2-1：Tools 端点缺少 `project_id`

**现状**：Frontend 的多个 tools API 函数（compress、batchCompress、diagnose、getSymbols、findReferences、getDefinition、keywordSearch）没有一致地接受 `project_id` 参数。

**Server 期望**：所有 tools 端点（compress/batchCompress 除外）都需要 `project_id`。

#### FE-P2-2：缺少 `diagnose` 端点的 Server 确认

**现状**：Frontend 的 `toolsApi.diagnose` 调用 `POST /api/tools/diagnose`，需要确认 Server 是否实现了此端点。

---

### P3 — 改进建议（Enhancement）

#### FE-P3-1：统一 API 错误处理

当前 Frontend 的 API 调用结果中，部分函数检查 `success` 字段，部分直接使用返回数据。建议统一使用 `apiClient` 的错误处理机制。

#### FE-P3-2：Health API 响应类型确认

Frontend 的 `EmbeddingHealthStatus` 使用 `{ healthy, model_name, message }` 格式，与 Server 匹配。但需确认 `QdrantHealthStatus` 和 `Bm25HealthStatus` 的字段完整性。

---

## 4. 修复优先级和时间线

### CLI 包修复计划

| 阶段 | 问题 | 涉及文件 | 预估工作量 |
|------|------|----------|-----------|
| **P0** | Entity API 路径添加 `{project_id}` | `commands/entity.rs`, `cli.rs` | 2h |
| **P0** | Entity ID 类型 `u32` → `String` | `types.rs`, `commands/entity.rs`, `commands/storage.rs` | 1h |
| **P0** | Watch API 路径添加 `{project_id}` | `commands/watch.rs`, `cli.rs` | 1h |
| **P0** | 清除索引请求格式修正 | `types.rs`, `commands/storage.rs` | 0.5h |
| **P0** | 增量索引添加 `project_id` | `types.rs`, `commands/index.rs` | 0.5h |
| **P0** | Index Stats 添加 `project_id` | `commands/storage.rs` | 0.5h |
| **P0** | Config Reload 添加 `project_id` | `commands/config.rs` | 0.5h |
| **P0** | Compress 请求格式修正 | `types.rs`, `commands/tools.rs` | 1h |
| **P0** | Batch Compress 格式修正 | `types.rs`, `commands/batch_compress.rs` | 1h |
| **P0** | References/Definition 格式修正 | `types.rs`, `commands/tools.rs` | 1.5h |
| **P0** | Symbols 格式修正 | `types.rs`, `commands/tools.rs` | 0.5h |
| **P0** | 小计 | | **10h** |
| **P1** | Entity 响应添加 `relation_epoch` | `types.rs` | 0.5h |
| **P1** | Entity 响应添加 `function_id`/`class_id`/`interface_id` | `types.rs` | 0.5h |
| **P1** | SearchResultItem 添加 `id` | `types.rs` | 0.3h |
| **P1** | EntitySearchResult 补齐字段 | `types.rs` | 0.5h |
| **P1** | AggregatedSearch 类型同步 | `types.rs`, `commands/agg_search.rs` | 1h |
| **P1** | KeywordSearchItem 补齐字段 | `types.rs` | 0.3h |
| **P1** | Config Reload 响应类型修正 | `commands/config.rs` | 0.3h |
| **P1** | Embedding Health 响应类型修正 | `commands/health.rs` | 0.3h |
| **P1** | 小计 | | **3.7h** |
| **P2** | 添加 Qdrant 进程管理命令 | `commands/qdrant.rs`, `cli.rs` | 1.5h |
| **P2** | 添加 Config Info 命令 | `commands/config.rs` | 0.5h |
| **P2** | 添加 Config Validate 命令 | `commands/config.rs` | 0.5h |
| **P2** | 添加 Summary 生成命令 | `commands/summary.rs`, `cli.rs` | 1h |
| **P2** | 小计 | | **3.5h** |
| **P3** | 全局 `--project-id` 参数 | `cli.rs` | 1h |
| **P3** | 响应反序列化统一 | `types.rs`, `commands/*.rs` | 1.5h |
| **P3** | QdrantProcessStatus 枚举化 | `types.rs` | 0.3h |
| **P3** | 小计 | | **2.8h** |

### Frontend 包修复计划

| 阶段 | 问题 | 涉及文件 | 预估工作量 |
|------|------|----------|-----------|
| **P0** | Tools References/Definition 重写 | `api/tools.ts` | 1h |
| **P0** | Tools Symbols 重写 | `api/tools.ts` | 0.5h |
| **P0** | AggregatedSearch 返回类型修正 | `api/search.ts` | 0.3h |
| **P0** | 小计 | | **1.8h** |
| **P1** | EntitySearch ID 类型 `number` → `string` | `api/search.ts` | 0.3h |
| **P1** | 小计 | | **0.3h** |
| **P2** | 确认 Diagnose 端点的 Server 实现 | `api/tools.ts` | 0.5h |
| **P2** | 小计 | | **0.5h** |
| **P3** | Tools API 统一添加 `project_id` | `api/tools.ts` | 1h |
| **P3** | 统一错误处理 | `api/client.ts` | 1h |
| **P3** | 小计 | | **2h** |

---

## 5. 附录：Server API 完整清单

### 项目管理和索引
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/index` | POST | 全量索引 |
| `/api/index/incremental` | POST | 增量索引 |
| `/api/parse` | POST | 单文件解析 |
| `/api/index` | DELETE | 清除索引（需 `{ project_id }`） |
| `/api/index/file/{path}` | DELETE | 删除文件索引 |
| `/api/index/entity/{id}` | DELETE | 删除实体索引 |
| `/api/index/batch` | DELETE | 批量删除 |
| `/api/index/stats` | GET | 索引统计（需 `?project_id=`） |
| `/api/project` | POST | 创建项目 |
| `/api/project` | GET | 项目列表 |
| `/api/project/{id}` | GET | 项目详情 |
| `/api/project/{id}` | PUT | 更新项目 |
| `/api/project/{id}` | DELETE | 删除项目 |
| `/api/project/{id}/index` | POST | 项目索引 |
| `/api/project/{id}/reload` | POST | 重载配置 |
| `/api/project/{id}/config` | PUT | 更新配置 |

### 实体查询（项目级作用域）
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/project/{project_id}/function/{id}` | GET | 函数详情 |
| `/api/project/{project_id}/function/{id}/calls` | GET | 函数调用 |
| `/api/project/{project_id}/function/{id}/callers` | GET | 函数被调用 |
| `/api/project/{project_id}/call-chain/{id}` | GET | 调用链 |
| `/api/project/{project_id}/call-path` | GET | 调用路径 |
| `/api/project/{project_id}/class/{id}/inheritance` | GET | 类继承 |
| `/api/project/{project_id}/class/{id}/implementations` | GET | 类实现 |

### 搜索
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/search` | POST | 语义搜索 |
| `/api/search/aggregated` | POST | 聚合搜索 |
| `/api/entities/search` | POST | 实体搜索（FTS5） |

### Watch（项目级作用域）
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/project/{project_id}/watch/start` | POST | 启动监听 |
| `/api/project/{project_id}/watch/stop` | POST | 停止监听 |
| `/api/project/{project_id}/watch/status` | GET | 监听状态 |

### 配置管理
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/config` | GET | 配置信息 |
| `/api/config/reload` | POST | 重载配置（需 `?project_id=`） |
| `/api/config/validate` | GET | 配置验证 |

### Qdrant 进程管理
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/qdrant/process/status` | GET | 进程状态 |
| `/api/qdrant/process/start` | POST | 启动进程 |
| `/api/qdrant/process/stop` | POST | 停止进程 |
| `/api/qdrant/process/restart` | POST | 重启进程 |

### 存储
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/storage/status` | GET | 存储状态 |

### 摘要
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/summary` | POST | 生成摘要 |

### 工具
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/tools/compress` | POST | 单文件压缩 |
| `/api/tools/compress/batch` | POST | 批量压缩 |
| `/api/tools/diagnose` | POST | 代码诊断 |
| `/api/tools/symbols` | POST | 获取符号 |
| `/api/tools/references` | POST | 查找引用 |
| `/api/tools/definition` | POST | 跳转定义 |
| `/api/tools/keyword-search` | POST | 关键词搜索 |

### 健康检查和重试队列
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/health` | GET | 统一健康检查 |
| `/api/health/qdrant` | GET | Qdrant 健康检查 |
| `/api/health/embedding` | GET | Embedding 健康检查 |
| `/api/health/bm25` | GET | BM25 健康检查 |
| `/api/retry-queue` | GET | 重试队列状态 |
| `/api/retry-queue/process` | POST | 处理重试队列 |
| `/api/retry-queue` | DELETE | 清空重试队列 |

### 指标
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/metrics` | GET | Prometheus 格式指标 |
| `/api/metrics/json` | GET | JSON 格式指标 |
| `/api/metrics/history` | GET | 指标历史 |
| `/api/metrics/cleanup` | DELETE | 清理指标 |