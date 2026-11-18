# 实施任务清单与代码落地报告

## 0. 摘要

本文件是 `08-frontend-parity.md` 的落地版：把该方案的 P1–P6 拆成可勾选任务，记录每项的
改动位置、验收方式与当前状态，并订正方案里被实现过程推翻的假设。

本次实际落地 14 项（P1 全部、P2 全部、P3 全部、P4 部分、P5 两项、P6 五项）。未落地的集中在
三类：需要新依赖或跨工作区解析的（图形画布）、契约本身缺失的（日志）、以及依赖开放点拍板的
（LLM/运行时配置写入）。

验证结果：`cce-desktop` 84 个单测全绿（改动前 60 个），`cargo clippy --all-targets` 与
`cargo fmt --check` 无告警；契约层 `cce-api` / `cce-client` 测试全绿。未做的验证见第 6 章。

> 后续两轮已把第 3 章的 #1（画布）与 #2（日志契约 + 前端类型化）做完，桌面端单测涨到 96 个，
> 前端 `contract:check`/`check`/`lint`/`test`/`build` 全绿。做法与探针结论见
> `10-canvas-and-log-contract.md`，本文件相应条目已就地更新。
>
> 第三轮把 #2（桌面日志面板）、#3（LLM 写入/测试）、#4（运行时配置 JSON）、#5（分类明细）、
> #6（摘要卡片）、#7（轻量高亮）、#8（命令面板）以及画布长尾（PNG 导出、节点上限）全部落地，
> 桌面端单测 102 个，后端只因 #5 改了一次契约。详见 `11-backend-necessity-and-remaining-work.md`。

> 环境前提：本仓库要求 `rust-version = 1.97`，镜像里的 stable 工具链是 1.93，直接用
> `cargo` 会在 `gpui-pre` 上报 `unstable library feature cold_path`。所有命令都必须带
> `+1.97.0`，且工具链要从 `RUSTUP_DIST_SERVER=https://rsproxy.cn` 安装，默认源会卡住。

## 1. 交付口径

| 术语 | 含义 |
|---|---|
| 已落地 | 代码已提交到工作区，`check`/`test`/`clippy`/`fmt` 全绿，且有单测覆盖可测部分 |
| 部分落地 | 主路径可用，但方案的完整目标还缺一块，缺的那块在第 3 章单列 |
| 未落地 | 一行代码都没动，第 3 章写明阻塞原因 |
| 手工验收 | 需要有后端与显示服务器；本次未做，第 6 章列出清单 |

每条"已落地"都给出符号名而不只是行号：行号会随下一次改动漂移，符号名不会。

## 2. 已落地任务

### 2.1 P1-1 统一跳转（Navigator）

| 项 | 内容 |
|---|---|
| 目标 | 用一个通用目标替换只服务"检索→图谱"的单向投递槽，让任意面板能跳到任意面板 |
| 新增 | `src/navigator.rs`：`Target`（`navigator.rs:19`）、`NavRequest`（`:55`）、`Applied`（`:65`） |
| 改造 | `state.rs:182` `navigate`、`:192` `pending_nav` 取代原 `deliver_graph_seed`；`dock_layout.rs:394` `apply_nav` 按 `match` 分发到各视图的 `on_navigate` |
| 消费方 | `search_view.rs` 给每个命中加"打开实体"与"打开图谱"；无稳定符号 id 的文件级命中改送 `Target::GraphFile`；`entity_view.rs:221` 接收 `Target::Entity`；`graph_view.rs:153` 接收 `GraphSeed`/`GraphFile` |
| 去重 | 单调 generation + `Applied::accept`；换项目时把未消费请求标记为已消费（`main_window.rs:114` 起的 `sync`），避免把旧作用域的跳转重放到新面板 |
| 单测 | `navigator.rs` 三例：目标到面板的映射、generation 只消费一次、窗口晚于投递打开时不重放 |

与方案的一处收缩：方案列了 5 个 `Target` 变体（含 `Search`、`DeadLetter`）。实现时只保留
3 个真正有入口的（`Entity`、`GraphSeed`、`GraphFile`）。`Search`/`DeadLetter` 找不到自然
入口——硬造一个按钮只会增加噪音，所以删掉而不是留死代码。

### 2.2 P1-2 破坏性操作二次确认

