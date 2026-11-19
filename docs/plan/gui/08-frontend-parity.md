# GPUI 客户端与 Web 前端的功能差异分析与改进方案

## 0. 结论摘要

桌面端（`crates/app/cce-desktop`）不是"前端缺页面的移植"，两者已分叉出五种结构性差异：

1. **数据新鲜度模型不同**。前端有三条常驻轮询（健康 15s、指标 30s、监视 10s，见
   `frontend/src/routes/+layout.svelte:163-165`），桌面端除连接心跳外全部是"按一次、看一次"
   的手动快照（`crates/app/cce-desktop/src/services/connection_service.rs:18`）。
2. **跨面板导航能力不对等**。前端用 URL 承载可寻址状态（`/graph?entity=…`、`/entities/{id}`），
   桌面端只有一个 `search → graph` 的投递槽（`crates/app/cce-desktop/src/state.rs:60-78`）。
3. **能力面缺口集中在六处**：图形画布、配置写入、日志控制台、Qdrant 进程管理、批量工具、
   实体影响分析。其中配置写入、Qdrant、批量工具、实体影响四项对应契约层 14 个已存在但未被
   消费的操作；图形画布是组件能力未用，日志控制台则是契约本身缺失。
4. **契约层本身缺两块**：日志（服务端已实现、被显式排除在 OpenAPI 之外，
   `crates/app/cce-server/src/api/openapi.rs:218-225`）与结构化错误详情
   （`crates/app/cce-client/src/error.rs:211-220` 把 `details` 拼进字符串）。
5. **桌面端有前端没有的一项**：本地平台能力（中英双语、主题/密度、CSV 导出、托盘常驻、
   版本化停靠布局、令牌不落盘），这些不应为"对齐前端"而回退。

改进主张：**先补两个基础层（统一跳转、统一刷新），再补契约（日志、错误详情），最后按价值补能力**。
不按"前端有什么桌面就有什么"逐页对齐：图谱、配置与日志面板均已实现；仪表盘、
摘要独立页、日志文件管理三项建议不做（理由见第 9、10 章）。

## 1. 判定口径与证据规则

本方案只讨论**功能实现**，不评价视觉像素差异。差异分四类：

| 类型 | 定义 | 处理倾向 |
|---|---|---|
| 能力缺失 | 服务端契约存在，桌面端无入口 | 按价值补 |
| 形态不同 | 两端都能达成目标，路径不同 | 保留差异，写清理由 |
| 有意取舍 | 桌面端明确不做（如网关供应侧） | 维持并复核理由是否仍成立 |
| 平台特性 | 只有桌面端可提供 | 保持，不为对齐而削弱 |

证据规则：每条关于代码现状的陈述附 `file:line`；只引用可执行代码，不引用历史报告。
凡"报告说未做、代码已做"的条目，在第 2.4 节单独订正。

## 2. 两端现状

### 2.1 规模与形态

| 维度 | 桌面端 | 前端 |
|---|---|---|
| 代码规模 | 52 个源文件、11394 行（`crates/app/cce-desktop/src`） | 148 个文件、30990 行（含 7316 行 OpenAPI 类型与 1043 行 mock） |
| 入口 | 侧边导航 + 停靠面板，12 个面板（`views/navigation.rs:14-27`） | 侧边导航 + 路由，20 个页面、导航 17 项（`routes/+layout.svelte:51-99`） |
| 状态 | 每面板 `Load<T>` 四态（`services/request.rs:51-88`） | 按资源分 13 个 rune store，跨页共享 |
| 契约覆盖 | 84 个客户端操作中用 65 个 | 76 个类型化端点 + 6 处裸 `fetch` |
| 测试 | 60 个单测 | 3 个 Node 测试 + 5 个校验脚本 |
| 语言 | en/zh 双语，452 键 | 仅英文 |

### 2.2 面板对照

| 前端页面 | 桌面端对应 | 状态 |
|---|---|---|
| `/`（仪表盘） | 无 | 缺失 |
| `/search`（语义/实体/关键词三视图） | `Search` 面板（语义 + 聚合两模式） | 部分 |
| `/graph`（画布 + 过滤 + 检查器） | `Graph` 面板（9 个查询页签，表格呈现） | 形态不同 |
| `/entities` + `/entities/{id}` | `Entities` 面板（8 个页签） | 部分 |
| `/projects`（含索引控制、解析、维护、删除） | `Projects` + `Index` 两个面板 | 形态不同 |
| `/index`（重定向到 `/projects`） | `Index` 面板 | 桌面端更强 |
| `/summary` | `Index` 面板摘要页签 | 部分（结果退化） |
| `/storage`（含 Qdrant 进程） | `Storage` 面板（无 Qdrant） | 部分 |
| `/watch` | `Watch` 面板 | 部分（无 epoch） |
| `/health` | `Health` 面板 | 桌面端更强（含 admission 指标） |
| `/metrics` | `Metrics` 面板（3 页签） | 部分（无时间范围/项目范围） |
| `/tools`（8 个工具） | `Tools` 面板（7 个） | 部分 |
| `/config`（6 张卡片，可写） | `Config` 面板（只读 + 校验 + 重载） | 差异最大 |
| `/logs/*`（4 页） | 无 | 缺失 |
| `/ops/dead-letters`、`/ops/retry-queue` | `Tasks` 面板（合并） | 部分（无多选） |

