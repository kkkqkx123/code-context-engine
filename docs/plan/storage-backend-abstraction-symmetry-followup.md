# 存储后端抽象对称性问题清单与整改方案（第二轮）

## 0 文档信息

| 项 | 值 |
| --- | --- |
| 核验对象 | `metadb` / `fulltext` / `vector` 三类存储后端的契约抽象、双实现方法集与装配层枚举 |
| 仓库 | `kkkqkx123/code-context-engine`，递归拉取含全部子模块 |
| 环境 | Rust 1.93.0；`cargo check` 可用；ES/PG/Qdrant 外部服务未启动 |
| 约束 | 遵循 `AGENTS.md`：代码与配置用英文、文档用简体中文、禁 `unwrap`、少用 `dyn`、代码注释不得引用计划编号 |
| 与既有文档关系 | `storage-backend-consistency-remediation.md` 已完成 F-01/F-02/V-01/V-02/F-03/M-01/D-01/D-02 及 O-02/O-03；本文只处理其未覆盖的新问题，并引用其 O-04 决策，不重复改写 |
| 文档约定 | 每条结论给出 `路径:行号`；精简 diff 仅示意改动要点，非完整代码 |

## 1 背景与目标

三类后端同构：`cce-storage-common` 定义契约，各有一个本地与远程实现，`cce-orchestrator` 用枚举分发（`vector_store/vector.rs:20-25`、`vector_store/fulltext.rs:17-22`、`metadb/backend.rs` 的 `MetadataBackendStore`）。上一轮整改聚焦"契约违背与韧性"，本轮聚焦"抽象本身的对称性"：方法集是否一致、默认实现是否掩盖能力差异、契约是否泄漏实现细节、契约测试是否足以证明跨后端等价。

目标：消除编译期不可见的静默不对称，合并冗余契约入口，并把无法在本轮消除的语义差异固化为明确的开放点。

## 2 核验方法

1. 逐条读取三个 trait 定义与六处 `impl`，用脚本按大括号配平提取方法名集合后做差集，避免人工遗漏。
2. 读取两个装配枚举的 inherent 方法与 trait 实现，核对是否存在同名不同实现。
3. 统计三份双后端契约测试共享 suite 实际调用的方法数，与契约方法总数对比。
4. 对每条候选问题区分：`真问题`（已修复）、`维持现状`（设计意图或既有决策）、`开放点`（需拍板）。

## 3 完整问题清单