| 项 | 内容 |
|---|---|
| 目标 | 误点一次不再丢数据 |
| 新增 | `src/components/confirm.rs`：`Confirm`（`:28`）、`request`（`:43`）、`confirm_bar`（`:71`） |
| 交互 | 第一次点击"臂化"并把按钮画成选中态，原位出现确认条（说明影响范围 + 确认/取消）；第二次点击才执行。换一个 key 会重新臂化，不会串行触发 |
| 覆盖 | 存储 6 处（清空、删文件、删实体、批量删除、Qdrant 停止、Qdrant 重启，见 `storage_view.rs:35-40` 的槽位常量）、项目删除、死信清理、指标清理 |
| 单测 | 4 例：首次臂化、二次执行、换 key 重新臂化、取消后需重新臂化 |

### 2.3 P1-3 全局提示队列

| 项 | 内容 |
|---|---|
| 目标 | 操作结果不再只写进各面板自己的 `outcome` 字段，跨面板可见 |
| 新增 | `src/components/toast.rs`：`ToastState::push`（`:34`）、全局 `push`（`:101`）、`overlay`（`:133`）、上限 4 条（`:18`） |
| 挂载 | `views/shell.rs:84` 在窗口根渲染浮层，因此换项目重建面板后提示仍在 |
| 清理 | 断开连接时清空（`connection_service::disconnect`），避免提示描述的是已断开的服务端 |
| 单测 | 3 例：id 递增、队列有界、按 id 删除 |

刻意不做自动消失：会自己消失的提示，正好是用户没读完的那条。

### 2.4 P2-1 统一刷新（RefreshScheduler）

| 项 | 内容 |
|---|---|
| 目标 | 让"实时面板"真的实时，替代前端的三条常驻轮询 |
| 新增 | `src/services/refresh.rs`：`Cadence`、`Schedule::with_defaults`（`:59`）、`Schedule::due`、`start`（`:137`）、下限 5s（`:30`） |
| 周期 | 健康 15s、监视 10s、指标 30s、存储 30s、任务 30s，其余显式登记为 `Manual`（漏登记的后果是永远不刷新，所以这张表必须穷举） |
| 驱动 | 单个 500ms tick 循环，串行派发，面板间隔 1ms 让出，避免到期撞车变请求风暴；只在连接状态为 `Connected` 时派发 |
| 面板侧 | 每个面板实现 `replay(&mut self, cx)`，用自己上次的输入重跑，不会覆盖用户正在编辑的字段：`health_view.rs:89`、`metrics_view.rs:151`、`watch_view.rs:87`、`storage_view.rs:148`、`tasks_view.rs:59`、`graph_view.rs:173`、`entity_view.rs:233` |
| 单测 | 4 例：手动项永不自动到期、未跑过的项立即到期且随后等满周期、低于下限被夹紧到 5s、默认表与前端周期一致 |

### 2.5 P2-2 结果过期告警

| 项 | 内容 |
|---|---|
| 目标 | 用 epoch 比较替代一句静态说明 |
| 新增 | `state.rs:201` `note_epoch`；`components/notice.rs:113` `stale_epoch` |
| 数据流 | 图/实体响应里的 `relation_epoch` 写入 `AppState`；面板渲染时与自己的 `served_epoch` 比较，落后则出横条并给"重新执行"按钮 |
| 范围 | 图 7 类答案、实体 6 类带 epoch 的答案。实体详情与分类统计在契约上无 epoch，因此不告警——猜一个值比不告警更糟 |
| 交互 | 不自动重载：自动重载会把用户刚展开的邻域收回去，所以刷新必须是显式动作 |

### 2.6 P3 结构化错误详情与候选消歧

| 项 | 内容 |
|---|---|
| 目标 | 歧义符号从"只能读一句错误"变成"点一下就能继续" |
| 契约 | `cce-api/models/response.rs` 新增 `SymbolCandidate`（服务端 `AMBIGUOUS_SYMBOL` 的 `details` 数组元素形状） |
| 客户端 | `cce-client/src/error.rs`：`Rejected` 增加 `details` 字段，`details` 不再拼进 `message`；新增 `details()` 与 `symbol_candidates()`（非法 JSON 返回空表，绝不把畸形 payload 当选项渲染） |
| 桌面侧 | `services/request.rs` 的 `Failure` 增加 `candidates`；`components/notice.rs` 的 `failure_block` 在候选非空时渲染可点按钮，点击直接 `navigate(Target::Entity)` |
| 单测 | 客户端 3 例（details 不入 message、非候选 JSON 返回空、无 body 无 details）＋ 桌面 2 例（候选拔出、普通拒绝无候选） |

