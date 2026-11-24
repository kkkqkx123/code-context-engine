# Wiki 只读 Profile 与有限步预算协议设计

## 背景与动机

`docs/plan/repowiki-data-supply-design.md` 提出的双通道供给中，通道 B（受限工具面）依赖两项契约：**生成期工具必须只读**、**回查必须有限步**。当前实现只落地了「调用期白名单」（`McpConfig.enabled_tools` + `handler::ensure_enabled`）与「单次返回体上限」（各工具自带的 `limit`/`truncated`），存在三个缺口：

1. **只读约束靠人工枚举**：要得到只读面，下游必须手动写出「19 个工具减去 `index_project`、`incremental_index`」的名单。工具集一旦增删就漂移；且默认白名单为 `None`（全开），写操作默认暴露。
2. **无 profile 语义**：设计文档承诺的 `wiki-read` 只读画像在代码中不存在，文档自身也前后矛盾（一处要求建 profile，另一处称「不另设机制」）。
3. **预算完全靠下游自觉**：服务端没有 run 级、页面级的调用上限，无法阻止无界回查；也没有把剩余预算回传给生成器，下游无法据此降级。

本设计只解决这两件事：**一个可命名的只读 profile**，与**一套服务端强制的三级预算协议**。两者正交：profile 决定「能调哪些工具」，预算决定「能调多少次」。

## 设计目标与非目标

目标：

- 配置一行即可得到「只读工具面」，且该集合随工具集演进自动维护，不随新增工具漂移。
- 服务端强制执行「单次返回 / 单页面 / 单 run」三级上限，超限即降级，不返回数据。
- 每次成功响应附带预算元数据，下游可据此规划下一步。
- 同一 run 内重复回查不重复计步。
- 与现有 `enabled_tools`、`error_codes`、`truncated` 语义兼容，不破坏现有调用方。

非目标：

- 不在服务端理解「项目总览 / 模块页 / 架构图 / 阅读指南」的阶段语义；阶段划分归下游生成器，服务端只执行数值上限。
- 不实现 Wiki 文案生成、页面规划、渲染与导出（维持既有职责边界）。
- 不在本设计中改造 HTTP 工具面的鉴权模型；HTTP 对称化列为后续项。

## 总体架构

两个正交层叠加在现有 `McpServerHandler` 上：

```
tools/call(_meta.wiki_run)
      │
      ▼
[能力层] ensure_allowed(tool)          —— profile 能力位 + 显式白名单，双重校验
      │ 通过
      ▼
[预算层] BudgetTracker.try_consume()   —— 三级上限 + run 内缓存，命中缓存不扣步
      │ 授予
      ▼
[执行] 复用既有 services（read_file / graph / search ...）
      │
      ▼
[响应] payload 顶层附 _budget 元数据
```

- **能力层**：纯函数式判定，输入「工具名 + profile + 显式白名单」，输出允许/拒绝。可在无 IO 下单测。
- **预算层**：进程内状态（`DashMap`），按 `run_id` 隔离，带 TTL 清理；缓存复用项目已依赖的 `moka`。
- **分层归属**：能力表与 profile 解析放 `cce-config`（配置）与 `cce-mcp`（执行点）；预算计数核心放 `cce-server-shared`（纯逻辑、无 `rmcp` 依赖、便于单测与 HTTP 面复用），MCP 层只负责 `_meta` 解析与元数据注入。

---

## 第一部分：`wiki-read` 只读 Profile

### 能力模型：用能力表取代硬编码名单

不把「只读名单」写死，而是给每个工具声明一个能力位，profile 由能力位推导。这样新增工具时只需在能力表补一行，profile 自动获得正确归属，并有单测兜底覆盖率。

能力分两类：`Read`（只读查询，含元信息与无状态折叠）与 `Write`（会改变索引或项目状态）。

| 工具 | 能力 | 归属 wiki-read |
|---|---|---|
| `search` / `aggregated_search` / `file_search` / `keyword_search` | Read | 是 |
| `entity_callees` / `entity_callers` / `call_chain` | Read | 是 |
| `graph_module` / `graph_ego` | Read | 是 |
| `read_file` / `file_outline` / `entity_detail` | Read | 是 |
| `fold` / `fold_batch`（无状态） | Read | 是 |
| `list_projects` / `get_project` / `health` | Read | 是 |
| `index_project` / `incremental_index` | Write | 否 |