| 编号 | 严重度 | 类别 | 问题 | 事实依据 | 处置 |
| --- | --- | --- | --- | --- | --- |
| S-01 | 高 | 真问题 | `VectorStore` 的 trait 实现漏掉 `write_metrics`，泛型路径静默拿到 `None` | `vector.rs:232-450` 实现块无该方法；inherent 方法在 `vector.rs:68-73`；对照 `fulltext.rs:126-128` 正确覆盖 | 已修复 |
| S-02 | 高 | 真问题 | 远程 fulltext 的 `stage_documents`/`commit_staged` 退化为"立即写入 + 恒返回 0" | `fulltext.rs:164-190` 默认实现；`cce-storage-bm25/src/contract.rs:143-251` 远程实现缺三个方法；编排按两阶段使用（`storage_coordinator/bm25.rs:139,170`） | 已修复（计数语义） |
| S-03 | 中 | 真问题 | 元数据契约存在两个语义重叠的删除入口 `project_delete_db` 与 `delete_project_db` | `metadb.rs:91-100` 与 `1056-1064`；两端实现都委托同一函数（sqlite `contract.rs:64-66`/`973-975`，pg `lib.rs:713-715`/`1466-1468`） | 已修复（合并） |
| S-04 | 中 | 开放点 | 远程 `entities_search_fts` 非全文检索，仅 `ILIKE '%token%'` 且按 `e.id` 排序，与本地 FTS5 不等价 | `metadb-pg/src/files.rs:485-506`；`metadb.rs:484-497` 契约注释已声明差异 | 维持现状（既有 O-04），不推翻 |
| S-05 | 中 | 设计权衡 | 契约注释含后端实现细节描述，抽象边界不纯 | `metadb.rs:79-85`、`1052-1064`、`498-505`、`415-425` | 部分不修改，理由见第 5 节纠正说明 |
| S-06 | 中 | 真问题 | 契约测试覆盖不足，无法证明跨后端等价 | 共享 suite 覆盖：vector 11/18、fulltext 12/18、metadata 37/约150；远程分支需 `CCE_TEST_*` 环境变量，否则直接跳过 | 开放点，见第 11 节 |
| S-07 | 低 | 设计权衡 | 本地 `health` 仅等于 `collection_exists`，弱于契约要求的 liveness | `vector-local/src/store.rs:733-735`；`lib.rs:486` | 维持现状（本地引擎无独立失效模式） |
| S-08 | 低 | 设计意图 | `raw_filter` 为 Qdrant 专用逃逸口，本地分支显式拒绝 | `lib.rs:83-86`；`store.rs:465-473` | 维持现状（既有文档已列为排除项） |
| S-09 | 中 | 架构问题 | 单一 `MetadataStorage` 聚合约 150 方法，新后端实现负担大 | `metadb.rs:56-1066`；宏在 `contract.rs:14-128` 生成 9 个窄域 trait 缓解调用侧依赖，但组合 trait 仍全量 | 开放点，见第 11 节 |
| S-10 | 低 | 设计意图 | fulltext `flush` 本地 no-op、远程 refresh，同名不同行为 | `fulltext.rs:294-299`；`cce-storage-bm25/src/contract.rs:128-130` | 维持现状（既有文档已列为排除项） |
| S-11 | 中 | 真问题 | `VectorStore::write_metrics` 同时存在 inherent 与 trait 默认实现，值不同 | `vector.rs:68-73` 有值，trait 实现修复前为 `None` | 已修复（S-01 一并消除） |

### 对上一轮分析的纠正

> **已过期（第三轮推翻）**：以下结论在第三轮复核中被推翻。文本协议被确认存在语义不对称、抽象泄漏与脆弱匹配三类问题，已改为结构化 `StorageError::UniqueViolation`。详见 `storage-error-protocol-and-openpoints-decision.md` 第 2~4 节。本段保留作为决策演进记录。

`metadb.rs:79-85` 要求唯一性冲突的错误文本包含 `UNIQUE constraint failed`，初看是 SQLite 细节泄漏；但 `metadb-pg/src/error.rs:29` 主动把 PG 错误格式化为该文本，且 `cce-server-shared/src/services/project.rs:120` 与 `cce-server/src/api/handlers/project/management.rs:98` 都依赖它做统一识别。这是**有意实现的跨后端错误协议**，不是缺陷，因此不改代码与注释。同理，`metadb.rs` 中其余描述后端差异的注释是对实际行为的诚实说明，删除会降低信息量，本轮不动。

## 4 与既有方案的边界

- 既有 `storage-backend-consistency-remediation.md` 的 O-04 已拍板"维持 FTS 差异"，本轮 S-04 直接引用，不重新决策。
- 既有文档的 O-05 已确认本环境无 ES/PG/Qdrant 监听，本轮同样不做外部服务集成测试。
- 本轮新增 S-01/S-02/S-03 三项代码修复，以及 S-06/S-09 两项开放点。

## 5 修复方案与落地对照

### S-01 / S-11 VectorStore 转发与调用路径统一

位置：`cce-orchestrator/src/index/vector_store/vector.rs:232`、`cce-orchestrator/src/index/storage_coordinator.rs:358-360`。
改动：在 trait 实现中补 `write_metrics` 转发；调用点改用显式 trait 调用，避免方法解析命中 inherent 实现。