### 2.3 契约消费差集

`cce-client` 共 84 个操作，桌面端消费 65 个（77%）。未消费的 19 个中：

- **5 个属网关供应侧**（`ingest_*`），按 `docs/plan/gui/04-views-and-services.md:29` 有意不做，理由仍成立。
- **14 个是真实缺口**，且前端全部有消费方：

| 操作 | 能力 | 前端证据 |
|---|---|---|
| `graph_entity_impact` | 实体级影响分析 | `frontend/src/lib/api/graph.ts:219`；桌面端已补实体影响页签，见 `12` |
| `classification_relations` | 分类关系明细 | `frontend/src/lib/stores/entities.svelte.ts:212` |
| `fold_batch` / `compress_batch` | 批量折叠/压缩 | `frontend/src/lib/api/tools.ts:79,96` |
| `get_project` / `update_project` / `update_project_config` | 项目基线读写 | `frontend/src/routes/config/+page.svelte:127,341,370` |
| `project_llm_overview` / `update_project_llm_config` / `test_project_llm` | LLM 配置读写与测试 | `frontend/src/routes/config/+page.svelte:174,263,287` |
| `qdrant_process_status/start/stop/restart` | Qdrant 子进程生命周期 | `frontend/src/routes/storage/+page.svelte:196-234` |

### 2.4 既有方案文档的过期条目（须订正）

`docs/plan/gui/07-stage3-4-report.md` 的"与方案的偏差"一节有四处已被后续代码推翻：

| 报告原文 | 代码现状 |
|---|---|
| 07:55-58 可停靠面板未做 | 已实现：`dock_layout.rs:34` 版本化布局，`main_window.rs:71-83` 装载与折叠 |
| 07:59-61 布局未版本化持久化 | 已实现：`dock_layout.rs:449-463` 读取并校验版本，`main_window.rs:171-210` 800ms 去抖落盘 |
| 07:62-63 托盘仍未落地 | 已实现：`tray.rs:49-56` 关闭决策，`app.rs:52` 初始化 `gpui_tray` |
| 07:64-66 表格导出未做 | 已实现：`export_service.rs:1-5` 网格 CSV 落盘 |

另有两条决策需随本方案重议：`04-views-and-services.md:25-26` 与 `05-implementation-plan.md:41`
均写"日志页不进桌面端，待契约层建模再议"。本方案第 7.3 节正是为其建模，故该结论转为待定
（开放点 2 已在第三轮收口：日志面板已落地）。

## 3. 差异总表

| # | 维度 | 桌面端现状 | 前端现状 | 判定 |
|---|---|---|---|---|
| 1 | 图谱呈现 | 表格 + `compograph` 画布双呈现（`graph/canvas.rs`） | Cytoscape 画布、布局切换、缩放、PNG 导出（`graph/+page.svelte:589-637`） | 已补：力导向/径向/网格、缩放、平移、取景、PNG 全图导出；节点上限按度数裁剪 |
| 2 | 图谱交互 | 选中高亮、一度邻接检查器、双击重定根（`graph_view.rs:814`） | 节点检查器、双击展开、域过滤、搜索高亮（`graph/+page.svelte:570-649`） | 已补交互三项；域过滤与搜索高亮仍在表格侧 |
| 3 | 检索分页 | 单次 `limit`，无 offset（`search_view.rs:36-37`） | 累计取数分页（`search/+page.svelte:73-93`） | 必须补 |
| 4 | 检索筛选 | 前缀、条数、排除测试、查询类型（`search_view.rs:300-348`） | 另含 min_score、patterns、categories、rerank、子查询权重（`FilterPanel.svelte:224-269`） | 应补 |
| 5 | 歧义符号 | 仅显示错误文本（`request.rs:27-39`） | 候选面板可点选重试（`client.ts:43`、`SymbolCandidatePanel`） | 必须补 |
| 6 | 实体关系筛选 | 参数硬编码（`entity_service.rs:139-150`） | 6 项筛选 + Apply 语义（`entities/[id]/+page.svelte:25-91`） | 必须补 |
| 7 | 跨面板跳转 | 仅 search→graph（`state.rs:186-209`） | 任意页 → 实体/图谱（URL） | 必须补 |
| 8 | 配置写入 | 全局信息/校验/重载 + 项目 JSON 读写 + LLM 概览/改/测（`config_view.rs`） | 6 卡片可写（`config/+page.svelte:476-560`） | 已补；密钥不回显，写入二次确认 |
| 9 | 日志 | 契约 + 客户端 + 桌面面板（`views/logs_view.rs`） | 4 页 + WS 实时流（`RealtimeLogViewer.svelte:20,53`） | 契约与面板已补；实时流定为 15s 轮询（开放点 15） |
| 10 | Qdrant 进程 | 无 | start/stop/restart（`storage/+page.svelte:196-234`） | 应补 |
| 11 | 批量工具 | 7 个（`tools_view.rs:36-45`） | 8 个（`tools/+page.svelte:41-49`） | 应补 |
| 12 | 死信批量 | 单条确认 + 全部重试（`tasks_view.rs:126-150`） | 多选批量（`dead-letters/+page.svelte:113-134`） | 应补 |
| 13 | 数据新鲜度 | 无轮询 | health 15s / metrics 30s / watch 10s | 必须补 |
| 14 | 失效提示 | 静态 epoch 文案（`graph_view.rs:688`） | epoch 比较后告警（`graph/+page.svelte:553-560`） | 应补 |
| 15 | 反馈通道 | 局部 notice，无全局提示 | Toast 常驻服务 | 应补 |
| 16 | 破坏性确认 | 无确认，按钮直连（`storage_view.rs:392-400`） | Modal 二次确认 | 必须补 |
| 17 | 代码呈现 | 纯文本前 12 行（`search_view.rs:265-272`） | 自研分词器高亮（`utils/code.ts:1-4`） | 应补，未做（开放点 5） |
| 18 | 键盘 | 全局 5 个快捷键（`app.rs:76-85`） | `/`、`j/k`、Enter（`search/+page.svelte:98-127`） | 可补 |
| 19 | 离线审查 | 需真实服务端 | `preview` 模式 mock（`preview-design.md`） | 应补 |
| 20 | 平台能力 | 双语、主题、密度、CSV、托盘、布局、令牌不落盘 | 无 | 保持 |

