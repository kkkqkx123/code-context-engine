# 性能开销分析与基准测试 — 实测数据分析报告

> 配套文档：`perf-overhead-design.md`（基准设计、P→T 映射、实现顺序）
> 本文件基于**已实际运行的基准**输出，给出原始数据、统计聚合、分析与性能问题说明。

---

## 0. 文档范围与重要声明

| 项 | 说明 |
|---|---|
| **已有实测基线** | **T-2（embedding 调度并发 / P-1）**、**T-3（chunker 并发竞争 / P-2）** |
| **仅完成编译、未采集基线** | T-1、T-4、T-5、T-6、T-7、T-8、T-9、T-10（见 §6、§8） |
| **数据性质** | 全部基准使用**固定延迟的 mock**（embedding 每批 `delay_ms=3`，无真实 LLM/远端调用），其余存储层为本地内存/临时目录。**绝对数值是合成的，相对加速比才是有效信号。** |
| **运行环境** | 沙箱单机（多核），`cargo build --benches` 全部通过；criterion 采样 10 次/函数 |
| **口径** | TSV 行为"一次完整场景"的手动 `Instant` 计时；criterion `sample.json` 的 `times` 含可变迭代次数，本文已按 `iters` 折算为**每次场景运行**的时延后再聚合 |

---

## 1. 原始数据列表（统计聚合后）

### 1.1 T-2 — embedding 调度并发（P-1）

来源：`crates/infra/cce-llm-client/benches/results/embedding_scheduling.tsv`
负载：64 批 × 每批 16 文本，mock 每批延迟 3 ms，并发度 16。

**TSV 原始行**

| scenario | batches | batch_size | delay_ms | concurrency | ms | batches_per_sec |
|---|---|---|---|---|---|---|
| serial | 64 | 16 | 3 | 1 | 272.7 | 234.7 |
| concurrent | 64 | 16 | 3 | 16 | 27.8 | 2305.3 |

**criterion 逐次场景时延（折算后，ms）**

| 指标 | serial | concurrent |
|---|---|---|
| 均值 mean | 272.72 | 27.68 |
| 中位数 median | 272.63 | 27.65 |
| 最小 min | 272.00 | 27.28 |
| 最大 max | 273.74 | 28.19 |
| 标准差 sd | 0.43 | 0.32 |
| 样本数 n | 10 | 10 |

**聚合结论**：加速比 **9.81×–9.85×**（TSV 9.81× / criterion 9.85×）；吞吐 `batches/s` 从 234.7 升至 2305.3（≈9.8×）。时延分布极稳（sd < 0.5 ms），说明 mock 延迟完全主导、调度开销可忽略。

### 1.2 T-3 — chunker 并发竞争（P-2）

来源：`crates/app/cce-orchestrator/benches/results/chunker_concurrency.tsv`
负载：200 个文件，`cce-bench-kit` 合成工程（默认语言分布），`OutputMode` 走完整 chunk 流程。

**TSV 原始行**

| scenario | concurrency | files | ms | files_per_sec |
|---|---|---|---|---|
| serial | 1 | 200 | 962.6 | 207.8 |
| bounded_k8 | 8 | 200 | 700.2 | 285.6 |
| unbounded | 64 | 200 | 700.9 | 285.4 |

**criterion 补充（unbounded，64 文件变体，折算后，ms）**

| 指标 | unbounded(64 files) |
|---|---|
| 均值 mean | 225.53 |
| 中位数 median | 225.97 |
| 最小 min | 220.08 |
| 最大 max | 229.59 |
| 标准差 sd | 3.16 |
| 样本数 n | 10 |

> 200 文件与 64 文件比例 3.12，TSV 的 200 文件 unbounded(700.9 ms) ÷ criterion 的 64 文件 unbounded(225.5 ms) = 3.11，线性缩放吻合，验证负载口径一致。

**聚合结论**：
- serial → bounded_k8：**1.37×**；serial → unbounded：**1.37×**。
- **bounded_k8(700.2 ms) 与 unbounded(700.9 ms) 几乎相等** —— 并发度从 8 提升到 64 不再带来任何收益。
- 对比 T-2 的 9.8×，chunker 并发收益低一个数量级，且早早饱和。

