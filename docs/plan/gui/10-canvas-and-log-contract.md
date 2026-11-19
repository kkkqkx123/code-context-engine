# 画布集成与日志契约落地报告

## 0. 摘要

本文件收尾 `08-frontend-parity.md` 里两个被"环境所限"挡住的条目，并记录实际做法：

1. **图谱画布已接入 `compograph`**（08 开放点 4 的探针结论：通过）。桌面端图谱面板新增"表格 / 画布"
   两种呈现，画布做力导向 / 径向 / 网格三种布局、平移缩放、选中高亮、一度邻接检查器、双击重定根。
2. **日志契约已建模**（08 §7.3）。六个 HTTP 端点进入 OpenAPI，`cce-api` 有 wire 类型、`cce-client`
   有对应方法、前端 `logs.ts` 的 6 处裸 `fetch` 收敛为类型化调用，`contract:check` 从 72 条路径涨到 78 条。
3. **Node 镜像已配置**并完成前端全量校验（`check` / `contract:check` / `lint` / `test` / `build`）。

验证结果：桌面端 96 个单测全绿（本轮 +12），`clippy --all-targets` 与 `fmt --check` 干净；
契约层 `cce-api` / `cce-client` / `cce-server` 测试全绿（含 OpenAPI 快照与路由一致性守护）；
前端 `svelte-check` 0 error、ESLint 无输出、21 个 Node 测试通过、静态构建通过隔离检查。

## 1. 范围与证据规则

本轮只做两件事，其余条目仍留在 `09-implementation-tasks.md` 第 3 章：

| 范围 | 判定 |
|---|---|
| 画布（09 #1） | 已落地，见第 2–4 章 |
| 日志契约（09 #2）与前端类型化（08 §7.3） | 已落地，见第 5–6 章 |
| 桌面端日志面板（09 #2 的另一半） | 已落地，见 `11` 第 4 章；下载 URL、自动刷新开关由 `12` 补齐 |
| 其余 8 项（配置写入、分类关系、摘要卡片、高亮、命令面板、离线审查…） | 由 `11` 与 `12` 分别收口 |

证据规则同 08：每条代码现状附 `file:line`，符号名优先于行号（行号会漂移，符号名不会）。

## 2. 依赖探针结论：通过

08 §10 把"跨第三个工作区引入图形库"列为风险，本轮先做探针再动代码。结论：

| 检查项 | 结果 |
|---|---|
| `compograph` 是否自带 workspace | 是，`crates/gpui-kit/crates/compograph/Cargo.toml` 是独立 workspace 根 |
| 能否作为 path 依赖跨 workspace 引用 | 可以。`cargo` 用该 crate 自己的 workspace 根解析 `workspace = true` 的继承项 |
| gpui 快照是否与 gpui-kit 同一版本 | 是。两边都精确 pin `gpui-pre =0.3.10`，图中只有一份快照，GPUI 类型可直接互通 |
| 是否引入额外编译负担 | 引入了 `petgraph 0.8`、`fixedbitset`、`hashbrown 0.15`；`cargo check` 冷启 39s，增量可忽略 |
| 结论 | 探针通过，08 开放点 4 的"画布不可行则维持结构化视图"分支不再需要 |

落地方式：`crates/app/cce-desktop/Cargo.toml:46-52` 直接按 path 引用五个子 crate
（`cg-graph` / `cg-interact` / `cg-layout` / `cg-render` / `cg-types`），并把原因写进注释：
`compograph` 与 gpui-kit 共用同一快照，所以不会出现两份 GPUI 类型。

## 3. 画布架构

### 3.1 分层

| 层 | 文件 | 职责 |
|---|---|---|
| 数据 | `src/graph/model.rs` | `Topology::from_answer`（`:53`）把一次答案的节点/边整理成可绘制的索引结构；`LayoutKind`（`:138`）跑布局；`hit_test`（`:194`）与 `fit_camera`（`:219`）是可单测的纯函数 |
| 绘制与交互 | `src/graph/canvas.rs` | `GraphCanvas`（`:53`）持有拓扑、位置、相机与指针状态；每帧把几何交给 `compograph` 的 paint plan |
| 面板集成 | `src/views/graph_view.rs` | `sync_canvas`（`:361`）、`render_canvas_panel`（`:781`）、`render_inspector`（`:814`）、`expand`（`:169`） |

