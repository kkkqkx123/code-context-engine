# RepoWiki 数据供给：问题清单与分阶段整改方案

## 缘起与范围

`repowiki-data-supply-design.md` 的开发落地后，对 WCP（Wiki Context Package）导出与受限工具面做了一轮代码审查，识别出若干工程问题。本文件给出逐项根因、方案与实施阶段。

两项问题已单独立项、作为未来方案，不在本轮范围：

- `wiki-read` 只读 profile → `docs/plan/wiki-read-profile-and-budget-design.md`
- 有限步预算协议 → 同上

本轮遵循 `AGENTS.md`：无向后兼容包袱、文档用简体中文、避免 `unwrap`/`unsafe`、最小化动态分发、优先长期可维护性。

## 问题清单

| 编号 | 问题 | 位置 | 严重度 | 本轮处理 |
|---|---|---|---|---|
| A | `entities` 达到上限静默截断且无 `truncated` 标志，下游无法察觉数据不全 | `services/wiki.rs` 实体遍历 | 高 | 修复 |
| B | `include_mermaid` 无配置开关，只能靠请求参数或 CLI 关闭 | `cce-config/modules/wiki.rs`、`services/wiki.rs` | 中 | 修复 |
| C | WCP 导出在 async 函数内用同步 `std::fs::read_to_string` 读 snippet，阻塞 worker | `services/wiki.rs::read_project_file` | 中 | 修复 |
| D | `read_file` 路径防护只挡绝对路径与 `..`，未防符号链接逃逸项目根 | `services/tools.rs::resolve_project_file` | 中 | 修复 |
| E | `push_import_edge` 八个参数并以 clippy allow 放行，代码味道 | `services/wiki.rs` | 低 | 修复 |
| F | 实体目录只含函数类实体，缺类/结构/枚举等类型实体 | 关系索引 `function_index` | 中 | 设计声明（不改动） |
| G | WCP 文件清单与关系数据取自不同快照（双 epoch） | `services/wiki.rs::export_context` | 中 | 设计声明（不改动） |
| H | 图指标每次导出全量重算，无缓存 | `services/wiki.rs` + `graph_metrics` | 低 | 未来增强（不改动） |

## 逐项根因与方案

### A. `entities` 静默截断无标志

`export_context` 遍历 `function_index` 时用 `if entities.len() >= MAX_WCP_ENTITIES { return; }` 提前退出闭包，一旦达到 5 万条上限便静默丢弃剩余实体。而 `dep_graph`、`external_deps`、`clusters` 均带 `truncated` 字段，唯独实体目录没有，破坏了「数据包自描述是否完整」的一致性契约。

方案：在 `WikiContextResponse` 增加 `entities_truncated: bool`（默认序列化，因属关键完整性信号）；遍历中置位；同步更新 handler 与 CLI 无需改动（结构透传）。补单测覆盖「达到上限时标志为真且实体数不超过上限」。

### B. `include_mermaid` 无配置开关

`resolve_export_options` 对 mermaid 采用 `unwrap_or(true)` 硬编码默认，`WikiExportConfig` 无对应字段，导致无法在 `[wiki]` 配置中统一关闭，只能逐请求传参。

方案：`WikiExportConfig` 新增 `include_mermaid: bool`，默认 `true`（保持现状行为）；`resolve_export_options` 改为读取配置默认值。补配置解析单测。

### C. async 内同步读文件

`read_project_file` 使用 `std::fs::read_to_string`，在 `export_context` 这个 async 函数里同步阻塞；开启 snippets 时对每个文件串行读盘，会卡住 tokio worker。项目已有 `cce_utils::file::read_file_to_utf8_async`（`tokio::fs::read` + 编码探测）。

方案：将 snippet 提取从同步闭包中解耦——第一遍遍历仅收集实体基础字段与行范围，第二遍用异步读取按文件去重填充 snippet；`read_project_file` 改为异步版本。补相关单测。

### D. `read_file` 路径防护未防符号链接逃逸