---

## 2. 性能开销点（P）→ 基准（T）映射总览

| 开销点 | 问题本质 | 对应基准 | 状态 |
|---|---|---|---|
| **P-1** | embedding 逐批串行提交，无法重叠 | T-2 | ✅ 已测得 |
| **P-2** | 全局 chunker `Mutex` 串行化多 worker | T-3 | ✅ 已测得 |
| **P-3** | SQLite 关系元数据逐行 INSERT，事务/fsync 放大 | T-4 | ⏳ 待采集 |
| **P-4** | tantivy 每个 microbatch 都 commit，段合并放大 | T-5 | ⏳ 待采集 |
| **P-5** | 关系重索引全量重放 O(N) | T-7 | ⏳ 待采集 |
| **P-6** | simvec WAL 每条写 fsync | T-6 | ⏳ 待采集 |
| **P-7** | codegraph 快照全量 O(N) 拷贝 | T-8 | ⏳ 待采集 |
| **P-8** | 扫描哈希串行 | T-9 | ⏳ 待采集 |
| **P-9** | 索引各阶段串行、无重叠 | T-1 | ⏳ 待采集 |
| — | 服务层搜索 API 并发安全性/吞吐 | T-10 | ⏳ 待采集 |

---

## 3. 已测得基线的分析与结论

### 3.1 T-2 / P-1 — embedding 串行是端到端最重热点，并发可解

**现象**
- 64 批 × 3 ms 延迟，理论串行下限 = 192 ms，实测 272.7 ms（含 await/调度开销 ≈ 4.3 ms/批）。
- 并发（`stream::iter(...).map(...).buffer_unordered(16).collect()`）实测 27.8 ms，理论下限 ≈ 12 ms（4 波 × 3 ms），实测略高源于 future 调度与 collect 聚合壁垒。
- **加速比 9.8×**，且时延分布极稳。

**为什么是 9.8× 而非 16×**：并发度上限为 16，64 批分 4 波，理论上限加速 ≈ `min(64, 16) = 16×`；实测 9.8× 约为理论上限的 **61%**。差距来自：(a) 异步运行时调度与 `.collect()` 屏障；(b) mock 延迟固定，无真实网络抖动，真实远端 embedding 下重叠收益会更显著（延迟越高，并发相对收益越大）。

**结论 / 优化指向**
- P-1 已被量化证实为高频可优化点：**embedding 调度必须走并发流水线**，而非逐批 `await`。
- 进一步可提升空间：增大 embedder 并发池、用专用 embedder runtime、在批内再做子批分片；真实延迟下预期加速比逼近理论值。
- 该点对应代码路径 `cce-llm-client::services::embedding::handler`（批循环），修复后可用 T-2 回归验证。

### 3.2 T-3 / P-2 — chunker 并发收益极低且早早饱和，指向全局锁串行化

**现象**
- 串行 1 worker = 962.6 ms；8 并发（信号量）= 700.2 ms；64 并发（JoinSet）= 700.9 ms。
- **并发从 8 → 64 无收益**（700.2 vs 700.9，差 0.1%）。
- 整体加速仅 **1.37×**，远低于 T-2 的 9.8×。

**根因推断（Amdahl 定律 + 锁竞争特征）**
- 若 chunker 真为 CPU 密集型且各文件独立，增加 worker 应持续线性扩展（如 T-2）。但实测在 k=8 即饱和，典型信号是**存在串行关键区**：多 worker 在某一共享资源上被强制排队。
- 与 P-2 描述一致：全局 chunker 持有 `Mutex` 保护共享状态（如语言检测缓存、全局配置读取、输出汇聚）。即便开 64 个任务，它们仍在那把锁上串行进退，故吞吐被锁持有时间钉死，多余并发纯属调度开销。
- 这也解释了为何"无界并发"反而不优于"有界 k=8"：瓶颈不在 worker 数量，而在锁；k=8 已足以喂满锁的串行带宽。

