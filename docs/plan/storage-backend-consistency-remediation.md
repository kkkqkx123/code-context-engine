# 存储后端契约一致性整改方案

## 0 文档信息

| 项 | 值 |
| --- | --- |
| 基线 commit | `5b6ec68da9907e5c14cf754ff8a7c4fed776de05` |
| 核验日期 | 2026-11-22 |
| 范围 | `fulltext` / `vector` / `metadb` 三类存储后端的契约一致性、韧性与文档 |
| 环境 | Rust stable 1.100.0；rsproxy 镜像；基线 `cargo check -p cce-storage-bm25 -p cce-storage-vector-qdrant -p cce-storage-metadb-pg` 通过 |
| 约束 | 遵循仓库 `AGENTS.md`：代码与配置用英文、文档用简体中文、禁 `unwrap`、少用 `dyn`、代码注释不得引用计划编号 |

## 执行状态汇总（已落地）

| 阶段 | 问题 | 状态 | 落地位置 | 验证 |
| --- | --- | --- | --- | --- |
| 一 | F-01 | 完成 | `cce-storage-bm25/src/retrieval.rs` 排除子句路径归一化 | `cargo test -p cce-storage-bm25 --lib` 22 passed |
| 一 | F-02 | 完成 | `cce-storage-bm25/src/elasticsearch/stats.rs` epoch 排序去重 | 同上 |
| 二 | V-01 | 完成 | `cce-storage-vector-local/src/store.rs` 结构体增 `distance` 字段、重建用配置度量 | `cargo check` 通过；qdrant 单测 56 passed |
| 二 | V-02 | 完成 | `cce-storage-vector-qdrant/src/client.rs` 新增 `search_dense` 韧性方法；`backend.rs` 委托 | 同上 |
| 三 | F-03 | 完成 | `cce-storage-bm25/src/elasticsearch/write.rs` 失败按 ids 补偿回滚；`delete.rs` 开放 `pub(super)` | `cargo test -p cce-storage-bm25 --lib` 22 passed |
| 四 | M-01 | 完成 | `cce-storage-metadb-pg/src/snapshot.rs` 快照读纳入单事务；两辅助函数改 `GenericClient` | `cargo check -p cce-storage-metadb-pg` 通过 |
| 五 | D-01 / D-02 | 完成 | `AGENTS.md:48` 术语修正；`cce-types`.../simvec `distance.rs` 注释修正 | 全仓 grep 无残留 |
| 汇总 | 全量 | 完成 | — | `cargo check --workspace` 通过；clippy 无 error、无新增 warning |
| 六（开放点决策） | O-02 | 完成 | `cce-storage-vector-qdrant/src/retrieval.rs` score 归一化 + Euclid 阈值本地过滤；`client.rs` 传实际度量 | `cargo test -p cce-storage-vector-qdrant --lib` 58 passed |
| 六（开放点决策） | O-03 | 完成 | `cce-storage-metadb-sqlite/src/repo/entity_repo.rs` 批量 `get_by_ids`；`contract.rs` 替换 N+1 | `cargo test -p cce-storage-metadb-sqlite --lib` 72 passed |

实现偏差说明：
- F-02 采用 collect 后 `sort_unstable` + `dedup`，与方案"排序去重"一致；`size=10000` 上限保留（开放点 O-02 相关）。
- V-02 韧性方法命名为 `QdrantClient::search_dense`（与 trait 方法同名但位于固有 impl），`backend.rs` 用 `QdrantClient::search_dense(self, query)` 显式委托；熔断记录用 `StorageError: ErrorClassify`（`storage.rs:206`）判定 transient，未改动既有 `record_circuit_outcome`。
- F-03 补偿删除复用 `delete_by_query` 的 `ids` 查询（`write.rs::rollback_ids`），失败仅告警、原写入错误优先返回；需真实 ES 验证部分失败路径（开放点 O-05）。
- M-01 用只读事务包裹 manifest+read 两次查询，辅助函数参数改为 `&impl GenericClient`（兼容 `Client` 与 `Transaction`），调用点共 4 处已全部同步。

本方案中每条结论均给出 `路径:行号` 依据。事实性陈述与处置建议分开表述。所有"设计意图"项均不修改。

## 1 背景与目标

三类后端共享同一范式：契约在 `cce-storage-common`，本地/远程双实现，`cce-orchestrator` 用枚举分派（`fulltext.rs:16-21`、`vector.rs:15-20`、`backend.rs:11-15`）。宏观分层健康，风险集中在**双实现的行为对等性**与**个别分支的韧性缺失**。

