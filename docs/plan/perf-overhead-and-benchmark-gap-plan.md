# 性能开销分析与性能测试补充设计

## 一、范围

排除完全依赖外部 API 的耗时（嵌入/rerank 模型推理本身），聚焦本项目代码内可控的性能开销：扫描、解析、批处理调度、本地存储写路径、关系索引构建、热更新路径。同时盘点现有性能测试覆盖，给出补充方案。

## 二、主要性能开销点

按「影响 × 频率」排序，量级为代码阅读推断，需基准验证。

### P-1 embedding microbatch 全串行调度

- 位置：`crates/infra/cce-llm-client/src/services/embedding/handler.rs`（逐批 await）；`crates/app/cce-orchestrator/src/storage_coordinator/vector.rs`（for 循环逐批发送 + 批间 `batch_delay_ms` sleep + 每批前后各一次 checkpoint SQLite 事务）。
- 问题：没有任何请求级并发，吞吐 = 1/(单批 RTT + delay)；千批级全量索引延迟线性叠加。`batch_delay_ms` 防限流 sleep 进一步放大。每 microbatch 额外 2 次 SQLite 往返。
- 改进方向：`buffer_unordered(k)` + 信号量限并发；checkpoint 改为按工作单元组提交。

### P-2 文件级 chunker 全局 Mutex

- 位置：`crates/app/cce-orchestrator/src/index/file_processor/code.rs`、`document.rs`。
- 问题：所有并发文件在 chunking 阶段争抢同一把 `Mutex`，解析阶段获得的并发在 chunking 段退化为串行，流水线整体并行度被锁死。
- 改进方向：chunker 若为无状态则去掉共享实例；若有状态，按文件分片或改用无锁设计。

### P-3 SQLite metadb 逐行插入与高频小事务

- 位置：`crates/infra/cce-storage-metadb-sqlite/src/contract.rs`（files/entities/chunks/mappings 各 save 均为循环内单行 `tx.execute`）；`summary_json_upsert` 等每文件一事务；orchestrator 的 CheckpointManager / state_tracker 每工作单元一事务。
- 问题：每行单独 prepare/bind，未复用 prepared statement 或 multi-row VALUES（`helpers.rs` 已有批量路径但热路径未用）；每事务一次 fsync，checkpoint 写放大是 metadb 写入主源。
- 改进方向：热路径统一走批量插入；checkpoint 合并/降频；确认 WAL 模式与 `synchronous` 配置。

### P-4 tantivy 每 microbatch / 每次删除 commit + reader reload

- 位置：`crates/infra/cce-storage-bm25/src/batch.rs`、`document.rs`、`delete.rs`。
- 问题：每个 microbatch 提交一次（segment 合并 + 磁盘刷写 + reader reload）；热更新删文件时每文件一次 commit，delete-only commit 还不清除已删文档。单 writer 锁使并发写互相等待。
- 改进方向：全量索引攒批延迟 commit；删除路径合并为定期批量 commit。

### P-5 关系索引全量重放

- 位置：`crates/app/cce-orchestrator/src/index/orchestrator/finalize.rs`。
- 问题：`build_and_publish_relations` 每次全量索引（含 resume 后）先 clear，再重放 spool 全部文件（内存缓存超阈值退化为多次磁盘读）并做两阶段全量 resolve，是全量索引尾部的固定 O(N) 成本。好的一面：不重新 tree-sitter 解析，只反序列化。
- 改进方向：评估 resume 场景复用已有快照做增量 resolve；spool 阈值与内存缓存策略调优。

### P-6 simvec WAL 每事务 fsync 且持写锁、索引发布全量重写

- 位置：`crates/simvec/src/storage.rs`（写锁内 append WAL + fsync，阻塞所有读者）、`index/persist.rs`（每次发布全量序列化 + `sync_all` + rename）。
- 问题：批量索引写路径被磁盘 fsync 串行化；大集合每次发布索引是秒级停顿。小集合（低于 full_scan_threshold）查询退化为暴力扫描。
- 改进方向：WAL 组提交/延迟 fsync；persist 增量化或双缓冲发布。注意 simvec 是独立子模块，改动需走子模块仓库。

### P-7 codegraph 快照 O(N) 遍历与 deep_clone

- 位置：`crates/parser/cce-codegraph/src/index/core/snapshot.rs`、`stores/entity_store.rs`、`relation_store.rs`。
- 问题：即使 `affected_files` 很小，每次快照发布都遍历全部 DashMap 并 deep_clone；高频热更新时成本随项目规模而非变更规模增长。delta 链查询合并为 O(链长 × 边数)。
- 改进方向：按文件建立倒排分片索引使快照只拷贝受影响分片；delta 链定期压缩。

### P-8 扫描阶段串行 IO + 全内容哈希