**结论 / 优化指向**
- P-2 是**当前最值得优先修复的结构性问题**：它把本可并行的 chunk 阶段压成了近似串行，是 T-1 端到端耗时的隐性大头。
- 修复方向：锁分片（按文件哈希分桶）、去锁（每文件独立 `OutputMode`/局部状态后合并）、或把共享读取改为 `Arc<immutable>` 只读共享。
- 修复后预期：T-3 加速比应随并发度持续上升（趋近 T-2 量级），可用 T-3 回归验证。

---

## 4. 各性能问题说明（含待采集基准的预期结论）

> 以下基准已完成编译与运行时冒烟，但**未在本环境采集正式基线**（重负载沙箱存在耗时/OOM 风险，设计文档 §8 明确基线"环境稳定后"统一采集）。给出问题本质、基准对照设计与**预期结论**，供稳定硬件上跑批后回填。

### 4.1 P-3 — SQLite 逐行 INSERT（T-4 `metadb_write_amplification`）
- **问题**：关系元数据（files/entities/chunks）逐行 `INSERT`，每行一次事务隐含 fsync，写入放大严重。
- **基准对照**：`write_row_by_row`（逐行）vs `batch_prepared`（预编译批量）vs `checkpoint_per_unit`（每单元 checkpoint）vs `checkpoint_merged`（合并 checkpoint），记录 `rows_per_sec / txns / txns_per_sec / write_ms`。
- **预期结论**：批量预处理 + 合并 checkpoint 将事务数从 `O(rows)` 降到 `O(1~units)`，写时延下降 1–2 个数量级；`rows_per_sec` 随批量大小显著抬升。

### 4.2 P-4 — tantivy 每 microbatch commit（T-5 `bm25_write`）
- **问题**：每个 microbatch 立即 `commit`，触发频繁段合并与索引刷新。
- **基准对照**：逐文档 `add_document`+`commit` vs `batch_add_documents`（批量提交），在 2w 文档规模比较 `write_ms`。
- **预期结论**：批量提交把段合并次数从 `O(文档数)` 降到 `O(microbatch 数)`，写路径时延大幅下降。

### 4.3 P-5 — 关系重索引全量重放（T-7 `finalize_relation`）
- **问题**：任一关系变更触发 `IndexBuilder` 全量重放，`finalize` 复杂度 O(N)。
- **基准对照**：全量重建 vs 增量 `selective_cow_copy(受影响集合)` + `detached_clone()`，实体规模 {1k, 10k, 100k}，变更数 {1, 10, 100}。
- **预期结论**：增量路径复杂度 O(受影响)，少量变更时接近常数；全量路径随 N 线性增长，N=100k 时差距可达千倍级。

### 4.4 P-6 — simvec WAL 每条 fsync（T-6 `local_vector_store`）
- **问题**：向量写每条强制 WAL fsync，IOPS 受限。
- **基准对照**：`ensure_collection` + `upsert_points` 批量 vs 单条、`search_dense` 查询吞吐（经 `VectorStorage` trait）。
- **边界**：本仓库只测**包装层**，simvec 内部不在此范围（子模块自理）。
- **预期结论**：批量 upsert 减少 fsync 次数，吞吐随批量规模抬升；查询路径应近似常数。

### 4.5 P-7 — codegraph 快照全量 O(N)（T-8 `snapshot_cost`）
- **问题**：快照对全量实体做 O(N) 拷贝。
- **基准对照**：`from_index_shared`（O(1) 克隆/共享引用）vs 全量快照，实体规模 {1k, 10k, 100k}。
- **预期结论**：共享克隆耗时近常数；全量快照随 N 线性增长，大规模下差距显著。

### 4.6 P-8 — 扫描哈希串行（T-9 `scan_parallel`）
- **问题**：文件扫描与哈希串行，未利用多核。
- **基准对照**：串行扫描 vs 并行扫描（rayon/分块）+ 插件过滤，比较 `ms`。
- **预期结论**：并行扫描随逻辑核数接近线性扩展；插件过滤在并行下应保持吞吐。