## 4. 关键差异逐项分析

### 4.1 图谱：表格能回答"有什么"，不能回答"连成什么样"

桌面端把 9 类图查询统一渲染成表格：邻域返回节点表与边表（`graph_view.rs:467-537`），
路径复用同一渲染（`graph_view.rs:302`），组件/环/影响/模块/结构各一张网格
（`graph_view.rs:304-436`）。这套实现的可取之处是查询面完整——9 个页签覆盖了契约的全部图操作，
比前端的 4 种种子模式更全。缺陷是**失去拓扑**：一个 200 节点的邻域在表格里是 200 行 + 400 行，
读者无法判断扇入扇出、孤立子图、环路的位置。

前端的对应实现是画布：`GraphCanvas.svelte`（500 行，Cytoscape）+ 工具栏
（`graph/+page.svelte:590-602`）+ 域过滤（`graph/+page.svelte:572-586`）+ 节点检查器
（`graph/+page.svelte:639-648`），双击展开一跳（`graph/+page.svelte:310-314`），
并按 `relation_epoch` 判定陈旧（`graph/+page.svelte:553-560`）。

关键事实：桌面端的设计目录**已经包含图谱节点色板**，但无消费方——
`design/tokens.json` 的 `nodes` 分区有 10 个变量，`design_tokens.rs:35-45` 已解析该分区，
`design_tokens.rs:183-184` 已有断言，注释明写"桌面端尚未渲染的分区"。这说明画布是既定方向，
只是未交付。

判定：**必须补，但分两步**。第一步交付"结构化邻域视图"（选中节点 + 一度邻接 + 边分组 +
可展开），第二步再评估自由画布（成本与依赖风险见第 10 章开放点 4）。

> 落地结果（后续一轮）：依赖探针一次通过，两步合并交付——表格保留，另加 `compograph` 画布，
> 选中节点 + 一度邻接 + 关系名 + 双击重定根都在画布上。见 `10-canvas-and-log-contract.md`。

### 4.2 检索：只有"第一页"

桌面端检索请求只有 `limit`，没有 offset，也没有累计取数（`search_view.rs:110-148`）；
`DEFAULT_LIMIT=10`（`search_view.rs:36-37`），用户只能改数字。后端无 offset 语义，
前端的做法是"第 N 页 = 取 N×pageSize 再前端切片"（`search/+page.svelte:73-93`），
并用 `total` 判断是否还有下一页（`search/+page.svelte:90-93`），而不是用已取行数——
这个细节桌面端完全缺失。

同时筛选面窄：桌面端只有 directory_prefix、limit、exclude_tests、query_type
（`search_view.rs:300-348`），聚合请求里 min_score、patterns、categories、rerank
全部硬编码为 `None`/空（`services/search_service.rs:94-137`）。实体检索同样没有 kind 过滤
（`services/search_service.rs:58-72` 的 `kind_filter: None`），而前端有
（`EntitySearchView.svelte:41-60`）。

判定：分页为**必须**（否则"共 300 条"不可达）；筛选项按 3 个高价值项补齐（min_score、
include/exclude_patterns、rerank），其余延后。

### 4.3 实体：参数被写死，导致关系查询少了一半能力

`entity_service.rs:139-150` 的 `chain_params` 把 `exclude_tests`、`directory_prefix`、
`domains`、`include_external` 全部固定为 `None/true`，只有深度与条数可调。
前端把同样 6 个字段做成了筛选条并要求显式 Apply（`entities/[id]/+page.svelte:25-91`），
因为这直接决定"调用者列表是否含测试代码、是否跨域"。