```diff
 impl VectorStorage for VectorStore {
     fn backend_name(&self) -> &'static str { ... }
+
+    fn write_metrics(&self) -> Option<VectorWriteMetrics> {
+        VectorStore::write_metrics(self)
+    }
```
```diff
-        self.vector.as_ref().and_then(VectorStore::write_metrics)
+        self.vector
+            .as_ref()
+            .and_then(cce_storage_common::VectorStorage::write_metrics)
```

### S-02 远程 fulltext 的 staging 计数

位置：`cce-storage-bm25/src/elasticsearch.rs` 结构体与 `impl`、`cce-storage-bm25/src/contract.rs` 远程 trait 实现。
改动：客户端持有 `Arc<AtomicUsize>`；`stage_documents` 写入后累加，`commit_staged` 取出并清零，`finish_writes` 显式空实现（远程无本地 merge 可等待）。远程写入时点不变——仍需真实 ES 环境决定是否改为延迟发布（开放点）。

```diff
 pub struct ElasticsearchClient {
     ...
+    staged_documents: Arc<AtomicUsize>,
 }
```
```diff
 impl FulltextStorage for crate::ElasticsearchClient {
+    async fn stage_documents(...) -> Result<usize, FulltextError> {
+        let written = crate::ElasticsearchClient::batch_index(self, index_name, documents).await?;
+        crate::ElasticsearchClient::add_staged(self, written);
+        Ok(written)
+    }
+    async fn commit_staged(...) -> Result<usize, FulltextError> {
+        Ok(crate::ElasticsearchClient::drain_staged(self))
+    }
+    async fn finish_writes(...) -> Result<(), FulltextError> { Ok(()) }
```

### S-03 合并重复的删除入口

位置：`cce-storage-common/src/metadb.rs` 的 `metadata_capacity` 域、两个后端 `contract.rs`/`lib.rs` 的 capacity 实现、三处测试调用点。
改动：删除 `metadata_capacity::delete_project_db`，统一使用 `metadata_project::project_delete_db`（语义已在契约中定义为删除项目全部持久化数据并返回移除数量）。

```diff
-    fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError>;
```
调用点：`MetadataStorage::delete_project_db(` → `MetadataStorage::project_delete_db(`（`metadata_backend_contract.rs` 三处、`metadata_backend_smoke.rs` 一处）。`pg/src/snapshot.rs:518` 的 `pub(crate)` 辅助方法与 sqlite 的 `delete_project_db_sync` 为内部实现，保留。

## 6 修复后的方法集复核

| 抽象 | 本地实现 | 远程实现 | 装配枚举 trait 实现 | 说明 |
| --- | --- | --- | --- | --- |
| `VectorStorage` | 18 | 17（缺 `write_metrics`，合理：远程无本地计数器） | 由修复前 17 补到 18 | 泛型路径不再静默丢失指标 |
| `FulltextStorage` | 18 | 由 15 补到 18 | 18 | 两端都覆盖 staging 三方法；远程 `write_metrics` 仍用默认 `None` |
| `MetadataStorage` | 由 148 减到 147 | 由 150 减到 149 | 宏生成自动同步 | 冗余入口已合并；剩余差异为 `backend_name`/`is_per_project_db` 的默认值覆盖 |

## 7 验证计划与结果

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 针对性编译 | `cargo check --ignore-rust-version -p cce-orchestrator -p cce-storage-bm25 -p cce-storage-common -p cce-storage-metadb-sqlite -p cce-storage-metadb-pg` | 通过（`Finished dev profile`，无 error）。环境 rustc 1.93.0 低于 workspace 声明的 1.97，故加该标志；依赖经 rsproxy 镜像下载 |
| 残留引用 | 全仓检索 `delete_project_db` | 仅剩内部辅助方法名，无契约调用点 |
| 契约测试 | `cargo test --ignore-rust-version -p cce-storage-common --tests` | 见第 9 节落地状态；远程分支因无服务跳过 |