目标：消除已确证的契约违背与韧性缺口，使本地/远程分支在同一 trait 下语义一致；同步修正过时文档。不做跨后端迁移、不重构枚举分派、不改冻结语义。

## 2 核验方法

1. 逐条读取争议代码及其上游契约，确认行号与调用链。
2. 用 `grep` 交叉验证符号定义与引用，排除"文档声称但代码不存在"的情况。
3. 对每条候选问题标注：`真问题`（契约或代码明确违背）、`设计意图`（契约冻结，不改）、`开放点`（需拍板或需真实环境验证）。
4. 基线编译确认可独立验证的作用域。

## 3 问题清单

| 编号 | 严重度 | 类别 | 问题 | 事实依据 | 处置 |
| --- | --- | --- | --- | --- | --- |
| F-01 | 高 | 真问题 | 本地 Tantivy 的 `excluded_files` 未做路径归一化即建 term，与契约及 ES 分支不一致 | `fulltext.rs:135-136` 要求 "File paths are compared after normalization on every branch"；本地 `retrieval.rs:113` 直接用 `path`；ES `query.rs:99-103` 已 `normalize_project_path` | 阶段一修复 |
| F-02 | 中 | 真问题 | ES `epochs_by_project` 返回按 doc_count 排序且被 `size=10000` 截断的 epoch 列表；本地返回有序去重全集 | `stats.rs:64` terms agg `size=10000`；`stats.rs:86-91` 按 buckets 原序 collect；本地 `client.rs:543-553` 用 `BTreeSet` | 阶段一修复 |
| V-01 | 高 | 真问题 | 本地集合被外部删除后重建时，距离度量硬编码为 Cosine，配置非 Cosine 时发生度量漂移 | `store.rs:386` `CollectionConfig::new(self.vector_size, SimDistance::Cosine)`；`LocalVectorStore` 结构 `store.rs:25-29` 未保存 distance | 阶段二修复 |
| V-02 | 高 | 真问题 | Qdrant 契约搜索路径绕过熔断、重试与指标，远程分支韧性低于写/删路径 | `backend.rs:38-48` 直接 `QdrantRetrieval::new(...).search_dense()`；`client.rs:750-776` 的 `search()` 带 `check_circuit_breaker`、`record_circuit_outcome`、`metrics.record_search`，但走的是另一套类型 | 阶段二修复 |
| F-03 | 高 | 真问题 | ES 批量写入按 chunk 分段提交，中途失败会留下"部分前缀"，违背批次原子契约 | `fulltext.rs:11-13` 冻结 "never a partial prefix"；`write.rs:28-35` 按 `bulk_size` 分块，`total += result?` 使已成功块保留 | 阶段三修复 |
| M-01 | 中 | 真问题 | PG 快照读在单个连接上执行多条 SELECT 且无显式事务，与 SQLite 单读连接语义不对等 | `snapshot.rs:65-71` 取 `client` 后连续调用 `snapshot_manifest_in` 与 `snapshot_read_in`，均为独立语句 | 阶段四修复 |
| D-01 | 中 | 真问题 | `AGENTS.md` 将元数据契约/枚举称为 `RelationStorage` / `RelationStore`，与实码 `MetadataStorage` / `MetadataStore` 不符 | `AGENTS.md:48`；实码 `contract.rs:14` 宏生成 `MetadataStorage`，`metadata.rs:16-17` 为 `MetadataStore` | 阶段五修复 |
| D-02 | 低 | 文档缺陷 | 本地 distance 模块注释声称 Qdrant 在 `engine/common/distance_utils.rs` 做 Euclid 归一化，但该文件在仓库中不存在，检索层也未归一化 | `distance.rs:8` 引用不存在的文件；全仓 `grep distance_utils` 仅命中该注释；`retrieval.rs:241` 直接透传 `score` | 阶段五修复（改注释）+ 开放点 O-02 |
| V-03 | 低 | 性能 | SQLite `entities_by_ids` 与 PG `find_base` 存在 N+1 查询 | `contract.rs:1492-1499`（逐 id）；`snapshot.rs:116-120`（逐 epoch） | 开放点 O-03，本轮不改 |
| M-02 | 低 | 行为差异 | PG 的全文匹配为 substring，SQLite 用 FTS5，切换后端结果不同 | `contract.rs:473-486` | 开放点 O-04，本轮不改 |

## 4 已排除项（上一轮分析中不成立，明确不修改）