另外三处形态差异：

- 桌面端无"打开到图谱"入口，前端有（`entities/[id]/+page.svelte:280-299`）。
- 桌面端无分类关系明细，前端有（`EntitySearchView.svelte:270-330` 调 `classification_relations`）。
- 桌面端匹配列表最多展示 10 条（`entity_view.rs:297`），前端带 kind 过滤 + 可调 limit。

判定：关系筛选**必须**（属"结果正确性"而非"易用性"）；分类关系**应补**；跳转由第 7.1 节统一解决。

### 4.4 索引与摘要：入口绕过了项目登记，摘要丢正文

桌面端全量索引要求手填路径与扩展名（`index_view.rs:419-428`），并把 `exclude_dirs`、
`ignore_patterns` 写成空、`respect_gitignore` 写成 `true`（`services/index_service.rs:20-35`）。
这与项目记录冲突：`Projects` 面板已能读项目配置（`projects_view.rs:197-216`），
而前端明确以项目记录为唯一事实源（`frontend/src/lib/stores/index.svelte.ts:31-43` 注释
"The project record is the single source of truth"）。后果是：桌面端"索引当前项目"可能
扫到项目配置里排除的目录，产出与 Web 端不一致的索引。

摘要页签更明显：结果只渲染 文件/实体数/重要度 三列（`index_view.rs:323-371`），
正文、标签、warnings、失败原因全部丢弃；请求端也不带 extensions/exclude/gitignore
（`index_service.rs:80-97`）。前端是可展开卡片，含摘要正文、tags、main_entities、失败原因、
warnings（`summary/+page.svelte:195-260`）。

判定：索引入口**必须**改为以项目配置为准（面板输入降级为高级覆盖）；摘要呈现**应补**。

### 4.5 配置：只读面板 vs 可写控制台

桌面端配置面板只有三个动作：看配置、校验、重载（`config_view.rs:211-230`）。
前端有 6 张卡片：令牌、全局、校验、项目基线、LLM 模型、运行时配置
（`config/+page.svelte:476-560`），其中项目基线可改 root_path/extensions
（`ConfigProjectBasicsCard`）、LLM 可改并单模型测试（`ConfigLlmCard.svelte:80-353`）、
运行时配置可编辑 JSON 并保存（`ConfigRuntimeCard.svelte:54-92`）。

契约侧全部就绪（`get_project`、`update_project`、`update_project_config`、
`project_llm_overview`、`update_project_llm_config`、`test_project_llm` 均已在 `cce-client`），
桌面端缺的是服务方法与视图，不是契约。

判定：**必须补**——"改了配置要回浏览器"是桌面端作为本机运维工具的核心缺口。
但需先确认授权语义（第 10 章开放点 1）。

### 4.6 日志：两端共同的结构性缺陷

服务端日志能力完整：读日志、列文件、下载、删除、轮转、读写配置、WS 实时流
（`crates/app/cce-server/src/api/router.rs:218-284`）。但它**被显式排除在契约之外**：

> "Console-only log endpoints are also excluded: they live in `log_paths` and intentionally
> have no OpenAPI annotation." —— `crates/app/cce-server/src/api/openapi.rs:218-225`

结果是前端只能裸 `fetch` 绕过类型化客户端（`frontend/src/lib/api/logs.ts` 5 处），
桌面端则索性不做。这不是"桌面端缺页面"，而是**契约层留了一个洞**，两边各自买单。

判定：先补契约，再决定消费方（第 7.3 节）。桌面端日志面板已在第三轮落地，见 `11` 第 4 章。

### 4.7 运维：三处"服务端有、桌面端无"

| 能力 | 服务端/契约 | 前端 | 桌面端 |
|---|---|---|---|
| Qdrant 子进程 start/stop/restart | 已有（`cce-client/src/qdrant.rs`） | `storage/+page.svelte:196-234` | 无 |
| 死信多选批量重试/确认 | 已有（`dead_letters` 接收 files 数组） | `dead-letters/+page.svelte:113-134` | 只能单条确认（`tasks_view.rs:126-150`） |
| 指标时间范围 + 项目范围 | 已有（`MetricsHistoryQuery` 含 from/to/project_id） | `metrics/+page.svelte:35-39` | 只按"最近 N 小时"，无项目范围（`ops_service.rs:55-72`） |

桌面端在健康面板上反而更强：除统一探测还有三路诊断与 admission 统计
（`health_view.rs:120-165`），前端只有三张状态卡。

判定：Qdrant **应补**（本地开发场景刚需）；死信多选**应补**；指标范围**应补**。

### 4.8 数据新鲜度：三条轮询 vs 全手动

前端在布局挂载时启动三个单一轮询源（`+layout.svelte:163-165`）：健康 15s
（`stores/health.svelte.ts:134`）、指标 30s（`stores/metrics.svelte.ts:52`）、监视版本 10s
（`stores/watch.svelte.ts:80-85`）。桌面端只有连接心跳 30s
（`connection_service.rs:18,71`），其余面板必须点按钮。