`resolve_project_file` 仅拒绝绝对路径与 `ParentDir` 组件，未解析符号链接。若项目内存在指向根目录外的软链，调用方可越界读取。

方案：用 `tokio::fs::canonicalize` 解析目标与项目根，校验规范化的目标路径以规范化根为前缀（按路径组件比较，避免字符串前缀误判如 `/a/proj` 与 `/a/proj2`），再返回规范化路径供读取。保留既有的空路径、绝对路径、`..` 校验作为前置快速失败。补单测覆盖软链逃逸场景。

### E. `push_import_edge` 参数过多

`push_import_edge` 接收八个参数并标注 `#[allow(clippy::too_many_arguments)]`，可读性差且易在调用处错序。

方案：引入 `ImportEdgeContext` 结构体聚合只读输入（索引、已见边集合、输出边/节点/外部用途缓冲），函数签名为 `(&mut ImportEdgeContext, from_file, relation)`，移除 clippy allow。

### F. 实体目录缺类型实体（设计声明）

关系索引的唯一实体存储 `function_index` 只收录可调用实体（function/method/accessor 等），类/结构/枚举等类型实体不进关系快照。因此这不是 WCP 导出的 bug，而是索引层的既定边界。要让 WCP 携带类型实体需改动索引构建与符号表，波及面大且与「实体=可调用符号」的诸多下游假设冲突。本轮不改代码；若未来 RepoWiki 强依赖类型实体，应作为独立课题评估，并在设计文档中把 WCP 的 entities 明确标注为「callable entities」。

### G. 快照来源混合（设计声明）

`files`/`manifest` 取自元数据存储的 active generation（`data_epoch`），而 entities/dep_graph/metrics 取自关系快照（`relation_epoch`）。二者在并发重建索引时可能不同代。`WikiManifest` 已同时暴露两个 epoch，下游可据此判断；彻底统一需让文件清单也从关系快照导出，但元数据里的 `category`、`content_hash` 与关系快照的 `FileInfo` 语义不同，属于跨层改造。本轮不改代码；通过单测与文档明确「双 epoch 语义」，作为未来一致性课题。

### H. 图指标缓存（未来增强）

`compute_metrics`（PageRank + 中心性 + 聚类）与 `connected_components` 每次导出全量重算。缓存需按快照 epoch 失效并在 `SharedContext` 引入新状态层，属独立性能优化，本轮不引入，以控制风险。

## 实施阶段

| 阶段 | 内容 | 编号 | 验收 |
|---|---|---|---|
| 一 | 实体截断标志 + mermaid 配置开关 | A、B | 单测 + `cargo check -p cce-server-shared -p cce-config` |
| 二 | 异步读文件 + 路径规范防护 | C、D | 单测 + `cargo check` |
| 三 | 参数结构重构 + 排序与双 epoch 语义复核 | E、G | `cargo clippy` 相关 crate |
| 四 | 整体格式化与编译验证 | 全部 | `cargo fmt --check` + `cargo check --workspace` |

每个阶段结束即验证，失败则先修复再进入下一阶段。

## 实施状态

| 阶段 | 内容 | 状态 | 验证结果 |
|---|---|---|---|
| 一 | 实体截断标志 + mermaid 配置开关 | 已完成 | `cargo test -p cce-server-shared --lib services::wiki` 11 项通过；`cargo test -p cce-config --lib` 170 项通过 |
| 二 | 异步读文件 + 路径规范防护 | 已完成 | `path_guard_tests` 4 项通过（含符号链接逃逸拒绝） |
| 三 | 参数结构重构 + 排序与双 epoch 语义复核 | 已完成 | `cargo clippy -p cce-server-shared --lib` 改动文件无告警 |
| 四 | 整体格式化与编译验证 | 已完成 | `cargo fmt --check` 通过；`cargo check --workspace` 通过；`cargo test -p cce-server-shared --lib` 63 项全通过 |

F、G、H 三项按上文声明保留，分别作为索引层边界、一致性课题与性能优化的未来工作。
