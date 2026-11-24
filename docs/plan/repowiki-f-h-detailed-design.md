# RepoWiki 后续问题 F–H 细化设计与实施记录

本文是 `repowiki-followup-remediation.md` 中 F–H 三项「设计声明/未来增强」的细化实施方案，并在文末记录实际落地状态。三项均在 `cce-server-shared::services::wiki::export_context` 这一条主链路内收敛，遵循 `AGENTS.md`：代码英文、文档简体中文、无裸 `unwrap`、静态分发、单测覆盖。

## 背景与共同约束

WCP（Wiki Context Package）是 CCE 向 RepoWiki 下游提供的**确定性数据**：文件清单、实体目录、文件级依赖图、外部依赖、图指标、聚类、阅读路径、Mermaid 骨架。职责边界不变——CCE 只供给数据，不生成 Wiki 文案，不调用 LLM。

三项问题的共同落点是 `export_context`。当前该函数的数据源有两个：

- **元数据 generation**（`manifest_active` → `data_epoch`）：提供 `files` 清单（路径、语言、category、content_hash）。
- **关系快照**（`RelationAccess.snapshot` → `relation_epoch`）：提供 callable 实体目录、依赖图、图指标、聚类、稳定符号 id。

## F. 实体目录缺类型实体

### 根因

关系快照的唯一实体存储是 `function_index`，只收录可调用实体（function/method/accessor 等）。`StableSymbolKey::for_entity` 与 `stable_id()` 在索引构建期只对这些实体登记符号键。因此 `index.for_each_function` 天然看不到 `class`/`struct`/`enum`/`trait` 等类型实体。

之前判定「改索引核心风险高」。本轮找到低风险数据通道：元数据存储的 `entities` 表由同一批解析实体写入（`EntityRecord` 含 `name`/`kind`/`scoped_name`/`overload_discriminator`/span/doc_comment/signature），且 `stable_id()` 是纯函数（schema 版本 + `file_path` + `scoped_name` + `kind` + `overload_discriminator` 的 SHA256）。因此可以从 meta 实体**复现出与关系快照一致的稳定符号 id**，类型实体同样获得下游 join 能力。

### 方案

1. `export_context` 在元数据 generation 上调用 `store.entities_with_files(project_id, data_epoch)`，一次取回该 generation 的全部实体及其文件路径（含类型实体）。
2. 筛选类型实体：`EntityKind` 反序列化自 `EntityRecord.kind` 字符串后，`kind.is_type_definition()` 为真者。用 `StableSymbolKey { file_path: normalize_project_path(path), scoped_name, kind, overload_discriminator }.stable_id()` 计算稳定 id。
3. 合并进现有 callable 实体目录，按稳定 id 去重（callable 实体 id 来自关系快照，类型实体 id 来自 meta 复现；同 id 保留关系快照版本，保证既有语义与 join 结果不变）。
4. 截断、排序、snippet 填充逻辑不变：类型实体同样走 `MAX_WCP_ENTITIES` 上限、`entities_truncated` 标志、`(file_path, id)` 排序、按行号窗口抽取 snippet（类型实体的 span 来自 `EntityRecord`）。

### 关键决策

- **增量补充而非整体替换**：callable 实体继续来自关系快照，其 stable id 天然对齐，零回归风险；类型实体仅从 meta 增补。避免把整个实体目录切换到 meta 导致的 join 语义漂移。
- **`kind` 解析**：`EntityRecord.kind` 是 snake_case 字符串，与 `EntityKind` 的 serde rename / `Display` 一致。用 `serde_json::from_value(Value::String(kind.clone()))` 反序列化，解析失败的类型实体跳过（不制造未知 id）。
- **`overload_discriminator`**：meta 记录已持久化该字段（`EntityRecord.overload_discriminator`），直接复用，保证复现的 stable id 与关系快照同类实体一致。

### 涉及文件

- `crates/app/cce-server-shared/src/services/wiki.rs`：新增类型实体收集与合并辅助函数；`export_context` 接入。
- 单测：新增 `stable_id` 复现确定性、类型实体筛选、去重与截断的纯逻辑测试。

## G. WCP 快照来源混合（双 epoch）

### 根因

`files`/`manifest` 来自元数据 active generation（`data_epoch`），而实体目录（F 之前）、依赖图、指标、聚类来自关系快照（`relation_epoch`）。一次索引重建期间两者可能不同代，下游拿到的包里 `files` 与 `entities`/`dep_graph` 描述的不是同一版代码。

`ProjectIndexManifest` 本身在同一条记录里带 `data_epoch` 与 `relation_epoch`——二者是**同一次 publication 的配对版本**，这是统一的数据锚点。

### 方案

1. `export_context` 只取一次 active manifest，并从它派生两个 epoch：`data_epoch`（meta 侧）与 `manifest_relation_epoch`。
2. 计算 `epoch_aligned = (access.snapshot.relation_epoch == manifest_relation_epoch)`：
   - 为真：`files`、`entities`（含 F 的类型实体，来自 data_epoch）、`dep_graph`/`metrics`/`clusters`（来自关系快照）描述同一 publication。
   - 为假：关系快照滞后于元数据。此时以关系快照为准上报 `relation_epoch`，并在 manifest 暴露 `epoch_aligned = false`，让下游可判定该包跨代、应选择重试或降级。