- 位置：`crates/infra/cce-scanner/src/walker.rs`、`file_processor.rs`。
- 问题：全量扫描单线程递归遍历，每文件全读并全内容哈希；增量路径已有 (size, mtime) 复用，全量场景是固有成本。插件文件级过滤逐文件 FFI/Lua 调用。
- 改进方向：并行遍历 + 有界并发哈希；确认与解析流水线的重叠程度。

### P-9 流水线阶段无重叠

- 位置：`crates/app/cce-orchestrator/src/index/orchestrator/batch.rs`。
- 问题：批内 解析→SQLite→向量→BM25→summaries→checkpoint 各阶段完全串行，批间也无流水线重叠，CPU 与 IO 不并行。次要项：NL export 每 batch 深拷贝 chunk map。
- 改进方向：评估跨批流水线（生产者-消费者）重构的收益与复杂度。

## 三、现有性能测试覆盖

### 已覆盖

| 范围 | 位置 | 内容 |
|---|---|---|
| 向量引擎 | `crates/simvec/benches/`（criterion，9 个文件） | scan 延迟/SIMD/维度扩展/过滤选择性、IVF build 与 recall@10、HNSW 构建/摄取/并发查询、distance kernel、persist CRC、WAL upsert 吞吐 |
| 编排器 | `cce-orchestrator/benches/`（7 个，手写 Instant） | 热更新扩展性/作用域、checkpoint 编解码、批并发、恢复曲线、融合缓存、分层链路 |
| 关系图 | `cce-codegraph/benches/` | final_replay（全量重放）、relation_perf（关系抽取） |
| 解析 | `cce-parser/benches/` | chunk_summary、inline_embedded、parse_nl_chunk |
| 扫描 | `cce-scanner/benches/` | poll_rates、scan_hash |
| BM25 | `cce-storage-bm25/benches/` | 查询延迟、缓存、规模扩展 |
| SQLite | `cce-storage-metadb-sqlite/benches/` | metadata_read、storage_txn |
| 分词 | `cce-text/benches/` | 中英混合分词 |
| 端到端 | `e2e/`（Python）+ `cce-e2e-tests` | 分档 fixtures（~100/~1000 文件）、迭代计时、指标采集（查询/rerank 延迟直方图）、检索质量与 rerank 成本评测 |

### 覆盖缺口

1. **编排器存储协调层**：现有 7 个 bench 均不覆盖 `storage_coordinator` 的 vector/BM25/SQLite 批处理节奏（P-1、P-3、P-4 的组合路径），也没有 embedding 并发调度改造前后的对照基准。
2. **cce-storage-vector-local 包装层**：simvec bench 测的是引擎内部，未测 `spawn_blocking` 包装、scroll 分页、批 upsert 经由 `VectorStorage` trait 的端到端写吞吐（P-6 在真实调用形态下的表现）。
3. **关系索引 finalize 路径**：`final_replay` 存在但与 `build_and_publish_relations` 实际调用形态（内存缓存阈值上下、resume 后重放）是否对齐未知；无「全量索引端到端分阶段耗时拆解」基准。
4. **BM25 写路径**：现有 3 个 bench 集中在查询侧，无 commit 频率、批量写、高频删除场景基准（P-4）。
5. **metadb 写放大**：`storage_txn` 有事务吞吐，但无「逐行 vs 批量 prepared statement」「checkpoint 降频前后」的对照，也没有大项目规模（10 万实体级）数据。
6. **chunker 锁竞争**：无多文件并发下 chunking 阶段的串行化程度基准（P-2）。
7. **codegraph 热更新快照成本随项目规模的曲线**：`hot_update_scaling` 在编排器层，codegraph 层快照 deep_clone / O(N) 遍历无独立基准（P-7）。
8. **scanner 全量扫描**：`scan_hash` 存在，但无并行化改造对照与插件过滤开销基准。
9. **服务层**：`cce-api`/`cce-gateway`/`cce-mcp` 无任何基准，仅靠 e2e 粗测；HTTP 并发延迟、连接管理未测。
10. **基础设施缺口**：除 simvec/tantivy 外全部基准为手写 Instant 单次计时，无统计框架（无噪声控制、无基线对比、无 CI 化回归门禁）；`cce-metrics` 直方图无阈值回归测试。

## 四、需补充的性能测试

优先级与 P 编号对应；统一约定见第五节。

### T-1 全量索引端到端分阶段耗时拆解（覆盖 P-1/2/3/4/5/9）— 最高优先

- 形态：orchestrator 集成基准（`benches/full_index_pipeline.rs`），合成项目三档规模（小 ~500 文件 / 中 ~5000 / 大 ~50000），埋点拆解各阶段耗时占比：扫描、解析、chunking、实体落库、embed 调度（区分等待 RTT 与本地调度）、BM25 写、summaries、checkpoint、finalize 关系构建。
- 验收：能产出各阶段占比报告，作为 P-1~P-5 优化前后的对照基线。