## 8 风险与残余

- S-02 只统一了 `commit_staged` 的返回值，远程仍在 stage 阶段写入。若上层依赖"commit 前不可见"，语义仍不等价；需真实 ES 环境验证后决定是否改为延迟发布。
- S-03 删除契约方法属于 API 变更。项目 `AGENTS.md:3-4` 明确无向后兼容要求，但外部插件若直接调用旧入口需同步；本仓内调用点已全部更新。
- S-06 的契约测试覆盖缺口未在本轮修复，跨后端等价仍主要依赖本地分支验证。

## 9 落地状态汇总

| 编号 | 状态 | 落地文件 |
| --- | --- | --- |
| S-01 | 已落地 | `crates/app/cce-orchestrator/src/index/vector_store/vector.rs`、`index/storage_coordinator.rs` |
| S-02 | 已落地 | `crates/infra/cce-storage-bm25/src/elasticsearch.rs`、`src/contract.rs` |
| S-03 | 已落地 | `crates/infra/cce-storage-common/src/metadb.rs`、两后端实现、两处契约测试 |
| S-04/S-05/S-07/S-08/S-10 | 维持现状 | 无代码改动 |
| S-06/S-09 | 开放点 | 见第 11 节 |

编译验证：`cargo check --ignore-rust-version -p cce-orchestrator -p cce-storage-bm25 -p cce-storage-common -p cce-storage-metadb-sqlite -p cce-storage-metadb-pg` 通过，输出 `Finished dev profile [unoptimized + debuginfo]`，无 error/warning。未加标志的首次 check 被 workspace 的 MSRV（要求 rustc 1.97）拦截，本环境为 1.93.0，属环境限制而非代码问题；依赖改由 rsproxy 镜像下载后完成编译。

契约测试：`cargo test --ignore-rust-version -p cce-storage-common --tests` 全部通过——lib 单测 17 passed，fulltext 契约 2 passed、metadata 契约 3 passed、vector 契约 2 passed，0 failed。远程分支因未设置 `CCE_TEST_*` 环境变量按设计跳过（既有行为，见 O-05）。

## 10 测试覆盖缺口（供后续补强）

- vector：未覆盖 `delete_collection`、`clear_collection`、`delete_by_group_epoch`、`scroll_all_points`、`count_all_points`、`backend_name`、`write_metrics`。
- fulltext：未覆盖 `write_metrics`、三个 staging 方法、`delete_by_file_paths_scoped_epoch`、`is_enabled`、`backend_name`。
- metadata：未覆盖 generation/candidate、materialize、GC、snapshot 激活、projection 等状态机关键路径。
- 远程三分支均靠环境变量启用，建议改为独立 CI job。

## 11 待拍板开放点清单

| 编号 | 开放点 | 建议 |
| --- | --- | --- |
| O-A | 远程 fulltext 是否改为真正延迟发布（stage 缓冲、commit 写入） | 需 ES 环境验证写入可见性与失败补偿；若保持现状，应在契约中写明"远程 stage 即发布" |
| O-B | 是否推翻既有 O-04、统一两端实体检索语义 | 需在 PG 启用 `tsvector` 并接受排序变化，独立立项 |
| O-C | 是否扩充共享契约 suite 至全部方法 | 优先补 generation/snapshot 状态机与 vector 集合管理；远程纳入 CI |
| O-D | 是否进一步拆分约 150 方法的组合 `MetadataStorage` | 宏已生成窄域 trait，可评估让调用方直接依赖窄 trait，但会扩大改动面 |
| O-E | 契约注释是否彻底去除后端差异描述 | 当前描述属诚实行为说明，且错误文本是统一协议；无强收益，倾向维持。**（第三轮部分推翻：错误文本项已结构化改造，见 `storage-error-protocol-and-openpoints-decision.md`）** |