| 项 | 排除理由 | 依据 |
| --- | --- | --- |
| 本地 `flush` 空实现 vs 远程 refresh | 契约**主动冻结**该差异：本地每批 reload reader，故 no-op 是正确的；远程需 refresh。属设计意图，不是缺陷 | `fulltext.rs:240-245`；`contract.rs:106-108`；`write.rs:78-82` |
| 切换后端需全量重建、无迁移 | 项目 `no-backward-compatible` 的刻意决策，规避跨引擎语义映射风险 | `AGENTS.md:3-4`；`storage.rs:36-38,71-73` |
| 枚举分派而非 `dyn` | 符合 `AGENTS.md:21`，两分支固定，`dyn` 仅存于事件/插件/热更新边界且已登记 | `vector.rs:15,214`；`dynamic.md:186-196` |
| `raw_filter` 为 Qdrant 专用 | 契约明确标注为 Qdrant 逃逸口，本地显式拒绝，属预期 | `lib.rs:83-86`；`store.rs:461-469` |
| 远程 phrase 查询降级为 token 合取 | 已在契约注释中说明为行为说明，且 ES 侧有明确文档 | `elasticsearch.rs:25-27`；`fulltext.rs:117-119` |

## 5 阶段一：fulltext 双端语义对齐

### F-01 本地路径归一化
- 位置：`crates/infra/cce-storage-bm25/src/retrieval.rs:113`
- 改动：构造 `file_path` term 前调用 `cce_types::normalize_project_path(path)`，与 ES 分支 `query.rs:99-103` 一致。
- 精简 diff：
```diff
-  let path_term = tantivy::Term::from_field_text(schema.file_path, path);
+  let normalized = cce_types::normalize_project_path(path);
+  let path_term = tantivy::Term::from_field_text(schema.file_path, &normalized);
```
- 影响面：仅本地检索的父代排除子句；契约测试 `fulltext_backend_contract.rs` 已用规范化路径，不破坏。

### F-02 ES epoch 列表对齐
- 位置：`crates/infra/cce-storage-bm25/src/elasticsearch/stats.rs:86-91`
- 改动：collect 后对 `Vec<i64>` 排序去重；`size=10000` 保留但补充注释说明的现实上限（epoch 数量级远小于此，GC 依赖全集，见 `gc.rs:56-67`）。
- 精简 diff：
```diff
-  .collect()
+  .collect::<Vec<i64>>()
+  // 与本地 BTreeSet 语义对齐：有序去重
+  // （注释用英文写入代码）
```
- 说明：排序去重由 `.sort_unstable(); .dedup();` 实现；代码内注释用英文，不引用计划编号。

### 阶段一验证
`cargo check -p cce-storage-bm25` + `cargo test -p cce-storage-bm25 --lib`。

## 6 阶段二：vector 韧性与度量一致性

### V-01 本地距离度量持久化
- 位置：`crates/infra/cce-storage-vector-local/src/store.rs:25-29,43-93,386`
- 改动：在 `LocalVectorStore` 增加 `distance: SimDistance` 字段，`open()` 时从 `config` 读取并保存，`ensure_collection()` 重建时使用该字段而非硬编码 Cosine。
- 精简 diff：
```diff
  pub struct LocalVectorStore {
      engine: Arc<LocalVectorEngine>,
      collection: String,
      vector_size: usize,
+     distance: SimDistance,
  }
  ...
-  let config = CollectionConfig::new(self.vector_size, SimDistance::Cosine);
+  let config = CollectionConfig::new(self.vector_size, self.distance);
```
- 风险：需确认 `SimDistance` 为 `Copy`（供结构体持有与 clone）。编译验证。

### V-02 Qdrant 搜索纳入统一韧性链
- 位置：`crates/infra/cce-storage-vector-qdrant/src/backend.rs:38-48`、`client.rs:750-776`
- 改动：在 `QdrantClient` 上新增 `search_dense` 方法（或复用现有 `search` 逻辑），执行顺序为 `check_circuit_breaker` → `QdrantRetrieval::search_dense` → `record_circuit_outcome` → `metrics.record_search`；`backend.rs` 委托给该方法，去掉直接 new。
- 约束：`check_circuit_breaker` / `record_circuit_outcome` 是 `client.rs:174,196` 的私有方法，新增方法须位于同一 impl 块或同 crate 内可访问处。
- 精简 diff（backend 侧）：
```diff
  async fn search_dense(&self, query: DenseSearchQuery) -> Result<Vec<ScoredPoint>, StorageError> {
-     let retrieval = QdrantRetrieval::new(...);
-     retrieval.search_dense(query).await
+     self.search_dense_with_resilience(query).await
  }
```
- 风险：中。保留 `QdrantRetrieval` 的过滤下推与 directory_prefix 裁剪逻辑不变，只在外层包韧性。