不合成的部分：没有用 `GraphStore` 实体。`compograph` 的布局入口收 `AdjacencySnapshot`，
而它可以从"节点列表 + 边对"直接构造（`AdjacencySnapshot::from_parts`），所以桌面端不需要把
服务端答案再灌进一个可编辑图存储。收益是整条链路没有 `Entity<GraphStore>`、没有订阅、没有
后台 refine，纯数据进出，画布逻辑因此可以完全无窗口单测。

### 3.2 一份答案到一张图

`Topology::from_answer` 做三件在真实数据上必须做的事：

1. **两种键都要索引**。边的 `source`/`target` 在部分关系族里是稳定 id，在另一些里是标签，
   所以 id 与 label 都进索引表，先插入者优先。
2. **丢弃悬挂边**。服务端对节点列表与边列表分别截断，边可能指向被截掉的节点；不丢弃就会
   画到原点。
3. **合并平行重复边**。同端点同关系的多条边对布局与阅读都没有增量信息（`edge_id` 区分的是
   调用点），画布上合并成一条；反向边不合并。

节点度数单独算一次（`Topology::degrees`，`:111`），自环只算一度，用于节点尺寸。

### 3.3 布局

`LayoutKind` 三个族都来自 `compograph-layout`：力导向（默认）、径向、网格。切换布局会重算
位置并重新取景。刻意同步执行：答案受面板 `limit` 约束，后台 refine 只会让首帧更晚到。

### 3.4 视口

画布尺寸只能从布局结果拿到，所以采用"先量后画"：`on_children_prepainted` 把子元素边界写回
实体（`measure`，`:195`），下一帧用真实尺寸构图。首次进入与窗口缩放都会重新取景，避免图落到
视口外。

## 4. 画布交互

| 手势 | 行为 | 实现 |
|---|---|---|
| 拖动空白 | 平移 | `PanState` + `Camera::pan_by` |
| 滚轮 | 以指针为锚点缩放 | `wheel_zoom_factor` + `Camera::zoom_at` |
| 单击节点 | 选中并把非邻接边压暗、邻接边加粗并显示关系名 | `node_style`（`:222`）/ `edge_style`（`:260`） |
| 双击节点 | 以该节点为种子重跑邻域 | `CanvasEvent::Expand`（`:47`）→ 面板 `expand`（`graph_view.rs:169`） |
| 检查器按钮 | 打开实体面板 / 以该节点为中心展开 | `render_inspector`（`graph_view.rs:814`） |

画布只报意图、不碰网络与导航：双击发事件，面板决定怎么跑查询；"打开实体"由面板走
`navigator::Target::Entity`。这样画布不依赖 `ClientStore`，也不产生第二套导航语义。

配色复用共享色板：`design_tokens::packed`（`design_tokens.rs:130`）把 `nodes` 分区的
10 个令牌解析成 `compograph` 需要的 `0xRRGGBB`，节点按类型取填充与描边。这是 08 §4.1 指出的
"色板已解析但无消费方"的收口。

呈现方式默认仍是表格：画布是"看拓扑"的补充，不是替代。9 个查询页签全部保留，其中邻域、路径、
子图三类会把拓扑喂给画布，其余族不覆盖画布内容，避免切页签把用户正在看的图清空。

## 5. 日志契约建模

### 5.1 契约侧

| 位置 | 内容 |
|---|---|
| `cce-api/src/models/logs.rs` | `LogEntry`（`:13`）、`LogsQuery`（`:33`）、`LogsResponse`（`:52`）、`LogFileInfo`（`:65`）、`LogConfig`（`:78`）、`UpdateLogConfigRequest`（`:105`） |
| `cce-api/src/paths.rs:31-38` | 六个 HTTP 路径常量 + `LOGS_WS`（WebSocket，不入契约） |
| `cce-server/src/api/handlers/logs.rs` | 六个 handler 加 `#[utoipa::path]`（`:21`、`:68`、`:121`、`:158`、`:199`、`:216`、`:243`），改用 `cce_api::models` 类型 |
| `cce-server/src/api/log_paths.rs` | 由"自有常量"改为 re-export `cce_api::paths`，路由注册处不动 |
| `cce-server/src/api/openapi.rs` | 文档登记 7 个 handler 与 `Logs` 标签；`is_ignored_path` 不再排除 `/api/logs` |