3. `WikiManifest` 新增：
   - `entity_epoch: i64`（实体目录与 files 所据的 `data_epoch`）——显式标注实体来源代，与 `index_epoch` 语义对齐并自描述。
   - `epoch_aligned: bool`（两个 epoch 是否同一次 publication）。
   - 保留 `index_epoch`/`relation_epoch` 原字段避免破坏既有消费者。
4. 所有从 meta 取的数据（files、类型实体）统一用 `data_epoch`，不夹杂 `files_visible_hashes` 等其它代的数据。

### 关键决策

- **不强行把关系数据搬到 data_epoch**：依赖图/指标/聚类的唯一数据源是关系快照，元数据 generation 不含这些信息。统一的目标是「同一 publication + 显式一致性标志」，而非虚构一个单一 epoch。
- **单测**：用纯函数 `epoch_aligned(relation_epoch, manifest_relation_epoch)` 覆盖对齐/不对齐两种情况，并验证 manifest 三字段的赋值关系（不依赖 IO）。

## H. 图指标计算无缓存

### 根因

`GraphService::compute_metrics()`（PageRank + 中心性 + 聚类系数）与 `connected_components()` 每次导出都基于 `searcher.query().index()` 全量重算。同一关系快照未变时（典型：多次 `wiki/context` 调用、MCP 与 HTTP 并发导出），结果完全相同，重复计算浪费 CPU。

### 方案

1. 在 `SharedContext` 增加进程内缓存字段 `wiki_graph_cache: WikiGraphCache`。`WikiGraphCache` 内部为 `moka::future::Cache<(i64, i64), Arc<CachedGraphMetrics>>`，键为 `(project_id, relation_epoch)`。
   - `CachedGraphMetrics` 持有 `entity_metrics: HashMap<EntityId, EntityMetrics>` 与 `components: Vec<Vec<EntityId>>`，二者均可克隆/共享（`Arc` 包裹整体）。
   - 缓存容量与 TTL 取保守值（如 64 条 / 30 分钟），仅作热路径优化，正确性不依赖缓存命中。
2. `cce-server-shared` 增加 `moka = { workspace = true }` 依赖（workspace 已有 0.12，features future）。
3. `export_context` 用 `(project_id, access.snapshot.relation_epoch)` 查缓存：
   - 命中：直接复用 `entity_metrics` 与 `components`。
   - 未命中：调用 `compute_metrics()` / `connected_components()`，结果 `Arc` 化后写入缓存。
   - epoch 变化即 keys 变化，天然失效，无需显式清理。
4. `connected_components()` 返回 `Result`，错误不缓存，仅缓存成功结果。

### 关键决策

- **缓存放在 `SharedContext`**（应用级、跨 HTTP/MCP/CLI 共享），而非 `GraphService` 内部——后者每次导出新建，无法跨调用复用。
- **键含 `relation_epoch`**：快照一变即换新键，杜绝跨代脏读；这与 G 的一致性目标一致。
- **纯函数抽离**：把「由 metrics 计算 per-file 指标」「由 components 计算 clusters」保留为纯函数，缓存只覆盖最昂贵的图遍历部分。

### 涉及文件

- `crates/app/cce-server-shared/src/context.rs`：`SharedContext` 增字段 + 构造初始化。
- `crates/app/cce-server-shared/src/services/wiki.rs`：新增 `CachedGraphMetrics`/缓存读取辅助，`export_context` 接入。
- `crates/app/cce-server-shared/Cargo.toml`：`moka` 依赖。
- 单测：缓存命中复用同一 `Arc`（指针相等）、不同 epoch 不命中。

## 验收标准

1. 实体目录在含类型实体的项目上返回 `class`/`struct`/`enum` 等 kind，且其 stable id 与关系图工具可 join。
2. 类型实体与 callable 实体不重复；`entities_truncated` 在含类型实体后仍正确置位。
3. `WikiManifest` 暴露 `entity_epoch` 与 `epoch_aligned`，对齐/不对齐场景可区分。
4. 同一 `(project, relation_epoch)` 的连续导出复用图指标缓存；epoch 变化触发重算。
5. `cargo fmt --check`、`cargo check --workspace`、相关 crate 单测与 clippy 全绿。

## 实施状态

| 项 | 状态 | 关键改动 |
|---|---|---|
| F 类型实体 | 已完成 | `parse_entity_kind`/`stable_id_of_record` + `entities_with_files` 增量补充类型实体，稳定 id 一致性由单测锁定 |
| G 单 epoch 锚定 | 已完成 | `WikiManifest` 新增 `entity_epoch`/`epoch_aligned`，`epochs_aligned` 纯函数 + 单测 |
| H 图指标缓存 | 已完成 | `SharedContext.wiki_graph_cache`（moka，键 `(project_id, relation_epoch)`），`export_context` 命中复用 |

验证结果：`cargo fmt --check`、`cargo check --workspace`、相关 crate 单测与 clippy 均通过（详见最终回复）。
