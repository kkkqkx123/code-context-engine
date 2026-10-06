# 前端死信运维功能方案

后端配套改动见 `dead-letter-observability-and-manual-ops.md`（已实现：死信列表、单文件/批量重试、确认忽略、查询重试队列死信观测等 API）。本文档描述前端（`frontend/`，SvelteKit 5 + TypeScript + openapi-fetch）的对应改动。

## 技术栈与集成方式

- SvelteKit 5（Svelte 5 runes）、TypeScript、`openapi-fetch` 按契约调用后端。
- API 客户端类型来自 openapi 生成产物（`tools/openapi-codegen/openapi.json` → 前端类型）。新增后端端点后需先重新导出 openapi.json 并同步前端类型，再实现页面。

## 新增 API 依赖

| 端点 | 用途 |
|---|---|
| `GET /api/project/{id}/dead-letters` | 项目死信列表 |
| `POST /api/project/{id}/dead-letters/retry` | 全量/按 files 数组重试 |
| `POST /api/project/{id}/dead-letters/acknowledge` | 确认忽略（file_path + 可选 module） |
| `GET /api/retry-queue` | 查询重试队列 pending/dead 计数 |
| `GET /api/retry-queue/dead` | 查询死信条目列表 |
| `DELETE /api/retry-queue/dead` | 清空查询死信 |

## 页面结构

遵循"浏览在项目内、操作收敛到运维页"的划界原则：

```
/routes/ops/dead-letters/+page.svelte        索引死信运维页（独立页面，核心）
/routes/ops/retry-queue/+page.svelte         查询重试队列运维页
导航新增 "Ops" 分组：Dead Letters / Retry Queue 两个入口
```

不新增更多页面：两类运维各自一页即可；批量文件操作（重索引、删除等）未来若增多也归入 ops 分组，形成手动干预操作的单一归宿。

## 索引死信运维页 `/ops/dead-letters`

### 数据与状态

- 顶部项目选择器（复用现有项目列表 API）。
- 加载 `GET dead-letters`，得到 `DeadLetterFileEntry[]`（file_path、version、modules[]、updated_at）。
- 筛选：错误码 chip 多选、模块多选、未确认/已确认开关。
- 轮询：30s 间隔静默刷新（死信低频，无需 WebSocket），操作后立即刷新。

### 列表与操作

- 行内展示：文件路径、模块标签、retry_count、truncated/acknowledged 徽标、错误消息折叠。
- 行操作：
  - "重试此文件" → `dead-letters/retry` body `{ files: [path] }`，toast 展示 retried/succeeded/still_failed/truncated_chunks。
  - "忽略" → 确认弹窗；多模块死信时提供模块多选（不选 = 全部确认）→ `acknowledge`。弹窗文案须说明"忽略后不再参与自动/手动重试"。
- 批量操作（复选框 + 工具栏）：
  - 批量重试：一次调用 `dead-letters/retry` 传完整 files 数组。
  - 批量忽略：循环调用 acknowledge，聚合展示成功/失败分栏。
  - 危险操作（忽略/清空类）一律二次确认。
- 头部"全量重试"按钮调用空 files 的 `dead-letters/retry`（后端语义：空 = 全部候选）。

### 错误码 → 动作映射（前端常量表）

| error_code | 提示 | 主按钮 |
|---|---|---|
| `LLM_TOKEN_LIMIT_EXCEEDED_ERROR` | 嵌入输入超限，可截断后重试 | 重试 |
| 其他/空 | 确定性失败，重试大概率无效 | 忽略 |

映射表放 `src/lib/dead-letter-actions.ts`，供本页与项目内摘要复用。

## 查询重试队列运维页 `/ops/retry-queue`

- 展示 pending_count、dead_count（`GET /api/retry-queue`）。
- 死信条目列表（query、retry_count，`GET /api/retry-queue/dead`）。
- "清空死信"按钮 → `DELETE /api/retry-queue/dead`，二次确认。
- dead 条目提供"复制为搜索"快捷操作（直接跳转搜索页并填入 query），不做单条重放（后端无此语义）。

## 项目详情内嵌摘要（仅浏览）

- 项目索引状态区增加一个折叠块："死信 N 项"，列出文件路径与模块标签，附"去运维页处理"链接跳转 `/ops/dead-letters?project={id}`。
- 不在项目详情放置任何重试/忽略按钮。

## 实现清单

1. 同步 openapi 契约，生成/更新前端 API 类型。
2. `src/lib/api/dead-letters.ts`：上述端点的类型化封装。
3. `src/lib/dead-letter-actions.ts`：错误码映射常量。
4. `/routes/ops/dead-letters/+page.svelte`：列表、筛选、批量选择、行操作、轮询。
5. `/routes/ops/retry-queue/+page.svelte`：队列状态与死信列表。
6. 导航注册 Ops 分组；项目详情摘要块。
7. 共享确认弹窗与 toast 复用现有组件（若无可复用则新建最小实现）。

## 验证

- `npm run check`（svelte-check）通过。
- 手动验证：无死信空态、单文件重试、批量忽略、清空查询死信、30s 轮询不产生闪烁。