### 阶段二验证
`cargo check -p cce-storage-vector-local -p cce-storage-vector-qdrant` + `cargo test -p simvec --lib`（若本地引擎测试不依赖外部服务）。

## 7 阶段三：fulltext 批量写入原子性

### F-03 ES 批次失败不留前缀
- 位置：`crates/infra/cce-storage-bm25/src/elasticsearch/write.rs:26-41`
- 事实：每个 chunk 以 `_id = document_id` 覆盖写入（`source.rs:59-68`），幂等。失败时已写入块保留，违背 `fulltext.rs:11-13`。
- 改动：维护已成功 chunk 的 `document_id` 列表；任一 chunk 失败时，按这些 id 执行删除补偿（`delete_by_query` 支持 `ids` 查询，`delete.rs:11-48`），再返回错误。成功路径不变。
- 精简 diff（控制流）：
```diff
  for chunk in documents.chunks(bulk_size) {
-     total += self.bulk_once(chunk).await?;
+     match self.bulk_once(chunk).await {
+         Ok(n) => { total += n; written.extend(chunk ids); }
+         Err(e) => { self.rollback_ids(&written).await; return Err(e); }
+     }
  }
```
- 风险：中。补偿删除本身走熔断与指标，须避免补偿失败掩盖原始错误（原错误优先返回，补偿错误仅记录）。
- 开放点 O-01：是否改为"单 bulk 不分块"以彻底规避 —— 需权衡 `bulk_size` 对超大请求体的保护，见开放点清单。

### 阶段三验证
`cargo check -p cce-storage-bm25`；契约测试不依赖真实 ES，可跑 `--lib` 单测。真实部分失败回滚需 ES 环境，列为 O-05。

## 8 阶段四：metadb 读一致性

### M-01 PG 快照读单事务
- 位置：`crates/infra/cce-storage-metadb-pg/src/snapshot.rs:60-73,1035-1058`
- 改动：`snapshot_read` 从池取 `mut client` 开事务，`snapshot_manifest_in` 与 `snapshot_read_in` 均改为接受 `&Transaction` 泛型执行器（或用 `tokio_postgres::GenericClient`），在同一事务内完成后 commit（只读事务）。
- 精简 diff（调用侧）：
```diff
-  let client = self.pooled().await?;
+  let mut client = self.pooled().await?;
+  let tx = client.transaction().await.map_err(classify_pg)?;
   let manifest = snapshot_manifest_in(&tx, ...).await?...
   snapshot_read_in(&tx, &manifest).await
```
- 风险：中。涉及 `snapshot_manifest_in` / `snapshot_read_in` 参数类型调整，需同步更新所有调用点（grep 全 crate）。确认无遗漏调用后方提交。

### 阶段四验证
`cargo check -p cce-storage-metadb-pg`。该分支测试依赖真实 PostgreSQL，本环境不保证可跑，列为 O-05。

## 9 阶段五：文档与术语同步

### D-01 AGENTS.md 术语修正
- 位置：`AGENTS.md:48`
- 改动：将 `RelationStorage` → `MetadataStorage`、`RelationStore` → `MetadataStore`，并补充"元数据分派由宏生成"的准确描述。保持英文。

### D-02 distance 注释修正
- 位置：`crates/simvec/src/distance.rs:8`
- 改动：删除对不存在文件 `engine/common/distance_utils.rs` 的引用，改为中性描述：远程分支必须自行保证 score 为"越大越好"的相似度语义。

### 阶段五验证
全文grep确认无残留错误术语；`cargo check` 全量。

## 10 验证计划

| 阶段 | 命令 | 预期 |
| --- | --- | --- |
| 基线 | `cargo check -p cce-storage-bm25 -p cce-storage-vector-qdrant -p cce-storage-metadb-pg` | 通过（已确认） |
| 一 | `cargo check -p cce-storage-bm25 && cargo test -p cce-storage-bm25 --lib` | 通过 |
| 二 | `cargo check -p cce-storage-vector-local -p cce-storage-vector-qdrant` | 通过 |
| 三 | `cargo check -p cce-storage-bm25 && cargo test -p cce-storage-bm25 --lib` | 通过 |
| 四 | `cargo check -p cce-storage-metadb-pg` | 通过 |
| 五 | `cargo check --workspace` | 通过 |
| 汇总 | `cargo clippy --all-targets --all-features`（按 `AGENTS.md:60-68`） | 无新增 error |