两处刻意的取舍：

1. **不把 `cce_logging::LogEvent` 直接端出去**。它是日志 crate 的运行时类型，`cce-api` 不该依赖它；
   handler 里做一次字段搬运，契约层保留自己的 `LogEntry`。
2. **WebSocket 留在契约外**。`/ws/logs` 是推送通道，帧不是 HTTP payload；把它塞进 OpenAPI 只会
   产出一个没有调用方的 path。

### 5.2 快照与守护

`CCE_REFRESH_OPENAPI=1` 刷新了两份快照（`tools/openapi-codegen/openapi.json` 与
`openapi-admission.json`），各 +6 条路径。`openapi_snapshot_matches` 与
`routes_match_openapi_paths` 都通过：路由、注解、文档三者仍然一一对应。

### 5.3 客户端

`cce-client/src/logs.rs` 新增 7 个方法（`:21`–`:61`）。为了让 `204 No Content` 不再被当成解析
失败，`transport.rs` 增加 `send_no_content` 与 `post_no_content` / `put_json_no_content` /
`delete_no_content` 四个助手：成功但无响应体的调用不再走 JSON 解码。

`tests/contract.rs` 的"每个路径常量都有客户端方法"守护新增一个例外表 `PUSH_TRANSPORTS`，
只放 `LOGS_WS`，并在文件头写明理由——例外是登记过的，不是漏测。

## 6. 前端迁移

| 项 | 改动前 | 改动后 |
|---|---|---|
| `frontend/src/lib/api/logs.ts` | 6 处裸 `fetch` + 手写 `URLSearchParams` + 手写 `if (!res.ok) throw` | `call(client.GET(...))` 与 `client.DELETE/PUT/POST`，错误统一走 `ApiError` |
| 类型来源 | 手写 `interface LogEntry / LogsResponse / LogFileInfo / LogConfig` | `components['schemas'][...]`，由 `schema.d.ts` 生成 |
| 下载 | 裸拼接 URL | 保留拼接（附件不是 JSON payload），函数注释说明为什么它是例外 |
| 实时流 | `wsLogsUrl` | 不变，注释说明 WebSocket 无 HTTP 操作 |

`schema.d.ts` 用 `tools/openapi-codegen` 重新生成（78 条路径）。`npm run contract:check` 现在
校验的是"快照路径 ⊆ 生成类型"，从 72 涨到 78 且通过。

Node 侧环境：`npm` 源为 `https://mirrors.tencent.com/npm/`（可用），`node v22.13.1` /
`npm 11.9.0`。前端与 `tools/openapi-codegen` 两个依赖树都已安装，前者不影响后者的 TypeScript 5 约束。

## 7. 落地清单

| # | 项 | 文件 | 状态 |
|---|---|---|---|
| 1 | 依赖探针 | `cce-desktop/Cargo.toml` | 完成 |
| 2 | 拓扑索引与纯函数 | `src/graph/model.rs` | 完成，9 例单测 |
| 3 | 画布实体 | `src/graph/canvas.rs` | 完成，2 例单测 |
| 4 | 色板打包解析 | `src/design_tokens.rs:130` | 完成，1 例单测 |
| 5 | 面板集成（呈现/布局/缩放/检查器） | `src/views/graph_view.rs` | 完成 |
| 6 | 中英文案 | `messages/{en,zh}.json` | 本轮各 +16 键；后续累计 641 键，最新计数以 `12` 为准 |
| 7 | 日志 wire 类型 | `cce-api/src/models/logs.rs` | 完成 |
| 8 | 日志路径常量 | `cce-api/src/paths.rs` | 完成，纳入唯一性测试 |
| 9 | 服务端注解与文档登记 | `cce-server/src/api/{handlers/logs.rs,openapi.rs}` | 完成 |
| 10 | 客户端方法与无体响应 | `cce-client/src/{logs.rs,transport.rs}` | 完成，1 例单测 |
| 11 | 前端类型化 | `frontend/src/lib/api/logs.ts` | 完成 |

