# 11. 后端必要性判定与剩余任务落地方案

## 0. 摘要

本轮回答两个问题：**剩余问题里哪些真的要改后端**，以及**剩下的任务怎么做**。

结论先行：10 项遗留里只有 **1 项需要动后端契约**——分类关系明细目前以
`Vec<serde_json::Value>` 出网，OpenAPI 也明写 "untyped JSON array"，三端都没有可用的字段类型。
其余 9 项的 HTTP 端点、请求模型和响应模型**在后端已经齐备**：日志 6 端点、配置读/写/重载、
LLM 概览/更新/测试、检索的摘要字段、画布导出所需的全部数据。缺的都是桌面端（以及一处前端）
的消费代码，不该借"补功能"之名改服务端。

本轮落地：分类关系契约建模（后端唯一改动）+ 桌面日志面板 + 桌面配置写入 + 检索摘要卡片与
轻量高亮 + 命令面板 + 画布 PNG 导出与节点上限。离线审查保持不做，理由在第 9 章。

## 1. 范围与证据规则

- 后端 = `cce-api`（wire 契约）、`cce-server`（路由/handler/OpenAPI）。`cce-server-shared` 只在
  分类关系投影时被动适配，不改语义。
- 每条判定都给 `file:line`；"已有"指端点在 `router.rs` 注册、模型在 `cce-api` 定义、客户端有
  方法三者同时成立。
- 不新增 Cargo 依赖。画布导出复用 compograph 已有的软件光栅化器；代码高亮用项目内轻量分词器，
  不开 `syntect` 特性（体积与离线审查一并留作开放点）。

## 2. 后端必要性逐项判定

| # | 遗留项 | 后端要改吗 | 证据 |
|---|---|---|---|
| 1 | 桌面端日志面板 | **否** | 6 个 HTTP 端点已注册（`router.rs`），wire 模型在 `cce-api/src/models/logs.rs`，客户端方法在 `cce-client/src/logs.rs:21-61`，OpenAPI 已收录 |
| 2 | 分类关系明细 | **是** | handler 返回 `Vec<serde_json::Value>`（`classification.rs:30`），响应注解写死 "untyped JSON array"（`:23`），客户端 `entity.rs:131` 与前端 `entities.ts:180` 都是 `unknown`/`Value` |
| 3 | LLM 配置写入与单模型测试 | **否** | 路径 `paths.rs:50-52`，路由 `router.rs:110-118`，客户端 `project.rs:117/129/143`，请求模型 `config.rs:215/236` |
| 4 | 运行时配置编辑 | **否** | `GET/PUT /api/project/{id}/config` 在 `router.rs:105-107`，`ProjectConfigUpdateRequest/Response` 在 `config.rs:56/68`，客户端 `project.rs:79/91` |
| 5 | 摘要卡片 | **否** | `SearchResultItem` 已带 `content_state`、`expanded`、`forward_nodes`、`backward_nodes`、`contributing_sub_queries`（`search.rs:113-153`），无需新字段 |
| 6 | 代码高亮 | **否** | 高亮是本地呈现；不引新端点、不引新依赖 |
| 7 | 命令面板 / 逐页键盘 | **否** | 纯本地 UI，跳转目标已由 `navigator::Target` 建模 |
| 8 | 离线审查 / fixture | **否（但需架构拍板）** | 桩服务在 `cce-e2e-tests`，与桌面端进程隔离；不是后端契约问题，见第 9 章 |
| 9 | 画布 PNG 导出 | **否** | compograph 自带软件导出 `export_pixels`（`cg-render/src/export.rs:210`）与 `encode_png`（`cg-render/src/lib.rs:50`） |
| 10 | 画布节点上限 / LOD | **否** | 布局输入来自已有图答案；裁剪在桌面端完成 |

一句话：后端只有一个洞——**分类关系没有 wire 类型**。其余"后端改造"若要做，只会是把已有
端点重新包一层，属于重复劳动。

## 3. 唯一后端改动：分类关系明细

### 3.1 现状

服务端查询出的是 `ResolvedRelation`（`cce-types/src/types/relation/resolved.rs:69`），handler 用
`serde_json::to_value` 把整个内部结构原样吐出（`:45`）。后果有三：

1. 序列化形态由内部类型决定，内部加字段就是一次隐式 API 变更；
2. OpenAPI 无法描述，前端只能写 `Promise<unknown[]>`；
3. 桌面端即使想显示明细也没有可编译的字段访问。

### 3.2 方案

在 `cce-api` 增加一个**投影模型** `ClassificationRelation`，只保留明细该显示的字段：