### T-2 embedding 调度并发基准（覆盖 P-1）

- 形态：`cce-llm-client` 或 orchestrator 层 mock embedder（固定模拟 RTT，不走真实 API），测串行 vs `buffer_unordered(k)`（k ∈ {2,4,8,16}）的批吞吐与批延迟分布，含 `batch_delay_ms` 变量。
- 关键指标：批/秒、P50/P99 批延迟、限流失败率。

### T-3 chunker 并发竞争基准（覆盖 P-2）

- 形态：orchestrator bench，N 个并发文件（N ∈ {4,16,64}）穿过 chunker，测量 chunking 阶段实际并行加速比。
- 验收：改造后加速比应接近 CPU 核数下限而非 1。

### T-4 metadb 批量写入对照基准（覆盖 P-3）

- 形态：扩展 `cce-storage-metadb-sqlite` bench：10 万级 files/entities/chunks 行，对照「现逐行 save」vs「批量 prepared statement/multi-row」；checkpoint 场景测每工作单元一事务 vs 合并事务的写吞吐与 fsync 次数。
- 指标：行/秒、事务/秒、fsync 计数（可用 `iostat` 或计数包装）。

### T-5 BM25 写路径基准（覆盖 P-4）

- 形态：新增 `bm25_write.rs`：全量攒批写入（每批 1/10/50 commit 频率对照）、增量高频删除（每文件 commit vs 合并 commit）下的写吞吐与查询延迟抖动。

### T-6 向量存储包装层基准（覆盖 P-6）

- 形态：`cce-storage-vector-local` 新增 bench：经 `VectorStorage` trait 的批 upsert 吞吐（批大小 × 是否并发）、WAL fsync 耗时占比、索引发布停顿时长随集合规模（1w/10w/50w 向量）曲线、发布期间的读延迟抖动。

### T-7 finalize 关系构建基准（覆盖 P-5）

- 形态：对齐 `build_and_publish_relations` 真实形态：内存缓存阈值上/下两种 spool 模式 × 项目规模曲线；resume 后重放场景单列。
- 指标：重放耗时、resolve 耗时、峰值内存。

### T-8 codegraph 快照成本基准（覆盖 P-7）

- 形态：扩展 codegraph bench：固定小变更集（1/10/100 文件）在不同项目规模（1k/10k/100k 实体）下的快照发布耗时与内存增量，验证「成本应随变更规模而非项目规模」。

### T-9 扫描并行与插件过滤基准（覆盖 P-8）

- 形态：扩展 `scan_hash`：单线程 vs 有界并发哈希；注册 Lua/原生插件过滤前后的每文件额外开销。

### T-10 服务层并发基准（覆盖缺口 9）— 次优先

- 形态：`cce-api`/`cce-gateway` 基准，固定索引数据集下并发查询（向量/BM25/关系混合负载，1/16/64 并发）的 P50/P99 延迟与吞吐。可用轻量 HTTP 压测或进程内直调 handler 两档。

## 五、统一基准基础设施约定

1. **统计框架**：新基准统一采用 criterion（与 simvec 一致），获得样本分布、噪声控制与基线对比能力；存量手写 Instant bench 在改动对应模块时顺势迁移，不做一次性全量迁移。
2. **基准数据生成**：复用 `cce-e2e-tests` 的 `bench_gen.rs` 数据生成思路，提供合成项目生成器（可控文件数/语言分布/实体密度），保证各基准间规模口径一致。
3. **固定规模分档**：所有涉及规模的基准统一三档（小/中/大），与 e2e fixtures 档位对齐，文档中记录每档的文件数与实体数。
4. **无外部依赖**：除 T-10 可选进程内直调形态外，基准不得依赖真实 LLM API；外部调用一律用固定延迟 mock。
5. **回归门禁**：先不引入 CI 自动性能门禁（噪声与机器差异问题）；第一步是把各基准的基线数字记录到 `docs/benchmark/` 下的基线文档，人工对比。待 criterion 迁移完成后，再评估 `criterion` 的基线对比是否接入 CI。
6. **子模块边界**：涉及 simvec 内部的优化（P-6）在 simvec 仓库单独迭代，本仓库基准只测包装层调用形态。

## 六、实施顺序建议

1. 先落 T-1（端到端拆解）拿到真实占比数据，修正第二节的优先级排序；
2. 依据占比实施 P-1（embedding 并发）与 P-2（chunker 锁），配套 T-2/T-3 验证；
3. 依次推进 P-3/P-4（存储写放大）配 T-4/T-5；
4. P-5~P-8 按占比数据排期，各配对应基准；
5. 服务层 T-10 在查询侧优化立项时再补。