能力表以静态切片形式落在 `cce-mcp`（例如 `TOOL_CAPABILITIES: &[(&str, ToolCapability)]`），并配一条单测：**能力表的键集合必须与 `tool_router().list_all()` 完全一致**，任何新增/删除工具而漏改表都会让测试变红。这是防漂移的关键。

### Profile 解析规则

配置新增 `[mcp] profile`，取值 `full`（默认）与 `wiki_read`。解析为「有效白名单」的规则：

| profile | `enabled_tools` | 有效工具集 |
|---|---|---|
| `full` | `None` | 全部（保持现状，向后兼容） |
| `full` | `Some(list)` | 仅 list（现状行为） |
| `wiki_read` | `None` | 全部 Read 工具 |
| `wiki_read` | `Some(list)` | list 与 Read 集合的**交集**（更严格者胜） |

交点语义保证 `enabled_tools` 仍可在只读面上进一步收窄（例如只给 `read_file` + `file_search`），但永远无法越过只读边界去放开写工具。

默认取 `full` 是刻意的：不改变现有部署行为；只有显式配置 `wiki_read` 才启用只读约束。

### 执行点：调用期拒绝 + 工具列表过滤

两层落地，纵深防御：

1. **调用期拒绝（必做）**：把现有 `ensure_enabled` 扩展为 `ensure_allowed`，在显式白名单判定之外叠加能力位判定。拒绝时返回结构化错误 `tool_disabled`，与现有禁用语义同一通道。即使列表层漏过滤，调用也进不来。
2. **工具列表过滤（建议做）**：让 `tools/list` 只返回当前 profile 可见的工具，避免 LLM 看到写工具后浪费预算去尝试。实现上需自定义 `list_tools`（或在 `#[tool_handler]` 生成的列表结果上做后置过滤）；若 rmcp 宏不允许覆盖，则在 `get_info` 的 instructions 中明确声明「当前为只读 profile，写工具不可用」，并保留调用期拒绝作为兜底。

`get_info` 的 instructions 应回显当前 profile 与预算概要，便于客户端自查。

### 与现有白名单的关系

`enabled_tools` 保留原样，作为 profile 之上的可选细化；`profile` 是更粗粒度、语义化的一层。二者不冲突：profile 定义上界，`enabled_tools` 在上界内做下界收窄。

---

## 第二部分：有限步预算协议

### 三级预算模型

| 层级 | 作用范围 | 默认值（可配） | 超限行为 |
|---|---|---|---|
| 单次调用（per-call） | 一次工具返回的条目/行数 | 返回条目 500；读文件 2000 行 | 截断 + `truncated=true`，仍计 1 步 |
| 单页面（per-page） | 同一 `page_id` 的累计步数 | 20 | 拒绝并提示该页预算耗尽 |
| 单 run（per-run） | 同一 `run_id` 的累计步数 | 200 | 拒绝并提示整 run 耗尽，转 `wcp_only` 降级 |

单次上限沿用现有 `READ_FILE_MAX_LINES`、`graph_ego` 的 `limit`、`MAX_WCP_*` 等常量，本设计把它们收敛到一张 `ToolLimits` 表，并确保每个只读工具都返回 `truncated` 标志（现状 `read_file`、`graph_ego` 已有，`search` 系列需补齐）。单次截断不额外扣步，只记 1 步，避免「截断即惩罚」诱导下游放大 limit。

### run 上下文载体

预算按 `run_id` 计数，因此每次工具调用必须能识别所属 run。载体设计为主备两条路径：

- **主路径（推荐）：MCP `_meta`**。在 `tools/call` 的 `_meta` 下放 `wiki_run` 对象，含 `run_id`、可选 `page_id`、可选 `snapshot_epoch`。服务端通过 rmcp 的 `RequestMetaObject` / `RequestParamsMeta` 读取。优点：**完全不侵入 19 个工具的输入 schema**，下游无需为每个工具多加参数。
- **兜底路径：可选公共参数**。若当前 rmcp 宏生成的 `call_tool` 无法拿到 `_meta`，则在只读工具的参数结构上加 `run_id: Option<String>`、`page_id: Option<String>`（`#[serde(default)]`，schema 中可选）。对未传参的旧调用方等价于「无预算的一次性调用」，行为不变。

两条路径解析出的上下文归一为同一个 `WikiRunContext`，后续逻辑不区分来源。

### 计数与存储

进程内 `BudgetTracker`，按 `run_id` 隔离：