OpenAPI 快照不受影响：没有 `schemas(...)` 显式清单，`SymbolCandidate` 也未被任何 path 引用，
因此不会进入文档。

### 2.7 P4 项目基线写入（部分）

| 项 | 内容 |
|---|---|
| 目标 | 桌面端能改项目基线，不必回浏览器 |
| 新增 | `project_service::update`；`projects_view::save_basics` |
| 语义 | 只发面板上有的字段（`name`、`extensions`），未触碰的字段留空表示"不改"，避免把排除列表清空 |
| 确认 | `projects_view` 的删除改为臂化执行 + 确认条 |

**方案订正**：08 写的是"改项目根路径后列表刷新一致"。实现时确认契约里 `UpdateProjectRequest`
没有 `root_path` 字段（服务端不在更新接口暴露根路径），所以这一条不可能按原描述实现。桌面端
按契约做的是改名与扩展名；根路径错了只能新建项目。此结论已写入本文件第 4 章。

### 2.8 P5 检索分页与筛选（部分）

| 项 | 内容 |
|---|---|
| 目标 | "共 300 条"可达；筛选面接近前端 |
| 分页 | `search_service::page`（`:87`）与 `Page`（`:65`）；视图侧 `page`/`fetch_size`/`go_to_page`。因为 wire 没有 offset，第 N 页按 `limit × (N+1)` 取回再切片，上限 200 |
| 是否还有下一页 | 用 `total` 判断，不用已取行数——服务端截断 `limit` 时后者会误报"没有更多" |
| 筛选 | 补齐 `min_score`、`include_patterns`、`exclude_patterns`、`rerank`；单一检索与聚合检索共用同一组字段（`AggregatedInput`），避免两条路径漂移 |
| 单测 | 3 例：翻页切片正确、越界页为空、`total` 决定末页 |

### 2.9 P5 实体关系筛选

| 项 | 内容 |
|---|---|
| 目标 | 调用者/被调用/调用链的筛选不再写死 |
| 新增 | `entity_service::ChainFilter`（`:28`）、`ChainRequest`（`:126`） |
| 修复 | 原先 `exclude_tests`/`directory_prefix`/`domains`/`include_external` 全部固定，意味着"调用者列表"一定含测试代码、一定跨域。现在由面板显式给出，默认值与旧行为一致（`include_external: true`），不会在用户没动之前悄悄收窄 |
| 视图 | `entity_view` 新增目录前缀、关系域、排除测试、含外部符号四个输入，只在 calls/callers/chain 三个页签显示 |
| 单测 | 3 例：默认筛选等价旧行为、收窄后逐字段到达 wire、空域列表表示"全部" |

`chain` 原本有 8 个位置参数，被 clippy 判为 `too_many_arguments`，已收成 `ChainRequest`。

### 2.10 P6-1 Qdrant 进程生命周期

| 项 | 内容 |
|---|---|
| 新增 | `storage_service::QdrantAction`（`:35`）与 `qdrant_action`（`:59`）、`qdrant_status` |
| 视图 | 存储面板新增区块：状态、归属（托管/外部）、刷新/启动/停止/重启；停止与重启需确认 |
| 刷新 | 状态纳入 30s 刷新；操作完成后立即回读一次 |
| 反馈 | 动作结果进 toast，失败保留在面板内的 outcome 行 |

### 2.11 P6-2 死信多选批量

| 项 | 内容 |
|---|---|
| 问题 | 轻量网格是只读的，没有行选择，批量操作无从下手 |
| 做法 | 死信列表改为可选行（`Button::selected`）＋ 全选/清空/重试所选/确认所选，保留网格用于只读汇总 |
| 服务 | `ops_service::acknowledge_dead_letters`（`:241`）：wire 一次只收一个文件，批量在客户端串行，遇错即停并报告已完成数量 |
| 确认 | "清空死信"改为臂化执行 |

刻意没有整体替换成全站组件库表格：`data_grid` 的只读约定对其余七个面板仍然成立，为两处批量
操作换掉全站表格不划算（08 §7.5 已预判这一点）。

### 2.12 P6-3 指标时间范围与项目范围

