# 前后端对照分析报告

> 分析日期：2026-07-19
> 对照范围：`crates/cce_server` (Rust 后端) ↔ `frontend` (SvelteKit 前端)

---

## 1. 严重等级定义

| 等级 | 说明 | 影响 |
|------|------|------|
| **P0** | 致命性 API 路径不匹配，运行时必定 404 | 功能完全不可用 |
| **P1** | 响应类型/字段不匹配，运行时可能解析错误 | 数据展示异常 |
| **P2** | 功能缺失、占位符页面 | 功能不可用 |
| **P3** | 设计/优化建议 | 非功能性 |

---

## 2. P0 — API 路径不匹配（运行时必定 404）

### 2.1 实体查询 API（7 个端点）

所有实体相关端点均缺少 `{project_id}` 路径段。

| 前端调用路径 | 后端实际路由 | 影响 |
|-------------|-------------|------|
| `GET /api/function/{id}` | `GET /api/project/{project_id}/function/{id}` | entityApi.getFunction() 失效 |
| `GET /api/function/{id}/calls` | `GET /api/project/{project_id}/function/{id}/calls` | entityApi.getCalls() 失效 |
| `GET /api/function/{id}/callers` | `GET /api/project/{project_id}/function/{id}/callers` | entityApi.getCallers() 失效 |
| `GET /api/call-chain/{id}` | `GET /api/project/{project_id}/call-chain/{id}` | entityApi.getCallChain() 失效 |
| `GET /api/call-path` | `GET /api/project/{project_id}/call-path` | entityApi.getCallPath() 失效 |
| `GET /api/class/{id}/inheritance` | `GET /api/project/{project_id}/class/{id}/inheritance` | entityApi.getInheritance() 失效 |
| `GET /api/class/{id}/implementations` | `GET /api/project/{project_id}/class/{id}/implementations` | entityApi.getImplementations() 失效 |

**影响范围：** 实体详情页、调用图、继承树、调用链 — 整个实体浏览功能完全不可用。