- 每个 run 维护：总步数计数、按 `page_id` 的分页计数、最近活跃时间戳。
- 并发安全用 `DashMap`（项目已依赖），计数用原子量，避免全局锁。
- 生命周期：惰性 TTL 清理（超过配置时长未触达的 run 被回收），并对活跃 run 数量设上限，防止 `run_id` 被恶意放大导致内存膨胀。
- `run_id` 做长度与字符白名单校验。

计数时机在**工具执行前**：`try_consume` 先检查三级上限，通过则占用 1 步并返回剩余额度，不通过直接返回预算耗尽，不进入服务层。这样超限调用不会产生任何查询开销。

### 预算元数据回传

每个成功响应在 payload 顶层附 `_budget` 字段，内容为：`run_id`、`calls_used`、`calls_remaining`、`page_calls_used`、`page_calls_remaining`、`snapshot_epoch`、`cached`（是否命中缓存）。这正是设计文档所称「作为工具响应元数据与生成器共享」的落地形态，下游据此决定继续回查还是转入降级。

### run 内缓存（重复回查不计步）

同一 run 内对**相同工具、相同规范化参数、相同快照 epoch** 的重复请求直接命中缓存，返回结果并标注 `cached=true`，**不消耗步数**。

- 适用工具：确定性只读工具（`read_file`、`file_outline`、`entity_detail`、`graph_module`、`graph_ego`、`keyword_search`、`file_search`）。语义搜索与 rerank 结果不稳定，不纳入默认缓存，或仅按参数缓存且不计入「免步」。
- 键设计：`(run_id, tool_name, canonicalized_args, snapshot_epoch)`。按 run 隔离，避免跨 run 预算串味；带 epoch，索引换代后自动失效。
- 容量与过期：复用 `moka`（项目已依赖）的容量上限 + TTL；缓存层本身不持久化。

### 降级与错误语义

- 预算耗尽返回结构化错误 `budget_exhausted`，携带 `remaining`（必为 0）与 `fallback: "wcp_only"` 提示。下游据此转为「仅用 WCP 数据 + 标注置信度」，不得重试。
- **失败不扣步，但计尝试次数**：工具执行报错（网络抖动、符号歧义等）不消耗正式步数，但累计 `attempts`；`attempts` 超过单页上限同样拒绝，防止以失败刷调用。
- 所有预算拒绝与现有工具错误走同一 `isError` 通道与机器可读 `code`，不新增传输层语义。

### 阶段化预算画像

服务端不理解阶段语义，但提供与设计文档四阶段对齐的**数值预设**，下游可按阶段切换 `max_calls_per_page`：

| 阶段 | 典型行为 | 建议 per-page |
|---|---|---|
| 项目总览 | 主要读 WCP，几乎不调工具 | 0–2 |
| 模块文档 | `file_outline` + `read_file` 验证关键声明 | 8–20 |
| 架构图 | 优先用 WCP 的 Mermaid 骨架，零工具 | 0–5 |
| 阅读指南 | `search` 补充主题入口 | 3–10 |

预设只影响配置默认值，不改服务端判定逻辑。

### 配置项

在 `[mcp]` 下新增（均可选，带安全默认）：

- `profile`：`full` | `wiki_read`，默认 `full`。
- `[mcp.budget]`：`enabled`（默认 `true`）、`max_calls_per_run`（200）、`max_calls_per_page`（20）、`max_return_items`（500）、`max_read_file_lines`（2000）、`max_active_runs`（如 256）、`cache_enabled`（true）、`cache_ttl_secs`（900）。

约束：`profile = wiki_read` 时 `budget.enabled` 被强制为 `true`，不可关闭——只读画像的意义即在于此。

---

## 第三部分：安全与治理

- 预算只作用于只读工具；写工具已被 profile 排除，双重保险。
- 计数按 `run_id` 隔离，`run_id` 校验长度与字符集，活跃 run 数量封顶，TTL 回收，避免内存放大。
- 单次返回体上限与 `truncated` 标志全工具统一，防止单次拉爆。
- `read_file` 路径防护沿用现有 `resolve_project_file`（拒绝绝对路径与 `..`），并建议补 `canonicalize` 校验最终路径仍在项目根内（防符号链接逃逸），与预算设计一并落地。
- 预算拒绝只回剩余数值与固定错误码，不泄露内部状态。

## 数据流时序