这带来一个具体后果：图表面板读的是某个 epoch 的快照，而 `Watch` 面板的
`relation_epoch` 可能在用户操作期间已推进，桌面端没有任何比较逻辑
（`watch_view.rs:139-188` 只渲染 active/events/started_at），只在图谱底部放了一句静态说明
（`graph_view.rs:688`，对应文案 "Answers are annotated with the relation epoch they were served
from"）。前端在检索页与图页都做了比较并给出"结果可能过期"的告警
（`search/+page.svelte:236-248`、`graph/+page.svelte:553-560`）。

判定：**必须补**统一刷新层；其收益覆盖 health/metrics/watch/storage 四个面板，
并顺带解决失效提示（第 7.2 节）。

### 4.9 反馈与安全：破坏性操作无二次确认

`storage_view.rs:392-400` 的"清空"直接调用 `clear`，无确认；批量删除（440-443）、
项目删除（`projects_view.rs:385`）、指标清理（`metrics_view.rs:123-135`）同样直连。
组件库本身提供 `dialog::AlertDialog`（`gpui-kit/crates/component/src/dialog/`），
桌面端未启用。前端有 Modal 二次确认（`IndexDeletionPanel.svelte` 的 `pendingDelete`）。
同时桌面端没有全局提示队列，操作结果只写进各面板的 `outcome` 字段
（如 `config_view.rs:232-234`），跨视图中不可见。

判定：**必须补**——误点一次就丢索引，属正确性问题。

### 4.10 代码呈现与键盘

桌面端检索结果只取前 12 行纯文本（`search_view.rs:265-272`），无高亮、无行号锚点；
前端有零依赖分词器（`utils/code.ts:1-4,119-141`）并高亮查询词
（`ResultCard.svelte:117`）。桌面端具备条件：`gpui-kit` 提供 `syntect` 特性与
`SyntaxHighlighter`（`crates/gpui-kit/crates/component/src/highlighter/`），
但当前只启用了 `component`、`assets` 两个特性（`crates/app/cce-desktop/Cargo.toml:44`）。

键盘方面，桌面端只有 5 个全局快捷键（`app.rs:76-85`），前端检索页有 `/` 聚焦、`j/k` 选择、
Enter 打开（`search/+page.svelte:98-127`）。桌面端更适合做命令面板（组件库已有
`command` 模块），而非逐页快捷键。

判定：高亮**应补**（特性开关即可）；键盘**可补**，走命令面板。

## 5. 根因

1. **交付顺序按"面板清单"而非"基础能力"**。阶段 3-4 按业务面板推进
   （`docs/plan/gui/05-implementation-plan.md:3-18`），基础层只做了连接与设置，
   跳转、刷新、反馈三个横切能力留给了每个面板各自实现，于是每个面板都缺一点。
2. **单面板快照模型没有"重放"入口**。`Load<T>` 只描述一次请求的四种状态
   （`request.rs:51-88`），没有"用当前输入重放"的统一约定，所以无法被外部触发刷新。
3. **跨面板只有一条单向投递槽**。`GraphSeed` 是为"检索送图谱"定制的（`state.rs:60-66`），
   不是通用导航，导致实体、日志、任务之间无法互跳。
4. **契约层两个洞把成本推给消费方**。日志未建模 → 前端裸 `fetch`、桌面端不做；
   错误详情未结构化 → 前端手写 JSON 解析（`client.ts:43-75`）、桌面端只能显示字符串。
5. **组件库能力未被使用**。`dialog`、`notification`、`pagination`、`table(row_selectable)`、
   `chart`、`command`、`highlighter` 均在依赖树内，桌面端只用了其中一小部分，
   于是"补能力"看起来像"从零写"。

## 6. 改进原则

1. **先横切后纵向**：跳转与刷新是两个横切层，先做，之后每个面板的补齐成本骤降。
2. **不复制前端页面结构**：桌面端按"任务面"组织（查询分析 / 运维），不按 Web 路由搬。
3. **契约先行**：任何新能力所需类型先进 `cce-api`，桌面端不做裸 HTTP，不定义第二套 DTO
   （沿用 `docs/plan/gui/03-desktop-architecture.md` 的单向依赖）。
4. **不做动态分发**：新抽象用 enum 分派（AGENTS.md 的 `dyn` 约束），不用 trait object。
5. **不新增依赖优先**：能靠 `gpui-kit` 特性开关解决的（高亮、表格、对话框、分页）不引第三方。
6. **破坏性操作一律二次确认**，且确认文案说明影响范围。

## 7. 目标架构：四个基础层 + 两块契约补全

### 7.1 统一跳转（Navigator）

新增 `src/navigator.rs`：

- `enum Target`：`Entity { id }`、`GraphSeed { entity_id, label }`、`GraphFile { file }`、
  `SearchQuery { text }`、`DeadLetter { file }`、`LogTrace { .. }`（可选）。
- `struct NavRequest { target: Target, generation: u64 }`，放在 `AppState`，
  取代 `state.rs:60-78` 的 `GraphSeed` 专用字段。