| 项 | 内容 |
|---|---|
| 新增 | `ops_service::HistoryInput`（`:57`）：`from`/`to` 显式覆盖 `hours`，`project_id` 收窄范围 |
| 视图 | 指标面板新增起止时间输入与"项目范围"开关；清理需确认 |
| 单测 | `HistoryInput::window` 的分支由既有调用路径覆盖；边界（只填一端）回落 `hours` |

### 2.13 P6-4 索引以项目登记为准

| 项 | 内容 |
|---|---|
| 问题 | 全量索引手填路径、`exclude_dirs` 写空、`respect_gitignore` 写死 `true`，可能扫进项目配置里已排除的目录，产出与 Web 端不一致的索引 |
| 新增 | `index_service::IndexDefaults`（`:26`）、`from_record`（`:37`）、`IndexOverrides`（`:80`）、`full`（`:89`） |
| 语义 | 默认值全部来自项目记录；面板字段降级为覆盖项，未填表示"用记录"；记录没有根路径时明确拒绝（`PROJECT_ROOT_MISSING`），不再猜一个路径 |
| 单测 | 4 例：默认来自记录、覆盖只替换所带字段、无根路径不可索引、只认匹配的记录 id |

### 2.14 P6-5 批量工具

| 项 | 内容 |
|---|---|
| 新增 | `tools_service::batch_items`（`:59`）、`fold_batch`（`:88`）、`compress_batch`（`:107`） |
| 视图 | 工具面板从 7 个页签扩到 9 个：批量折叠（`---` 分隔片段，首行 `# 名称` 命名）、批量压缩（每行一个路径） |
| 单测 | 3 例：分隔符切分、命名片段用名作 id、空输入不产条 |

## 3. 未落地任务

| # | 任务 | 状态 | 阻塞原因 |
|---|---|---|---|
| 1 | 图谱画布 | **已落地** | `src/graph/{model,canvas}.rs` + 图谱面板"表格/画布"双呈现；依赖探针通过（`compograph` 与 gpui-kit 共用 `gpui-pre =0.3.10`，图中只有一份快照）。详见 `10-canvas-and-log-contract.md` 第 2–4 章 |
| 2 | 日志契约与桌面面板 | **已落地** | 契约见 `10` 第 5 章；面板在 `views/logs_view.rs`，15s 轮询 + 筛选分页 + 文件确认 + 配置热改，`PanelKind::Logs`、`LAYOUT_VERSION=2` |
| 3 | LLM 配置写入与单模型测试 | **已落地** | `ops_service::{llm_overview,update_llm,test_llm}` + 配置面板分区；只走已有契约，密钥从不落到桌面端 |
| 4 | 运行时配置（JSON）编辑 | **已落地** | `GET/PUT /api/project/{id}/config` 已存在；面板加载、编辑、JSON 校验、二次确认后写入 |
| 5 | 分类关系明细 | **已落地（含后端唯一改动）** | `ClassificationRelation` 投影模型 + 服务端显式映射 + 快照 + 前端明细表；见 `11` 第 3 章 |
| 6 | 摘要卡片化 | **已落地** | 检索结果加状态徽标（内容状态/实体类型/扩展节点/子查询命中），正文按行分词 |
| 7 | 代码高亮 | **已落地（非 syntect）** | 自研 `highlight.rs` 轻量分词器，覆盖注释/字符串/数字/关键字；不开可选特性以控制体积 |
| 8 | 命令面板 / 逐页键盘 | **命令面板已落地** | `MainWindow::render_palette` 用组件库 `command` 模块，条目来自 `PanelKind::ALL`；全局快捷键未做（开放点 16） |
| 9 | 离线审查 / fixture 模式 | 未落地 | 承载位置（`cce-e2e-tests` 桩服务 vs 新 dev crate）是开放点 8 |
| 10 | 日志文件下载/删除/轮转 | **删除/轮转已落地** | 日志面板内两处二次确认；下载仍是 URL 展示（`log_file_download_url`），未接外部浏览器 |

## 4. 与 08 方案的偏差订正