- `caller_id` / `caller_stable_id` / `caller_name` / `caller_file_path`（来源与声明位置）
- `callee_id` / `callee_name`（目标）
- `relation_type` / `classification` / `external_target` / `is_external`
- `call_row` / `overload_signature` / `call_frequency`（调用点与调用形态）

`cce-api` 不依赖 `cce-types`（其 `Cargo.toml` 只有 serde/utoipa），所以映射必须写在服务端
handler：把 `ResolvedRelation` 显式投影成新模型，而不是让 serde 猜。这样内部类型演进不会
再直接改变出网形状。

变更点：

| 文件 | 改法 |
|---|---|
| `cce-api/src/models/entity.rs` | 新增 `ClassificationRelation`（`ToSchema`），字段全部是基本类型 |
| `cce-api/src/models.rs` | 导出新模型 |
| `cce-server/.../entity/classification.rs` | handler 签名改 `ApiResult<Vec<ClassificationRelation>>`；`#[utoipa::path]` 的 200 改 `body = Vec<ClassificationRelation>`；显式投影 |
| `cce-client/src/entity.rs` | `classification_relations` 返回 `Vec<ClassificationRelation>` |
| 两份 OpenAPI 快照 + `frontend/src/lib/api/schema.d.ts` | 重新生成并漂移守护 |
| `frontend/src/lib/api/entities.ts`、`stores/entities.svelte.ts` | `unknown[]` 换成生成类型，增加明细表 |
| `cce-desktop/src/services/entity_service.rs`、`views/entity_view.rs` | 增加分类筛选与明细表 |

**兼容性**：字段是 `ResolvedRelation` 已有信息的子集，旧消费者读不到新增的类型名但 JSON 形状
基本兼容；唯一变化是内部类型将来再加字段时不再出现在响应里——这是有意为之的契约收紧。

## 4. 桌面端日志面板（不改后端）

新增 `PanelKind::Logs`，归入运维组。服务层 `logs_service.rs` 包装已有的 7 个客户端方法；视图
`logs_view.rs` 提供：

- 顶部：级别/模块/关键字/页码筛选 + 自动刷新开关；
- 中段：日志表格与分页（用 `total` 判末页）；
- 底部：文件列表（下载 URL、删除二次确认）、轮转二次确认、日志配置读取与修改。

实时流选择**轮询**（`Cadence::Every(15s)`），不开 WebSocket：契约已备好轮询所需的一切，WS 要
新增依赖且前端 `RealtimeLogViewer` 的用法不是桌面端的必需能力（开放点 13 的结论）。

面板集合变化会让持久化布局失效，因此 `LAYOUT_VERSION` 从 1 升到 2；这是干净的失效而不是
迁移，符合 `dock_layout.rs` 既有约定。

## 5. 配置写入面板（不改后端）

在 `ConfigView` 保留现有"全局信息 / 校验 / 重载"三按钮，另加两个分区：

- **项目运行时配置**：`GET` 拿到 `config_version` 与 JSON 文档，文本编辑后 `PUT`；
  写入前二次确认，因为这是会被服务端热应用的写操作。
- **LLM**：`GET overview` 展示掩码后的 provider/model 状态；`default_chat`、
  `default_rerank`、`rerank_enabled`、`rerank_max_candidates` 可改；未加载概览时 rerank 开关
  发送空值，让服务端保持原值。对某个 model_key 发 `POST test` 做连通性探测。密钥永远来自响应里的
  `has_api_keys` 等标志，不在桌面端出现。

服务层包装放在 `ops_service.rs`（配置类命令已在此），不新建服务。项目 id 取
`AppState::active_project_id()`，无项目时按钮禁用并给出提示。

## 6. 检索卡片与代码高亮（不改后端）

`SearchView::render_result_item` 现在是"路径 / 实体 / 正文 / 参考标记 / 来源 / 动作"的平铺。
改成卡片：

> 头部：文件:行 + 分数 + 状态徽标（reference / expanded / 聚合命中）
> 正文：带轻量高亮的代码片段（最多 12 行）
> 底部：来源、节点数、动作按钮

高亮用新模块 `highlight.rs`：按语言族（由文件扩展名推断）做字符串/注释/关键字/数字的
正则分词，输出 `Vec<Span>` 给 `div().child(...)` 组合。不开 `syntect`：它已在组件库里作为
可选特性存在，但会显著增大二进制体积，且与前端 `utils/code.ts` 的自研分词器不对齐。

## 7. 命令面板（不改后端）

用 `gpui-kit::component::command::{Command, CommandState, CommandItem}`。条目集合来自
`PanelKind::ALL`（跳面板）与 `navigator::Target`（跨面板跳转），不新增目标类型；确认时通过
`navigator` 投递。挂在 `MainWindow` 的工具栏按钮上，不占全局快捷键（避免与 gpui 内建
键位冲突，留作开放点）。