- `MainWindow::sync` 消费请求：`Target → PanelKind`，打开面板后调用该面板的
  `on_navigate(&Target)`。每个视图实现这一入口，按 enum 分派，不用 trait object。
- 复用现有去重手法：单调 generation + 消费后取走（`main_window.rs:129-139` 已验证）。
- 切项目时丢弃未消费请求（与 `main_window.rs:116-127` 的重建规则一致）。

收益：一次改造同时补上 search→entity、entity→graph、entity→entity（调用链下游跳转）、
tasks→dead letter 四条链路。

### 7.2 统一刷新（RefreshScheduler）

新增 `src/services/refresh.rs`：

- `enum Cadence { Manual, Every(Duration) }`；每个面板注册一个 cadence。
- 单一后台循环（500ms tick）到期后向订阅面板发 `refresh` 信号；面板用自己的输入重放
  上一次请求。为此给每个面板补一个 `fn replay(&mut self, cx)`，即 `run` 的参数组装部分。
- 周期与前端对齐：健康 15s、指标 30s、监视 10s、存储状态（新增）30s、Qdrant 状态（新增）5s。
- 附加能力：`Watch` 面板拉到 `relation_epoch` 后写入 `AppState`，图/实体/检索面板比较自己的
  响应 epoch，不一致时在结果上方渲染"索引已推进，结果可能过期"告警并提供"重放"按钮
  （对齐 `graph/+page.svelte:553-560`），不做自动重载——避免打断正在进行的展开。

### 7.3 契约补全 A：日志

- `cce-api` 新增 `models/logs.rs`：`LogEntry`、`LogsQuery`（level/module/query/start/end/page/page_size）、
  `LogsResponse`、`LogFileInfo`、`LogConfig`、`LogRotateResponse`；`paths.rs` 增加 6 个常量。
- `cce-client` 新增 `logs.rs`：`logs`、`log_files`、`delete_log_file`、`rotate_logs`、
  `log_config`、`update_log_config`。
- 实时流：两种选择（第 10 章开放点 3）。短期用定长轮询（2s、取服务端允许的最近一批）
  按时间戳去重；若要真流式，需在 `cce-client` 引入 WebSocket 能力。
- 副作用（预期）：OpenAPI 快照变化 → 前端 `schema.d.ts` 重生成、`contract:check` 先红后绿；
  前端 `logs.ts` 的 5 处裸 `fetch` 收敛为类型化调用。这一步同时消掉前端的契约外请求。

> 落地结果（后续一轮）：已按本节完成。快照 72 → 78 条路径，前端实际是 6 处 `fetch` 收敛为
> 类型化调用（另 1 处下载 URL 与 WS URL 保留，均写明理由）；`204` 响应需要在传输层加
> "无响应体"助手，这是本节没预见到的。桌面端面板未做，见 `10` 第 9 章。

### 7.4 契约补全 B：结构化错误详情

- `cce-client::ClientError::Rejected` 增加 `details: Option<String>` 字段（当前被拼进 message，
  `error.rs:211-220`）；`Failure::from_error` 保留分类键，另挂 `candidates`。
- `cce-api::models` 增加 `SymbolCandidate { stable_id, file_path, scoped_name, kind }`，
  与前端 `client.ts` 的接口字段对齐；`cce-client` 提供 `symbol_candidates(&str) -> Vec<SymbolCandidate>`。
- 桌面端 `failure_block` 在候选非空时渲染候选按钮，点击走 7.1 的 Navigator。
- 单测覆盖：空 details、非 JSON、合法数组、字段缺失。

### 7.5 组件层复用（不引新依赖）

| 需求 | 复用 | 位置 |
|---|---|---|
| 破坏性确认 | `dialog::AlertDialog` | `gpui-kit/.../component/src/dialog/` |
| 全局提示 | `notification` / Toast | `.../component/src/notification.rs` |
| 分页控件 | `Pagination` | `.../component/src/pagination.rs` |
| 多选表格 | `TableState::row_selectable` | `.../component/src/table/state.rs:386` |
| 代码高亮 | `SyntaxHighlighter` + `syntect` 特性 | `.../component/src/highlighter/` |
| 命令面板 | `Command` | `.../component/src/command/` |
| 指标趋势 | `chart::{line,bar}` | `.../component/src/chart/` |

注意：`data_grid.rs:1-7` 目前明确拒绝组件库表格（只读、无选择、无虚拟化）。
引入多选后该结论只对"批量操作类表格"失效，其余面板继续用轻量网格，避免全量替换。

### 7.6 关键改动的接口形状

只给字段与签名，不给实现；目的是让评审能判断改动面，而不是让人照抄。

导航请求（新增，替换 `state.rs` 中的 `GraphSeed` 字段）：

```
pub struct NavRequest { pub target: Target, pub generation: u64 }
pub enum Target { Entity(String), GraphSeed { entity_id: String, label: String },
                  GraphFile(String), SearchQuery(String), DeadLetter(String) }
```

错误详情的契约扩展（`cce-client/src/error.rs` 的 `ClientError::Rejected` 增加一个字段，
其余变体不变；`details` 不再拼进 `message`）：