```
下游生成器                    CCE（MCP 面）
   │  tools/call(_meta.wiki_run)  │
   ├─────────────────────────────▶│
   │                              │ 1. ensure_allowed：profile 能力位 + 白名单
   │                              │ 2. budget.try_consume：run/page/单次三级校验
   │                              │ 3. cache.get(run, tool, args, epoch)
   │                              │      命中 → 直接返回（cached=true，不计步）
   │                              │ 4. 未命中 → 调用 services
   │                              │ 5. cache.put + 计数落账
   │  payload + _budget           │
   │◀─────────────────────────────┤
   │  remaining 用尽 → 转 wcp_only │
```

## 关键决策记录

1. **只读判定用能力表而非硬编码名单**：新增工具自动归类，单测锁定能力表与路由一致性，杜绝漂移。
2. **profile 默认 `full`**：不改变现有部署行为，显式配置才启用只读。
3. **run 上下文优先走 `_meta`**：不污染 19 个工具 schema；解析不到时退化到可选公共参数，两者归一为 `WikiRunContext`。
4. **预算在工具执行前拦截**：超限不产生查询开销。
5. **失败不扣正式步数但计尝试**：容忍瞬时错误，同时防止以失败刷调用。
6. **缓存按 `(run, tool, args, epoch)` 键控**：跨 run 隔离预算，索引换代自动失效。
7. **服务端只执行数值上限，不感知阶段语义**：保持职责单一与确定性，阶段划分归下游。
8. **`wiki_read` 下强制开启预算**：只读画像必须同时具备步数上限才有意义。

## 实施顺序与验收

实施顺序（每一步独立可测）：

1. 引入能力表 + `ToolProfile` 枚举 + profile 解析纯函数；`ensure_enabled` 扩展为 `ensure_allowed`。
2. 配置层新增 `profile` 与 `[mcp.budget]`，接入 `global.rs` 校验。
3. 预算核心：`WikiRunContext` 解析、`BudgetTracker`、三级计数、`budget_exhausted` 错误码。
4. run 内缓存与 `_budget` 元数据注入。
5. 工具列表按 profile 过滤（若宏可覆盖）与 instructions 回显。
6. 单测 + 集成测试；HTTP 面对称化列为后续。

验收标准：

- `profile=wiki_read` 下调用 `index_project`/`incremental_index` 返回禁用错误；`full` 下不受影响。
- `profile=wiki_read` 且 `enabled_tools=Some(["read_file"])` 时，仅 `read_file` 可用（交集语义）。
- 能力表键集合与 `list_all()` 完全一致（单测）。
- 超过 `max_calls_per_run` 返回 `budget_exhausted` 且 `remaining=0`、`fallback=wcp_only`。
- 相同参数重复调用命中缓存，`cached=true` 且不扣步。
- 每个成功响应带 `_budget` 元数据。
- `wiki_read` 下即使配置 `budget.enabled=false` 仍强制启用预算。
- 工具执行失败不扣正式步数，但 `attempts` 超限后拒绝。

## 影响面

| 区域 | 改动 |
|---|---|
| `crates/core/cce-config/src/modules/mcp.rs` | 新增 `McpProfile` 枚举、`McpBudgetConfig`，`McpConfig` 增字段 |
| `crates/core/cce-config/src/global.rs` | 接入 profile 与 budget 的结构化校验 |
| `crates/app/cce-mcp/src/handler.rs` | 能力表、`ensure_allowed`、`list_tools` 过滤、instructions 回显 |
| `crates/app/cce-mcp/src/`（新模块） | `profile.rs`（能力表与解析）、`budget.rs`（`_meta` 解析与注入） |
| `crates/app/cce-server-shared/src/services/`（新模块） | 预算计数核心与缓存（无 `rmcp` 依赖） |
| `crates/core/cce-types` | 新增 `error_codes::BUDGET_EXHAUSTED` |
| `crates/app/cce-mcp/src/server.rs` | 构造时传入 profile 与预算配置 |
| 测试 | 能力表覆盖、profile 解析、三级预算、缓存免步、元数据回传 |

## 风险与待确认

- **rmcp 宏覆盖能力**：`#[tool_handler]` 生成的 `list_tools`/`call_tool` 能否安全覆盖、能否在 wrapper 内取到 `_meta`，需在实现首步用最小样例验证；不能取到则走可选参数兜底，不阻塞整体方案。
- **缓存一致性**：缓存键含 `snapshot_epoch`，但 epoch 来自关系快照；若元数据与关系快照不同代（见上一份分析报告的一致性问题），缓存可能跨代命中，实现时需以实际快照 epoch 为准。
- **活跃 run 上限**：多租户/多下游并发时为防内存膨胀设上限，超限新 run 被拒绝并提示，需在下游文档说明。