## 8. 画布长尾（不改后端）

- **PNG 导出**：订正 10 文档"有 `encode_png` 接上即可"的说法——`encode_png` 只把像素缓冲
  编成 PNG，真正把 paint plan 画成像素的是 `cg-render/src/export.rs:210` 的 `export_pixels`
  与 `raster.rs` 的软件光栅化器。`GraphCanvas::export_png`（`graph/canvas.rs:144`）采集
  `ExportSnapshot`（node/edge styles、positions、texts、camera、viewport），以 2 倍缩放导出
  全图，写文件走导出服务已有的下载目录约定，结果路径由 toast 回报。
- **节点上限**：超出 `max_nodes`（默认 200）时按度数降序保留前 N 个及其关联边，检查器与提示
  行写明"已裁剪 x 个节点"。裁剪发生在 `Topology` 构造后、布局前，保证布局不会为不可见节点
  浪费计算。

## 9. 保持不做的事

| 项 | 理由 |
|---|---|
| 离线审查 / fixture 模式 | 需要决定是扩展 `cce-e2e-tests` 桩服务还是新建 dev crate；这是承载位置的选择，不是代码量问题。写进开放点 14 |
| `syntect` 特性 | 体积代价换来的语言覆盖超出本项目"检索结果预览"的需求；轻量分词器已覆盖前端同款场景 |
| 日志 WebSocket 流 | 见第 4 章；轮询足够，新增依赖不划算 |

## 10. 实施顺序与验收

1. 分类关系契约（后端唯一改动）+ 快照 + 三端消费：先跑 `cargo test -p cce-api -p cce-client -p cce-server openapi`。
2. 日志面板与配置写入：先加 `PanelKind`，再补服务/视图/注册/文案，`LAYOUT_VERSION` 一并升。
3. 检索卡片、高亮、命令面板、画布导出与裁剪。
4. 全量：`cargo test/clippy/fmt` 全工作区；前端 `check/contract:check/lint/test/build`。

### 10.1 落地结果（本轮）

| 项 | 落点 | 说明 |
|---|---|---|
| 分类关系契约 | `cce-api/src/models/entity.rs:261` → `cce-server-shared/src/services/relation.rs:703` → `classification.rs:24,42` | 后端**唯一**改动；投影由服务端显式构造，内部类型演进不再改变出网形状 |
| 三端消费 | `cce-client/src/entity.rs:132`、`frontend/src/lib/api/entities.ts:28,182`、`entity_view.rs:380` | 前端 `unknown[]` 与明细表空白一并补上；桌面新增分类输入与明细表 |
| 日志面板 | `services/logs_service.rs:20,76`、`views/logs_view.rs:35,213,286,318` | 新 `PanelKind::Logs`，15s 轮询、筛选分页、文件两处确认、配置热改；`LAYOUT_VERSION` 升到 2 |
| 配置写入 | `services/ops_service.rs:138,159,168,180`、`views/config_view.rs` | 项目配置 JSON 读/改（写入二次确认）与 LLM 概览/更新/测试；无新端点 |
| 画布 | `graph/model.rs:18,115`、`graph/canvas.rs:136,144`、`views/graph_view.rs:384` | 节点上限按度数裁剪并回报丢弃数；PNG 全图导出走软件光栅化器 |
| 检索 | `highlight.rs:50`、`views/search_view.rs` | 摘要卡片增加状态徽标，正文按行分词着色 |
| 命令面板 | `views/main_window.rs:289` | 条目来自 `PanelKind::ALL`，复用 navigator 跳转，无全局快捷键 |

### 10.2 验证

- `cce-desktop`：102 个单测全绿（本轮 +6），`clippy --all-targets` 与 `fmt --check` 干净。
- `cce-api` / `cce-client` / `cce-server`：测试全绿；两份 OpenAPI 快照（普通/admission）的
  漂移守护与路由一致性通过，前端 `contract:check` 78 条路径一致。
- 前端：`check`（svelte-check 0 error）、`lint`、`test`、`build`、`check:style` 全绿。

需要显示服务器才能验收的交互（画布取景与导出文件、命令面板键盘、日志轮询的真实往返、
配置写入后的热应用）只做了静态验证与单测，不假称手工验收通过。

## 11. 本轮新增开放点

| # | 问题 | 影响 |
|---|---|---|
| 14 | 离线 fixture 承载在 `cce-e2e-tests` 还是新 dev crate | 决定离线审查能否做；不影响后端 |
| 15 | 日志保留轮询，若将来要求秒级实时是否引入 WS | 目前 15s 周期；WS 客户端依赖待评估 |
| 16 | 命令面板是否绑定全局快捷键 | 本轮只做按钮入口，避免与 gpui 内建键位冲突 |