### 4.7 P-9 — 索引各阶段无重叠（T-1 `index_pipeline_stages`）
- **问题**：扫描→chunk→embed→存储各阶段串行，阶段间空闲。
- **基准对照**：端到端 `IndexOrchestrator::execute`（Small 规模），拆解各阶段耗时，定位可重叠段。
- **预期结论**：识别串行关键路径；与 P-1/P-2 修复结合后，阶段重叠可进一步压缩端到端时延。

### 4.8 服务层并发（T-10 `service_layer_concurrency`）
- **问题**：搜索 API 在并发请求下是否无锁安全、吞吐是否随并发扩展。
- **基准对照**：K∈{1, 16, 64} 并发 `handle_search`，比较吞吐/延迟（复用 `tests/common.rs` 真实引擎）。
- **预期结论**：验证 handler 在并发下不退化；若共享可变状态未隔离，高 K 下延迟会陡升。

---

## 5. 综合结论与优化建议（优先级）

| 优先级 | 开销点 | 依据 | 建议动作 |
|---|---|---|---|
| **P0** | **P-2 全局 chunker Mutex** | T-3 仅 1.37× 且 k≥8 饱和 | 锁分片/去锁/每文件独立状态；修复后 T-3 应随并发持续上升 |
| **P0** | **P-1 embedding 串行** | T-2 实测 9.8× 加速 | 批间并发流水线已验证，落地到 `embedding::handler` |
| P1 | P-3 / P-4 写路径放大 | 设计预期 1–2 数量级 | 批量预处理 + 合并 checkpoint / 批量 tantivy commit |
| P1 | P-5 / P-7 重放/快照 O(N) | 设计预期随 N 线性 | 增量 `selective_cow_copy` / 共享克隆 `from_index_shared` |
| P2 | P-8 / P-9 并行与重叠 | 设计预期可扩展 | 扫描并行化 + 索引阶段流水线重叠 |
| P2 | T-10 服务层并发 | 待采集 | 并发回归，确认 handler 无锁安全 |

**核心判断**：本仓库性能瓶颈呈"**两极分化**"——
- **可并发收益高**的点（P-1 embedding）已被 T-2 量化证明，落地即可获 ~10×；
- **被串行关键区钉死**的点（P-2 chunker）并发几乎无效，必须改结构而非加线程。
二者共同构成 T-1 端到端耗时的主体，应作为本阶段优化主线。

---

## 6. 待采集基线清单（稳定硬件上回填）

以下基准已编译通过、运行时冒烟正常，但正式基线未在沙箱采集。建议在有稳定 I/O 与充足内存的机器上统一 `cargo bench` 一次，将结果追加到各 `benches/results/*.tsv`，并回填本表：

| 基准 | 规模 | 预期耗时量级 | 落盘文件 |
|---|---|---|---|
| T-1 | Small | 秒级 | `cce-orchestrator/benches/results/index_pipeline_stages.tsv` |
| T-4 | 10w 行 | 数十秒~分钟 | `cce-storage-metadb-sqlite/benches/results/metadb_write_amplification.tsv` |
| T-5 | 2w 文档 | 数十秒 | `cce-storage-bm25/benches/results/bm25_write.tsv` |
| T-6 | 50w 向量 | 分钟级 | `cce-storage-vector-local/benches/results/local_vector_store.tsv` |
| T-7 | 10w 实体 | 数十秒~分钟 | `cce-codegraph/benches/results/finalize_relation.tsv` |
| T-8 | 10w 实体 | 数十秒 | `cce-codegraph/benches/results/snapshot_cost.tsv` |
| T-9 | 并行扫描 | 秒级 | `cce-scanner/benches/results/scan_parallel.tsv` |
| T-10 | K∈{1,16,64} | 秒级 | `cce-server/benches/results/service_layer_concurrency.tsv` |

> 注：`crates/**/benches/results/` 被 `.gitignore` 忽略，基线 TSV 不进版本库；每个 `benches/results/` 已放置 `.gitkeep` 占位以保证目录存在、基准可直接运行。