**文件：**
- [entities.ts](file:///workspace/code-context-engine/frontend/src/lib/api/entities.ts)（L93-L123）
- [router.rs](file:///workspace/code-context-engine/crates/cce_server/src/api/router.rs)（L83-L110）

### 2.2 Watch 文件监控 API（3 个端点）

所有 Watch 端点均缺少 `{project_id}` 路径段。

| 前端调用路径 | 后端实际路由 | 影响 |
|-------------|-------------|------|
| `POST /api/watch/start` | `POST /api/project/{project_id}/watch/start` | watchApi.startWatch() 失效 |
| `POST /api/watch/stop` | `POST /api/project/{project_id}/watch/stop` | watchApi.stopWatch() 失效 |
| `GET /api/watch/status` | `GET /api/project/{project_id}/watch/status` | watchApi.getStatus() 失效 |

**影响范围：** 文件监控页面功能完全不可用。

**文件：**
- [watch.ts](file:///workspace/code-context-engine/frontend/src/lib/api/watch.ts)（L21-L33）
- [router.rs](file:///workspace/code-context-engine/crates/cce_server/src/api/router.rs)（L126-L137）

### 2.3 存储统计 API（1 个端点）

| 前端调用路径 | 后端实际要求 | 影响 |
|-------------|-------------|------|
| `GET /api/index/stats`（无参数） | `GET /api/index/stats?project_id={id}`（必需 query 参数） | indexApi.getStats() 返回 400 |

**文件：**
- [index.ts](file:///workspace/code-context-engine/frontend/src/lib/api/index.ts)（L74）
- [storage.rs](file:///workspace/code-context-engine/crates/cce_server/src/api/handlers/storage.rs)（L544-L616）

---

## 3. P1 — 类型/字段不匹配（响应解析错误风险）

### 3.1 ID 字段类型不匹配：`number` vs `String`

后端实体相关 API 使用 **stable symbol ID**（`String` 类型），前端错误地定义为 `number`。

| 类型 | 后端定义 | 前端定义 |
|------|---------|---------|
| `FunctionCallsResponse.function_id` | `String` | `number` |
| `FunctionCallersResponse.function_id` | `String` | `number` |
| `CallChainNode.function_id` | `String` | `number` |
| `CallChainResponse.function_id` | `String` | `number` |
| `CallPathResponse.start_function_id` | `String` | `number` |
| `CallPathResponse.end_function_id` | `String` | `number` |
| `ClassInheritanceResponse.class_id` | `String` | `number` |
| `ClassImplementationsResponse.class_id` | `String` | `number` |
| `ClassRelation.class_id` | `String` | `number` |
| `InterfaceRelation.interface_id` | `String` | `number` |

**影响：** `handleNavigate(id)` 等函数使用 `String(id)` 转换，虽可运行但不准确。稳定符号 ID 是 `"snapshot-local:123"` 这样的字符串，而不是纯数字。

**文件：**
- [entities.ts](file:///workspace/code-context-engine/frontend/src/lib/api/entities.ts)（L31-L91）
- [models.rs](file:///workspace/code-context-engine/crates/cce_server/src/api/handlers/models.rs)（L229-L340）

### 3.2 缺少 `relation_epoch` 字段

后端 6 个实体相关响应都包含 `relation_epoch: i64` 字段，前端类型定义中完全缺失。

| 响应类型 | 影响 |
|---------|------|
| `FunctionCallsResponse` | 缺少 `relation_epoch` |
| `FunctionCallersResponse` | 缺少 `relation_epoch` |
| `CallChainResponse` | 缺少 `relation_epoch` |
| `CallPathResponse` | 缺少 `relation_epoch` |
| `ClassInheritanceResponse` | 缺少 `relation_epoch` |
| `ClassImplementationsResponse` | 缺少 `relation_epoch` |

**影响：** 运行时不会报错，但前端无法访问关系版本信息。

### 3.3 WatchStatus 字段名不匹配

| 字段 | 后端 (`WatchStatus`) | 前端 (`WatchStatus`) |
|------|---------------------|---------------------|
| 监控目录列表 | `watched_dirs: Vec<String>` | `dirs_watched: string[]` |

**影响：** 前端 `status.dirs_watched` 始终为 `undefined`。

**文件：**
- [watch.ts](file:///workspace/code-context-engine/frontend/src/lib/api/watch.ts)（L14-L19）
- [models.rs](file:///workspace/code-context-engine/crates/cce_server/src/api/handlers/models.rs)（L569-L581）

### 3.4 清除索引请求体不匹配

| 前端发送 | 后端期望 |
|---------|---------|
| `{ clear_vectors: bool, clear_bm25: bool, clear_relations: bool, clear_cache: bool }` | `{ project_id: i64 }` |

**影响：** 后端 `ClearIndexRequest` 只接受 `project_id`，前端发送的 `clear_*` 字段被忽略。后端始终清除所有后端，前端的选择性清除 UI 形同虚设。

**文件：**
- [index.ts](file:///workspace/code-context-engine/frontend/src/lib/api/index.ts)（L42-L48, L77-L80）
- [models.rs](file:///workspace/code-context-engine/crates/cce_server/src/api/handlers/models.rs)（L343-L372）

### 3.5 增量索引请求体不匹配

| 前端发送 | 后端期望 |
|---------|---------|
| `IncrementalIndexRequest`（无 `project_id`） | `IncrementalIndexRequest`（必需 `project_id: i64`，且 `files_to_index`/`files_to_remove` 为 `Vec<String>` 非可选） |

**文件：**
- [index.ts](file:///workspace/code-context-engine/frontend/src/lib/api/index.ts)（L30-L34）
- [models.rs](file:///workspace/code-context-engine/crates/cce_server/src/api/handlers/models.rs)（L700-L705）

### 3.6 索引响应体不匹配

后端 `handle_index` 返回的 `IndexResponse` 包含 `files_scanned`, `files_indexed`, `failed_files`, `total_entities`, `total_relations`, `total_vectors`, `message`, `errors` 等字段。前端 `indexApi.runIndex()` 调用后未处理响应，不影响功能但丢失了重要的索引状态信息。

---

## 4. P2 — 功能缺失/占位符

### 4.1 Qdrant 进程管理

后端提供 4 个 Qdrant 进程管理端点：
- `GET /api/qdrant/process/status`
- `POST /api/qdrant/process/start`
- `POST /api/qdrant/process/stop`
- `POST /api/qdrant/process/restart`

前端无任何对应页面或组件。存储页面仅显示进程状态（只读），无启动/停止/重启操作按钮。

### 4.2 配置管理页面

| 后端端点 | 前端状态 |
|---------|---------|
| `GET /api/config` | 占位符 "coming soon" |
| `POST /api/config/reload` | 未实现 |
| `GET /api/config/validate` | 未实现 |

[config/+page.svelte](file:///workspace/code-context-engine/frontend/src/routes/config/+page.svelte) 仅显示占位符文本。

### 4.3 摘要生成页面

| 后端端点 | 前端状态 |
|---------|---------|
| `POST /api/summary` | 占位符 "coming soon" |

[summary/+page.svelte](file:///workspace/code-context-engine/frontend/src/routes/summary/+page.svelte) 仅显示占位符文本。

### 4.4 项目管理功能

后端提供完整的项目管理 CRUD：
- `POST /api/project` — 创建项目
- `GET /api/project` — 列出项目
- `GET /api/project/{id}` — 获取项目详情
- `PUT /api/project/{id}` — 更新项目
- `DELETE /api/project/{id}` — 删除项目
- `POST /api/project/{id}/index` — 触发项目索引
- `POST /api/project/{id}/reload` — 重新加载项目配置
- `PUT /api/project/{id}/config` — 更新项目配置

前端 `projectApi` 中已定义上述所有方法，但**没有独立的项目配置管理页面**。当前只有 `index/+page.svelte` 页面可用于索引操作，但缺少项目创建、编辑、删除等功能界面。

---

## 5. P3 — 设计/优化建议

### 5.1 前端硬编码 `project_id`

前端搜索和索引操作中 `project_id` 硬编码为 `1`：

```typescript
// search.ts - L31
projectId: 1,  // 硬编码

// storage.ts - L69-L74
await indexApi.clearIndex({
  clear_vectors: true,
  clear_bm25: true,
  clear_relations: true,
  clear_cache: true,
});  // 缺少 project_id
```

建议：添加项目选择器，让用户选择要操作的项目。

### 5.2 ParseResponse 类型缺失

后端 `parseResponse` 包含 `encoding`, `relations` 等字段，前端 `ParseResult` 接口定义过于简单：

```typescript
// 前端定义
export interface ParseResult {
  entities: any[];
  language: string;
  file_path: string;
}

// 后端实际响应
pub struct ParseResponse {
  pub success: bool;
  pub file_path: String;
  pub language: String;
  pub encoding: String;
  pub entities: Vec<EntityInfo>;
  pub relations: Vec<RelationInfo>;
  pub elapsed_ms: u64;
}
```

### 5.3 错误响应格式不匹配

后端使用 `ErrorResponse` 格式（`{ success: false, error: { code, message, details? } }`），前端 `ApiClient` 从 `errorData.message` 提取错误信息，但未处理 `error.code` 和 `error.details`。

### 5.4 聚合搜索响应类型

前端 `searchApi.aggregatedSearch()` 返回类型声明为 `SearchResponse`，但后端 `AggregatedSearchResponse` 多一个 `sub_queries_count` 字段。虽然不影响主要功能，但类型不精确。

### 5.5 前端 `SearchRequest` 多余字段

前端 `SearchRequest` 接口包含 `file_extensions`、`entity_types`、`languages` 字段，但后端 `SearchRequest` 模型没有这些字段。后端使用 `exclude_patterns`、`include_patterns`、`exclude_content_types` 来过滤结果。前端发送的字段会被后端忽略。

---

## 6. 完整修正清单

### 必须修复（P0）

| # | 问题 | 修复方案 | 涉及文件 |
|---|------|---------|---------|
| 1 | 实体 API 缺少 `project_id` | 添加 `projectId` 参数到所有 entityApi 方法，修正路径模板 | entities.ts, entities store, entity detail page |
| 2 | Watch API 缺少 `project_id` | 添加 `projectId` 参数到所有 watchApi 方法，修正路径模板 | watch.ts, watch store, watch page |
| 3 | `GET /api/index/stats` 缺少 `project_id` | 添加 `projectId` 查询参数 | index.ts, storage store |

### 建议修复（P1）

| # | 问题 | 修复方案 | 涉及文件 |
|---|------|---------|---------|
| 4 | ID 字段类型 `number` → `String` | 更新所有实体相关接口的 ID 字段类型 | entities.ts, search.ts |
| 5 | 缺少 `relation_epoch` | 添加 `relation_epoch: number` 字段 | entities.ts |
| 6 | `dirs_watched` → `watched_dirs` | 修正字段名 | watch.ts, watch page |
| 7 | 清除索引请求体 | 修正为 `{ project_id }` 或添加通用清除端点 | index.ts, storage store |
| 8 | 增量索引请求体缺少 `project_id` | 添加 `project_id` 字段 | index.ts |

### 可实现（P2）

| # | 问题 | 修复方案 |
|---|------|---------|
| 9 | Qdrant 进程管理 UI | 在存储页面添加启动/停止/重启按钮 |
| 10 | 配置管理页面 | 实现配置查看/编辑/重新加载功能 |
| 11 | 摘要生成页面 | 实现摘要生成功能 |
| 12 | 项目管理页面 | 实现项目创建/编辑/删除功能界面 |

---

## 7. 分析结论

前后端存在 **3 个 P0 级问题**（API 路径不匹配，运行时必定 404），**5 个 P1 级问题**（类型/字段不匹配），**4 个 P2 级问题**（功能缺失）。最严重的问题是实体 API 和 Watch API 缺少 `{project_id}` 路径段，导致整个实体浏览和文件监控功能无法使用。

根因分析：后端在开发过程中将实体和监控 API 改为**项目级作用域**（添加 `{project_id}` 路径段），但前端未同步更新。建议在修正 P0 问题后，建立前端 API 调用与后端路由的自动一致性检查机制。