## 11 开放点设计决策（已拍板）

本轮对上一轮悬置的 5 项开放点逐项核验代码事实后作出确定决策，不再保留"待拍板"状态。决策区分两类：**已落地代码**（O-02、O-03）与**明确维持现状**（O-01、O-04、O-05）。事实性依据与决策理由分列。

| 编号 | 决策 | 类别 | 事实依据 | 决策理由 |
| --- | --- | --- | --- | --- |
| O-01 | 维持分块 + 失败补偿，不改单 bulk | 维持现状 | `write.rs:29` 按 `bulk_size` 分块；`storage.rs:255-258` 将 `bulk_size` 限制在 1~10000 | 单 bulk 会把整批向量塞进一个 HTTP 请求体，超过 ES `http.max_content_length`（默认 100MB）时反而整体失败；补偿回滚已满足 `fulltext.rs:11-13` 的"不留部分前缀"契约。分块既有吞吐保护价值，无需为原子性牺牲 |
| O-02 | 已实现 score 归一化（含 Euclid 阈值方向修正） | 已落地代码 | 契约 `lib.rs:527` "higher score is better"；Qdrant Cosine/Dot 原生符合，Euclid 返回距离（越小越好）；原 `retrieval.rs:241` 直接透传 | 新增 `normalize_score`：Euclid 的 `d` 映射为 `1/(1+d)`，与本地 simvec `distance.rs:57` 的 `to_score` 对齐；Euclid 的 `score_threshold` 取消下推（Qdrant 视其为距离上界，方向相反），改为归一化后本地过滤。默认 Cosine 行为零变化，向后兼容。纯函数单测 58 passed |
| O-03 | 仅优化 SQLite N+1；纠正 PG `find_base` 误判 | 部分落地代码 | SQLite `contract.rs:1493-1499` 逐 id 查询（真 N+1）；PG `files.rs:1179` 的 `entities_by_ids` 已用 `= ANY($2)` 批量，无需改；PG `snapshot.rs:119-140` 的 `find_base` 沿 delta 链逐跳回溯，每一跳依赖上一跳的 `base_epoch`，是数据依赖链而非 N+1 | 为 SQLite 增加 `EntityRepository::get_by_ids`（`entity_repo.rs:139`，分批 500 规避变量上限），`contract.rs` 替换循环；PG 两处不动。上一轮把 `find_base` 列为 N+1 属定性错误，本轮纠正——该链不可批量化，强行改写会破坏 delta 语义。单测 72 passed |
| O-04 | 维持差异，不改代码 | 维持现状 | `files.rs:460-463` 明确注释 PG `ILIKE` substring 是 "portable baseline"，并说明未来可用 `tsvector` 在不改契约的前提下替换；`indexes.rs:181` 的 SQLite FTS5 是本地优化 | 该差异已被上游代码明确声明为可移植基线，属设计权衡而非缺陷。统一需在 PG 启用 FTS 并接受排序变化，超出"契约一致性整改"范围，单列为独立议题更稳妥 |
| O-05 | 不写集成测试代码，标记为环境阻断 | 维持现状 | 本环境无 ES/PG/Qdrant 服务监听（`ss -ltn` 核验 9200/5432/6333/6334 均无） | 集成测试在此环境无法运行，强行添加的测试无法验证且易腐化。F-03 补偿回滚、M-01 单事务、O-02 归一化的真实环境验证留待开发板/CI，作为外部待办而非本轮代码交付 |

### 开放点决策的残余风险

- **O-02**：Euclid/Dot 的归一化仅经纯函数单测，未在真实 Qdrant 上验证 score 方向（依赖 O-05 的集成环境）。Dot 度量下 Qdrant 返回的内积可能为负且无上界，与本地 simvec 的归一化内积（同样可负、无上界）语义一致，但数值尺度依赖向量是否预归一化——此为两后端共有的既有语义，未在本轮改变。
- **O-03**：SQLite `get_by_ids` 返回顺序不保证（原逐 id 循环按入参顺序），上层 `entities_by_ids` 调用方未依赖顺序；若未来有顺序需求需在契约层显式声明并排序。
- **O-01/O-04/O-05**：均为"维持现状"决策，若后续部署或产品要求变化，应新开方案文档，不在本文档反复改写。