## 8. 与 08/09 的订正

| 原文 | 实际 | 处理 |
|---|---|---|
| 08 §4.1 / §9："第一步结构化邻域，第二步再评估画布" | 探针一次通过，直接做了画布；表格模式保留，等于两者都在 | 本文件第 2、3 章 |
| 08 开放点 4："画布依赖跨工作区解析，需探针结论" | 结论：可行，且不引入第二份 GPUI 快照 | 第 2 章 |
| 08 §7.3："`cce-api` 新增 `models/logs.rs`…`paths.rs` 增加 6 个常量" | 与实现一致；额外发现 `204` 响应需要传输层助手，08 未提 | 第 5.3 节 |
| 08 §7.3："前端 `logs.ts` 的 5 处裸 fetch" | 实际 6 处 fetch + 1 处 URL 拼接；URL 拼接保留 | 第 6 章 |
| 09 第 3 章 #1/#2 | 已移出未落地清单 | 09 同步更新 |
| 08 §7.2："`Search` 面板比较 epoch" | 契约里检索响应仍无 epoch，本轮未改契约 | 仍不做 |

## 9. 未做与开放点

仍未落地（继承 09 第 3 章，编号不变）：

- **桌面端日志面板**。已由 `11` 第 4 章落地（新 `PanelKind::Logs`、15s 轮询、文件确认与配置热改）；
  下载 URL 展示与自动刷新开关由 `12` 补齐。
- **配置写入**（项目基线已做、运行时 JSON 与 LLM 未做）——等 08 开放点 1。
- **分类关系明细、摘要卡片、代码高亮、命令面板、离线审查**——纯增量，未排入本轮。

本轮新增的开放点：

| # | 问题 | 影响 |
|---|---|---|
| 11 | 画布是否需要导出 PNG / 存布局位置 | **PNG 已落地**：`encode_png` 只编码像素，真正的软件光栅化在 `export_pixels`；见 `11` 第 8 章订正。布局位置持久化仍未做 |
| 12 | 画布节点上限 | **已落地**：复用面板 `limit`（默认 200），按度数裁剪并回报丢弃数；LOD/聚合仍留给后续 |
| 13 | 日志面板的实时流：轮询还是 WebSocket | **定为轮询**：15s 周期，复用刷新调度；WS 需新依赖，记为 `11` 开放点 15 |

## 10. 建议顺序

1. 用开发机跑一次第 11 章的验收清单，把画布与日志的偏差记回本文件。
2. 拍板开放点 2：日志面板进不进桌面端。进了就照第 5 章的契约直接做视图，无需再动服务端。
3. 拍板开放点 1：桌面端是否承担写入型运维。这是配置写入与 LLM 两组条目的共同前置。
4. 分类关系明细（09 #5）与摘要卡片（#6）可以随时插队，不依赖任何拍板。

## 11. 验收清单

画布（需要显示服务器 + 运行中的服务端）：

1. 邻域查询后切到画布，节点应完整落在视口内（取景正确）。
2. 三种布局切换后不出现节点堆叠在原点；切回表格再切回画布，图仍在。
3. 滚轮缩放的锚点是鼠标位置，不是视口中心。
4. 单击节点：非邻接边变淡，邻接边加粗并显示关系名；检查器出度数、出边、入边。
5. 双击节点：种子框被填成该节点，页签回到邻域，查询自动重跑。
6. 空答案或非拓扑答案切到画布：给出说明文案，不出现空白区域。
7. 窗口从窄拉宽：图重新取景，不留在左上角。

日志契约与前端（不需要显示服务器）：

1. `cargo test -p cce-server openapi` 两个守护测试通过。
2. `cd frontend && npm run contract:check` 报 78 条路径。
3. 打开 `/logs` 四个页面：检索、文件列表、配置读写、实时流各完成一次真实往返。
4. 故意用不存在的日志文件名删除：前端应显示服务端 404 的可读错误，而不是 `HTTP 404`。
5. 修改日志级别并保存后刷新配置：读到的是新值。