| 08 原文 | 实际 | 处理 |
|---|---|---|
| §7.6 建议 `full(cx, client, project_id, overrides)` | 实现同形，但 defaults 由**服务端项目列表**推导，而非面板传路径 | 保留签名，语义写进 2.13 |
| §9 表格"Projects 补 `update_project`"，并举例改根路径 | `UpdateProjectRequest` 无 `root_path`；桌面端只能改 name/extensions/exclude_dirs/gitignore/patterns | 按契约实现，第 2.7 节记录 |
| §7.1 `Target` 五个变体 | 只保留三个有入口的 | 第 2.1 节说明删除理由 |
| §7.2 "图/实体/检索面板比较 epoch" | 检索响应没有 epoch 字段，无法比较 | 只做图与实体，检索不做（`graph_view.rs` / `entity_view.rs`） |
| §7.5 "多选表格用 `TableState::row_selectable`" | 改为可选行列表 | 见 2.11 的取舍理由 |
| §8 P1 验收"8 处破坏性操作均弹确认" | 实际 9 处（存储 6 + 项目 + 死信 + 指标），且为原位确认条而非模态框 | 交互形态不同，效果等价：确认时仍看得见作用范围 |

## 5. 决策记录

实现过程中替用户拍的小板（可在评审推翻）：

1. **确认用原位确认条，不用模态框**。模态框会遮住"要删的是哪个项目/哪批文件"，本轮选择让用户在
   看得见范围的情况下确认。若评审更看重"不可忽略"，可换 `dialog::AlertDialog`，组件已在依赖树内。
2. **刷新只重放、不自动重载**，且给每面板 5s 下限。理由：自动重载会打断正在展开的邻域。
3. **toast 不自动消失**，上限 4 条，连接断开时清空。
4. **索引不再接受"手填路径即索引"**，记录无根路径时直接报错。
5. **`Search`/`DeadLetter` 两个跳转目标删除**，不留无人调用的变体。

仍需拍板（沿用 08 §10 的编号）：

| 开放点 | 影响本轮哪一项 |
|---|---|
| 1 桌面端是否承担写入型运维 | 直接决定第 3 章 #3/#4 是否要做 |
| 2 日志面板是否进桌面端 | 决定 #2 是否要做 |
| 4 结构化邻域 vs 自由画布 | 决定 #1 的第一步形态 |
| 5 是否引入 `syntect` | 决定 #7 |
| 6 死信多选是否扩到全站表格 | 本轮只改了 Tasks |

## 6. 回归与验证

已执行：

```shell
cargo +1.97.0 test   --manifest-path crates/app/cce-desktop/Cargo.toml   # 84 passed
cargo +1.97.0 clippy --manifest-path crates/app/cce-desktop/Cargo.toml --all-targets
cargo +1.97.0 fmt    --manifest-path crates/app/cce-desktop/Cargo.toml
cargo +1.97.0 test   -p cce-api -p cce-client                            # 契约层全绿
```

未执行（环境所限，需在开发机补）：

| 项 | 原因 |
|---|---|
| GUI 手工验收（逐面板真实往返、画布七项） | 容器无显示服务器 |
| 索引/检索/Qdrant 的真实后端联调 | 无运行中的 CCE 服务端 |

前端与环境侧的验证已在后续一轮补齐（npm 源 `https://mirrors.tencent.com/npm/`，依赖已装）：

```shell
cd frontend && npm run check && npm run contract:check && npm run lint && npm test && npm run build
```

手工验收清单（按 08 §11 的口径，每面板三项）：

1. 无后端时给空态与原因，不出现空白区域。
2. 有后端时完成一次真实往返（本轮新增：Qdrant 启动/停止/重启、批量折叠/压缩、死信多选、指标范围）。
3. 故意触发一次失败时给出可读错误；歧义符号应出现候选按钮，点击后落到实体面板。
4. 破坏性操作取消后状态不变（本轮共 9 处需逐一点过）。
5. 不操作 60s，健康/指标/监视/存储应自动刷新（看请求日志即可确认）。

## 7. 下一步顺序建议

1. 用开发机跑一次第 6 章、`10` 第 11 章与 `11` 第 10.2 节的手工清单，把画布导出、日志轮询、
   配置写入、命令面板的偏差记回文档。
2. 开放点 1/2 已随本轮实现自然收口（桌面端承担写入、日志面板进桌面端）；如要回退，需同步移除
   `PanelKind::Logs` 与两个配置分区。
3. 离线审查的承载位置（开放点 14）仍需拍板；日志是否上 WebSocket（开放点 15）与命令面板全局
   快捷键（开放点 16）同理。
4. 画布布局位置持久化仍未做：`compograph` 有位置 IO，但没有"切换布局后恢复"的用户语义。