```
Rejected { .., details: Option<String> }
```

索引入口改为以项目记录为准（`index_service::full` 的签名收敛为项目 id 加可选覆盖）：

```
pub fn full(cx, client, project_id: i64, overrides: Option<IndexOverrides>)
```

## 8. 分阶段落地

| 阶段 | 内容 | 主要文件 | 验收 |
|---|---|---|---|
| P1 | Navigator + Confirm + Toast | `src/navigator.rs`、`components/{confirm,toast}.rs`、`views/main_window.rs`、各视图 `on_navigate` | 四条跳转链路可用；8 处破坏性操作均弹确认；成功/失败提示全局可见 |
| P2 | RefreshScheduler + epoch 告警 | `services/refresh.rs`、`views/{health,metrics,watch,storage}_view.rs`、`state.rs` | 无操作 60s 后健康/指标/监视自动更新；epoch 推进后图/实体页出现告警 |
| P3 | 错误详情 + 候选消歧 | `cce-api/models`、`cce-client/error.rs`、`services/request.rs`、`components/notice.rs` | 歧义符号在检索/图谱/实体三处可点选；4 条解析单测通过 |
| P4 | 配置写入 | `views/config_view.rs`、`services/ops_service.rs` | 项目 JSON 保存后版本前进；LLM 保存后 overview 反映新值；令牌不回显 |
| P5 | 图谱（画布）+ 检索分页/筛选 | `graph/{model,canvas}.rs`、`views/graph_view.rs`、`services/search_service.rs`、`views/search_view.rs` | 画布可选中节点并显示一度邻接；第 N 页累计取数；3 个新筛选生效 |
| P6 | 运维与工具补齐 | `views/{storage,tasks,metrics,tools,index}_view.rs`、`services/{ops,index}_service.rs` | Qdrant 三态可控；死信多选批量；指标含时间/项目范围；批量工具两页签；索引走项目配置；摘要可展开 |

顺序理由：P1/P2 是所有面板的公共分母；P3 让错误可操作且成本极低；P4 补齐"桌面端能否独立完成一次运维闭环"；
P5 是最大工作量，放在基础层稳定之后；P6 为长尾。

## 9. 逐面板目标形态

| 面板 | 目标 | 关键改动 | 不建议做 |
|---|---|---|---|
| Search | 语义 + 聚合两模式 + 分页 + 筛选补齐 + 候选消歧 + 高亮 | 累计取数；`min_score`/`patterns`/`rerank`；实体与关键词检索留在各自面板，经 Navigator 跳转 | 不把三视图塞进一个面板；不做 URL 深链 |
| Graph | 结构化邻域（选中/一度/边分组/展开）+ 9 类查询页签 + 域过滤 + epoch 告警 | 节点详情抽屉、边关系分组、复用 `nodes` 色板 | 首轮不做自由画布与 PNG 导出 |
| Entities | 8 页签 + 关系筛选条 + 跳图谱 + 分类关系明细 | 去掉 `chain_params` 硬编码；表格行跳转 | 不做继承树可视化（列表已够） |
| Index | 以项目配置为准 + 高级覆盖 + 摘要卡片 | `index_project` 为主路径；摘要正文/tags/warnings | 不新增独立摘要面板 |
| Projects | 列表 + 创建/编辑/删除（确认）/索引/配置读 | 补 `update_project`；删除确认 | 不做项目迁移 |
| Config | 全局（只读）+ 项目基线（可写）+ 运行时（可写）+ LLM（可写 + 测试）+ 令牌 | 新增两个服务与页签；掩码显示 | 不在桌面端做服务端 env 级配置编辑 |
| Storage | 状态 + 统计 + 删除（确认）+ Qdrant 进程 | 加 4 个 qdrant 操作与 5s 状态 | 不做文件级容量分析 |
| Watch | 路径/扩展/去抖 + 状态 + epoch | 补 `debounce_ms` 与 epoch 展示；10s 轮询 | 不做 live 事件流（服务端无此流） |
| Health | 统一探测 + 三路诊断 + admission | 15s 自动刷新 | 不重复前端卡片排版 |
| Metrics | 文本/JSON/历史 + 时间范围 + 项目范围 + 清理确认 | 补 from/to 与 project_id；列补 median/p90/max | 首轮不做图表（表格够用） |
| Tools | 8 个工具（补批量折叠/压缩） | 两个页签 + 批量参数 | 不做工具链编排 |
| Tasks | 死信多选 + 重试队列 + 清理确认 | 多选表格；确认框 | 不合并到健康面板 |
| Logs（若拍板） | 实时（轮询）+ 检索 + 文件列表 + 配置 | 依赖第 7.3 节契约 | 首轮不做下载、删除与轮转（契约方法可先建） |

## 10. 风险、取舍与待拍板开放点

### 风险

| 风险 | 触发点 | 缓解 |
|---|---|---|
| 跨第三个工作区引入图形库 | `compograph` 自带 workspace 与 `gpui-pre` 精确 pin，桌面工作区已因多工作区解析写了长注释（`Cargo.toml:11-21`） | 先做结构化视图；画布前先做一次依赖探针，失败则维持结构化 |
| 日志契约入 OpenAPI 导致前端检查先红 | `contract:check` 比对快照 | 同一提交内重生成 `schema.d.ts`，并在 PR 说明预期红转绿 |
| 配置写入的权限语义 | admission token 是否有写权限取决于服务端 | 先只读探测写接口能力，失败给出"无写权限"而非通用错误 |
| 刷新层引发请求风暴 | 多面板同时到期 | 单循环串行 + 每面板最小间隔 5s + 仅活动面板刷新 |

### 取舍

- **不做仪表盘**：其信息（在线状态、项目数、存储计数、最近查询）已在状态栏、健康、存储、
  检索历史四处存在，单独一页是 Web 的导航便利，不是桌面的信息缺口。
- **不做摘要独立面板**：单字段面板违背 `04-views-and-services.md:28` 的既定原则，卡片化即可。
- 日志文件删除与轮转已按开放点 2 的采纳结果落进日志面板（均二次确认）；下载仍以 URL 展示，
  不在桌面端实现流式下载。
- **不保存令牌**：前端存 localStorage（`auth.svelte.ts:26-50`），桌面端坚持内存态
  （`connection.rs:1-6`）。这是更安全的取舍，不因"对齐"回退。

### 待拍板开放点

1. **桌面端是否承担写入型运维**（项目基线、运行时配置、LLM、日志配置、Qdrant 进程）？
   **第三轮起视为是**：运行时配置、LLM、日志配置已做，均有确认或显式按钮；admission 写权限
   仍按服务端响应提示，不做本地猜测。
   若只做只读，P4 与 P6 的 Qdrant 部分取消，桌面定位退化为"观察器"。
2. **日志面板是否进桌面端**？**已定：进**。面板见 `views/logs_view.rs`，第 7.3 节的契约做法全部按计划执行。
3. **实时日志用轮询还是 WebSocket**？后者需新增依赖，与"不新增依赖"惯例冲突。
4. **图谱用结构化视图还是自由画布**？~~画布依赖 `compograph` 跨工作区解析，需探针结论。~~
   **已定**：探针通过（`compograph` 与 gpui-kit 同 pin `gpui-pre =0.3.10`，一份快照），画布已落地，
   表格模式保留。剩 PNG 导出与节点上限两个长尾（`10` 开放点 11–12）。
5. **是否引入 `syntect` 特性**？会显著增大二进制体积，换取代码高亮。
6. **死信多选是否替换轻量网格**？替换范围仅限 Tasks，还是全站统一表格。
7. **是否需要"可复制深链"**（如 `cce://panel/graph?entity=…`）？桌面无 URL，但协作场景可能要求。
8. **预览/fixture 模式放哪**：`cce-e2e-tests` 增加离线桩服务，还是新建 dev 专用 crate？
9. **桌面端是否显示"当前连接的是本机还是远端"**？影响写入型运维的风险提示。
10. **刷新周期是否允许用户配置**？默认对齐前端（15/30/10s），可配置会引入设置项与迁移。

## 11. 验证与文档维护

### 验证命令

```shell
# 桌面端（独立工作区，须用清单路径）
cargo test --manifest-path crates/app/cce-desktop/Cargo.toml
cargo clippy --manifest-path crates/app/cce-desktop/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path crates/app/cce-desktop/Cargo.toml --check

# 契约与前端（改动 cce-api 时必跑）
cd frontend && npm run contract:check && npm run check
```

### 单测清单（新增）

- Navigator：generation 单调去重、跨项目丢弃、目标到面板的映射唯一。
- Refresh：到期判定、最小间隔、串行不重叠、面板销毁后订阅清理。
- 错误详情：4 类 details 解析 + 候选为空时不渲染按钮。
- 确认框：取消不触发请求、危险操作必须携带确认。
- 索引：默认请求来自项目配置（根路径/扩展名/排除项三个断言）。

### 本次已执行的文档订正

| 文件 | 位置 | 改动 |
|---|---|---|
| `docs/plan/gui/07-stage3-4-report.md` | "与方案的偏差"之后 | 追加"现状订正"表，逐条指出四处偏差已被代码推翻，并指向本方案第 2.4 节 |
| `docs/plan/gui/04-views-and-services.md` | 25-26 | 日志结论由"不设面板"改为"暂不设，待契约建模后由开放点 2 决定" |
| `docs/plan/gui/05-implementation-plan.md` | 风险段 | 日志结论改为"取决于契约建模，见第 7.3 节" |
| `docs/plan/gui/README.md` | 文档地图 | 追加本文件与 07 的订正说明 |

后续维护规则：方案中每条"现状"都带 `file:line`，代码落地后应在同一次提交内改这里的现状描述，
而不是只在新文档里写"已更新"。若某条结论被推翻（如开放点改变），必须回到该条修改并注明日期。

### 面板验收口径

每个面板三项一次通过才算完成：无后端时给出空态与原因、有后端时完成一次真实往返、
故意触发一次失败时给出可读错误（不出现空白区域）。破坏性操作另加一项：取消后状态不变。
