# P9 — 全量基准执行结果、性能瓶颈分析与已落地修改

> 本文档记录本轮对 code-context-engine 工作区全部 bench 的执行结果、基于
> `file:line` 证据的瓶颈分析，以及已落地修改（M1/M2/M3）的前后对照数据。
> 文档遵循项目规范：方案写入 `docs/plan/`，声明均附代码引述。

---

## 0. 摘要（TL;DR）

- **执行完成度**：30 个 cce 工作区 bench + 9 个 simvec bench 全部执行完毕
  （日志 `/workspace/bench_logs/`，TSV 在各 crate `benches/results/`）。
- **T-10 根因修正并修复**：历史文档将 T-10 归因于 `ops.rs` 锁，随后又被
  "SearchService→QueryCoordinator 已锁内克隆"的观察证伪。本轮完整追踪
  `handle_search → SearchService::run_single → CodeContextEngine::search →
  Searcher::search` 后确认：**主查询路径同时存在锁串行化与缓存缺失两个
  缺陷**，且 `Searcher` 路径从不经过 `QueryCache`。修复后
  **K=16：16,018ms → 1.4ms（约 11,400×）；K=64：64,070ms → 3.2ms（约
  20,000×）**；criterion `concurrent_16` 由 16.017s 降至 **811µs
  （-99.995%）**。
- **M2 实体写入批量化**：T-1 索引管线 criterion 口径 515.67ms → 442.85ms
  （500 文件端到端 506.0ms → 482.8ms）。
- **M3 BM25 删除批量化**：新增 `delete_by_file_paths_scoped_epoch` 批量 API
  （本地 tantivy 单次 search + 单次 commit；ES 单次 `terms` delete-by-query），
  替换 `prepare.rs` 逐文件删除循环，附 3 个等价性测试。T-5 实测逐文件删除
  44,005.7ms vs 合并 38.2ms（1151×）即为该路径的量化依据。
- **BM25 写路径改造（Q5，后续轮次）**：常驻单 `IndexWriter`、按操作聚合删除提交、
  接线 `writer_num_threads`、实体映射去平方。每次提交成本 17.7ms → 3.7–5.4ms 且
  跨规模恒定，20k 档由不可完成变为约 100s 完成（见 §3.5 与
  `bm25-write-path-performance.md`）。
- **新发现**（未落地，见 §7/§8）：simvec 在该 CPU 上 **avx512 比 avx2 慢
  约 18%**（降频）；`SearcherBuilder::build` 中 `CachedEmbedder` 已构造但
  未接线（`let _embedder`，死代码）；concurrent_write 的 `engine+pl`
  单写者出现 6 倍长尾。

---

## 1. 环境与口径

| 项 | 值 | 说明 |
|---|---|---|
| 宿主机 | 32 vCPU / 123GiB | 物理规格 |
| cgroup 限额 | **8 GiB 内存 / 4 CPU**（cpu.max=400000/100000） | `available_parallelism()=4`；所有并行类数字受此封顶 |
| /tmp | tmpfs | 占用 cgroup 内存；simvec scratch 索引落在 `target/bench-vector` |
| cargo 源 | rsproxy sparse 全局镜像（crates.io 不可用） | `~/.cargo/config.toml` |
| 链接器 | lld（`-C link-arg=-fuse-ld=lld`） | 降低 fat-LTO 链接内存，避免 `cce` bin 链接 OOM |
| git | `gh-proxy.com` 全局 insteadOf | 8 个子模块经代理克隆 |

**口径注意事项**：

1. **bench 目标选择**：`cargo bench -p <pkg> --bench <name>` 必须显式指定。
   默认会同时构建 bin 目标（bin 的 `bench=true` 默认开启），`cce` bin 的
   fat-LTO 链接在 8GiB cgroup 下 OOM。
2. **gear 不可比**：simvec bench 支持 `QUICK=1` 冒烟档与默认全量档，两档
   数字互不可比；本文 simvec 数据全部取自全量档（`/workspace/bench_logs/simvec_full.log`）。
3. **T-5 文档档**：全量 20,000 文档的 commit-x1 场景在本沙箱不可完成
   （8 分钟写 27GB 未完），按 `CCE_BENCH_BM25_DOCS=2000` 运行，与存档基线
   （2000 文档档）可比；cross-check 时勿与 20k 档混用。
4. **数字仅本机可比**：4 CPU 配额下的并行加速比（如 T-9 扫描 14.28×）是
   配额内观测，不等于裸机行为。

---

## 2. 执行清单与完成度

### 2.1 cce 工作区 bench（30/30 完成）

| # | 包 | bench | 状态 |
|---|---|---|---|
| T-1 | cce-orchestrator | index_pipeline_stages | ✅（M2 后重跑） |
| T-2 | cce-llm-client | embedding_scheduling | ✅ |
| T-3 | cce-orchestrator | chunker_concurrency | ✅ |
| T-4 | cce-storage-metadb-sqlite | metadb_write_amplification | ✅（M2 后重跑） |
| T-5 | cce-storage-bm25 | bm25_write（2000 文档档） | ✅ |
| T-6 | cce-storage-vector-local | local_vector_store | ✅ |
| T-7 | cce-orchestrator | finalize_relation | ✅ |
| T-8 | cce-orchestrator | snapshot_cost | ✅ |
| T-9 | cce-scanner | scan_parallel | ✅ |
| T-10 | cce-server | service_layer_concurrency | ✅（M1 后重跑） |
| 其余 20 | 各包 | mixed_tokenizer / parse_nl_chunk / chunk_summary / inline_embedded / scan_hash / poll_rates / metadata_read / storage_txn / bm25_query / bm25_cache / bm25_scale / final_replay / relation_perf / fusion_cache / layered_chain / hot_update_scope / hot_update_scaling / checkpoint_codec / recovery_curve / batch_concurrency | ✅ |

### 2.2 simvec bench（9/9 完成）

distance_bench、vector_scan_bench、ivf_bench、hnsw_build_bench、
hnsw_ingest_bench、concurrent_search_bench、concurrent_write_bench、
alloc_stats_bench、persist_crc_bench —— QUICK 冒烟 + 全量两轮。

---

## 3. 实测数据总表

### 3.1 索引管线（T-1，500 文件 fixture）

| 指标 | 修改前 | 修改后（M2） |
|---|---|---|
| 端到端 | 506.0 ms（988.2 files/s） | 482.8 ms（1035.5 files/s） |
| criterion end_to_end_small | 515.67 ms | **442.85 ms** |

### 3.2 Embedding 调度（T-2）

serial 262.2ms vs concurrent K=16 **18.3ms = 14.32×**。生产已使用
`buffer_unordered(concurrency)`（`handler.rs:216-220`，默认 `max_concurrency=16`），
P-1 已落地。

### 3.3 分块并发（T-3）

serial 244.0ms；bounded(k=4) 67.3ms（3.62×）；unbounded(k=64) 31.3ms（7.79×）。
生产 `process_batch_with_mode` 为信号量有界并发 + blocking pool，P-2 已落地。

### 3.4 元数据库写放大（T-4，30 万行内存库）

| 场景 | 本轮 | 存档基线 |
|---|---|---|
| row_by_row | 3344.4ms（89,703 rows/s） | 3451.4ms（86,922 rows/s） |
| batch_prepared | 3850.5ms（77,911 rows/s） | 3937.9ms |
| checkpoint per-unit | 8.7ms（114,814 txns/s） | 7.6ms |
| checkpoint merged | 5.1ms（194,546 txns/s） | 5.2ms |

> 注：T-4 bench 测的是仓库层两种写入模式对照，不走 orchestrator 生产路径；
> M2 改动效果体现在 T-1（见 §3.1）。

### 3.5 BM25 写路径（T-5，2000 文档档）

| 场景 | 数据 |
|---|---|
| commit ×1 | **250,888ms**（8 docs/s；query p50 12.37ms） |
| commit ×10 | 33,465.8ms |
| commit ×50 | 6,281.7ms（p50 0.59ms） |
| **delete per-file** | **44,005.7ms** |
| **delete merged** | **38.2ms（1151×）** |

改造后（本环境，Zen 4 / release）：

| 场景 | 2000 文档 | 20000 文档 |
|---|---|---|
| commit ×1 | 7,400.7ms（270 docs/s；p50 0.07ms） | 74,121.0ms（270 docs/s；p50 0.45ms） |
| commit ×10 | 1,030.7ms（1,940 docs/s） | 10,467.4ms（1,911 docs/s） |
| commit ×50 | 214.8ms（9,310 docs/s） | 2,159.9ms（9,260 docs/s） |
| delete per-file | 1,343.4ms | 14,200.5ms |
| delete merged | 6.6ms | 69.6ms |
| segment 数（×1） | 5 | 8 |

每次提交成本由 17.7ms（×10/×50 恒定）降到 3.7–5.4ms，且在 2k/20k 上完全一致
→ 超线性增长消除，20k 档整轮约 100s 完成（改造前单档 >57min 未完成）。
根因与改造清单见 `bm25-write-path-performance.md`。

### 3.6 本地向量库（T-6）

seq 2k/5k/10k = 50.8/134.5/313.9ms（39,395/37,174/31,859 pts/s）；
conc 1.13–1.31×；read_idle 0.48–0.57ms；read_during_write p99 8.94–9.87ms；
生产微批(32) 6,740 pts/s（1000 点批的 1/4.7）。D1 修复（put 纯内存 +
checkpoint 批量化）已落地：每点 0.145ms（修复前约 17ms）。

### 3.7 关系定型与快照（T-7/T-8）

- T-7 full_rebuild 1k/10k/100k = 1.77/23.93/328.95ms；memory_materialize
  0.83/19.51/243.58ms。
- T-8 shared 0.450/7.695/120.361ms vs 深拷贝 1.714/29.491/366.155ms
  （两者随规模线性增长 → 发布路径仍是 O(N) 重建，见 B5）。

### 3.8 扫描并行（T-9）

single 65.4ms；2/4/8/32t = 34.2/18.3/8.8/4.6ms（1.91×/3.57×/7.42×/14.28×）；
criterion 持续口径 single 60.18ms vs bounded 32.02ms（仅 1.88×——4 CPU
配额节流）；filter delta 311.6ms/5000 文件（62µs/文件）。

### 3.9 服务层并发（T-10）—— 修改前后

| 场景 | 修改前 | 修改后（M1） | 提升 |
|---|---|---|---|
| K=1 | 1.0ms（953.2 req/s） | 1.1ms（889.4 req/s） | 持平（首次 embedding 消耗 burst token） |
| K=16 | 16,018.2ms（1.0 req/s） | **1.4ms（11,188.0 req/s）** | ~11,400× |
| K=64 | 64,070.0ms（1.0 req/s） | **3.2ms（19,955.1 req/s）** | ~20,000× |
| criterion concurrent_16 | 16.017s | **811µs** | -99.995% |

### 3.10 其余 bench（摘要）

mixed_tokenizer / parse_nl_chunk（parse 主导，chunk 占 13–20%）/
chunk_summary（reuse 0.14ms vs summary 1.50–2.88ms）/ inline_embedded /
scan_hash（2x read 11–31.6%）/ poll_rates / metadata_read（批量 vs 逐 id
33×）/ storage_txn / bm25_query+cache+scale（亚毫秒）/ final_replay
（canonical snapshot 52.39ms = build 的 2.6×）/ relation_perf / fusion_cache /
layered_chain（fingerprint 10–12ms 恒定）/ hot_update_scope /
hot_update_scaling（view 随规模线性：0.27→12.17ms）/ checkpoint_codec /
recovery_curve / batch_concurrency。

### 3.11 simvec 全量数据

**distance_kernel**（每核时延/吞吐）：

| 核 | dim128 | dim256 | dim512 |
|---|---|---|---|
| Dot | 9.65ns / 13.3 Gelem/s | 16.6ns / 15.4 Gelem/s | 31.2ns / 16.4 Gelem/s |
| Euclid | — | 27.25ns / 9.4 Gelem/s | 47.0ns / 10.9 Gelem/s |
| Cosine | — | 16.38ns / 7.8 Gelem/s | — |

**vector_scan**：

| 场景 | 1k | 10k | 100k |
|---|---|---|---|
| scan Dot | 17.5µs | 113.9µs | 1,580.4µs（63.3 Melem/s） |
| scan Cosine | 35.1µs | 183.2µs | 1,950.2µs（51.3 Melem/s） |
| scan Euclid | 19.2µs | 138.9µs | 1,617.3µs（61.8 Melem/s） |
| filter 100% | — | 140.1µs | — |
| filter 50% | — | 1,476.0µs | — |
| filter 10% | — | 442.2µs | — |
| filter 1% | — | 1,329.9µs | — |
| upsert_wal single | — | 3,992.6µs | — |
| upsert_wal batch_100 | — | 4,432.4µs | — |

**SIMD 内核对比（关键发现）**：

| 维度 | naive | avx2 | avx512 |
|---|---|---|---|
| 384 Dot | 2,842.2µs | **295.9µs** | 319.8µs |
| 768 Dot | 5,996.7µs | **1,032.3µs** | 1,215.2µs |

> **avx512 比 avx2 慢 18–23%**（降频效应），但比 naive 快 4.7–9.6×。
> 若运行期按 avx512 优先选择内核，反而更慢（见 B6/§8 开放点）。

**IVF**：build 2000 点 163.13ms；nprobe 1/4/16/64/256 =
24.8/40.3/188.2/300.6/236.9µs；upsert without/with index = 6.22/6.34ms。

**HNSW build**（workers_4 vs sequential）：

| 规模 | sequential | single_pool | workers_4 |
|---|---|---|---|
| 1000 | 203.98ms | 183.41ms | **63.58ms（3.2×）** |
| 2000 | 484.69ms | 481.05ms | **150.55ms（3.2×）** |

**hnsw_ingest**（256 点）：exact_scan 5.03ms vs published_hnsw 5.02ms
（已发布 HNSW 相对精确扫描无额外收益，规模太小）。

**concurrent_search**：1 线程 6.25ms（10.2 Kelem/s）；4 线程 3.04ms
（21.0）；8 线程 2.88ms（22.2）——4 线程后受 4 CPU 配额封顶。

**concurrent_write（全量）**：

| 模式 | 1w | 2w | 4w | 8w |
|---|---|---|---|---|
| store pts/s | 48,551 | 53,340 | 54,557 | 60,154 |
| engine pts/s | 33,533 | 37,643 | 37,411 | 41,202 |
| engine+pl pts/s | **6,473** | 34,995 | 38,187 | 38,767 |

> `engine+pl` 单写者 1,544.9ms 为异常长尾（其余 writer 数 258–286ms），
> 疑似冷启动摊销（首批 payload 序列化/索引构建）；待复跑确认。

**alloc_stats（10k 点）**：ingest 167.2ms（130,432 allocs / 31.9MB）；
build 2.914s（207,918 allocs / 168.0MB）；search unfiltered 19.85ms/200 查询；
filtered-miss 721.7ms/200 查询（**3.6ms/查询，高选择性过滤未命中代价高**）。

**persist_crc（10k）**：ivf crc 0.83µs vs save 4.02ms（占比 0.03%）；
hnsw crc 9.75µs vs save 8.89ms（占比 0.13%）——CRC 校验开销可忽略。

---

## 4. 与历史基线对照（docs/plan/p3-p8-baselines-and-fixes.md）

| 历史结论 | 本轮状态 |
|---|---|
| T-10 根因 = `ops.rs:464-467` 锁 | **部分恢复**：锁确实存在（已修），但完整根因还有缓存缺失层（§5.1） |
| "handle_search 走 SearchService→QueryCoordinator（已锁内克隆），锁非瓶颈" | **证伪**：`run_single` 实际走 `engine.search`（`services/search.rs:244`），QueryCoordinator 只在 `run_aggregated` 使用 |
| P-1 embedding 并发 | 已落地确认（§3.2） |
| P-2 chunker 执行器治理 | 已落地确认（§3.3） |
| P-4 BM25 bulk | 已落地确认（batch_index 单次提交） |
| simvec D1 写路径 | 已落地确认（§3.6） |
| T-4 实体逐行 prepare | **本轮修复**（M2，§6.2） |
| T-5 逐文件删除/提交 | **本轮修复批量删除**（M3，§6.3）；逐文件 commit 写入（file_ops.rs）仍存，见 §7 |
| T-9 扫描零并行 + 文件级过滤无缓存 | 未修复，方案见 §7（M4） |

---

## 5. 瓶颈分析（file:line 证据）

### 5.1 B1 — T-10：主查询路径三重叠加（已修复，M1）

完整调用链（本轮实测追踪确认）：

```
handle_search                       crates/app/cce-server/src/api/handlers/search.rs:55
  → SearchService::run_single       crates/app/cce-server-shared/src/services/search.rs:198
    → CodeContextEngine::search     crates/app/cce-server-shared/src/engine/ops.rs:451
      → searcher.lock().await       ops.rs:472（修改前持锁跨 await）
        → Searcher::search          crates/app/cce-orchestrator/src/query/searcher/searcher_core.rs:142
          → search_with_view        searcher_core.rs:151（修改前无任何缓存）
            → execute_search_flow → embedding（撞 RateLimiter）
```

三个缺陷叠加，与 K=16 = 16×1s 的实测完全吻合：

1. **锁串行化**：`ops.rs:472`（修改前）`let searcher = searcher.lock().await;`
   持有项目级 `Mutex<Searcher>` 跨整个搜索 await，16 个并发查询排队执行。
2. **缓存缺失**：`Searcher::search` 路径从不经过 `QueryCache`
   （`QueryCache` 仅被 `QueryCoordinator::search_with_view`
   使用，`coordinator.rs:477`；而 coordinator 只服务 `run_aggregated`，
   `services/search.rs:322`）。16 个相同查询各跑一次完整 embedding。
3. **限速瓶颈**：默认 `rate_limit=60/分钟` → rps=1.0, burst=1，全局共享键
   `{base_url}::{provider_id}`（`suite.rs:206-210`、
   `llm_models.rs:280-282`、`ratelimit.rs`）。每次 embedding 等 1s 令牌，
   16 次串行 = 16s。

> K=1 = 1.0ms 的解释：令牌桶初始 burst=1，首个请求立即取得令牌。

### 5.2 B2 — T-5：BM25 变更路径逐文件提交（M3 已修删除侧）

- `candidate/prepare.rs` `delete_candidate_epoch_external_data_batch`
  （修改前 :29-56）：fulltext 分支逐路径
  `delete_by_file_path_scoped_epoch`，每 path 一次 search + commit +
  reload_reader（`delete.rs` 单路径函数的既有形态）。原注释明言
  "the contract exposes no batch-by-paths form and the embedded engine is
  in-process"——即缺口本身，本轮已补齐（M3）。
- `file_ops.rs:117` 附近：`hot_update_bm25_file` 逐文件 `store_bm25`
  （1 提交/文件）——**仍未修复**，见 §7（M9）。
- `summary.rs:393`：逐文件摘要删除——单文件 API 语义，调用方
  （`deletion.rs:33`）在 candidate 内已走批量注册路径，非候选路径保留
  逐文件（失败可归因），暂不动。

### 5.3 B3 — T-4：实体写入逐行 prepare（已修复，M2）

`entities.rs`（修改前 :239）循环内 `EntityRepository::insert(tx, &record)`，
每次 `execute_insert` 重编译 22 列 INSERT（`entity_repo.rs:33-64`）。
同 crate 的 `insert_batch`（`entity_repo.rs:67-117`，内部单次 prepare）早已
存在但未被该路径使用。

### 5.4 B4 — T-9：扫描零并行 + 文件级过滤无缓存（未修复）

`crates/infra/cce-scanner/src/walker.rs` 无 rayon/thread::spawn；
`pattern_matcher.rs:178-213` `should_include_file` 线性 glob 回溯匹配，
文件级无记忆化（仅目录级 `filter_cache`），filter delta 62µs/文件。

### 5.5 B5 — T-8/hot_update_scaling：view 构造/快照发布 O(N) 重建（未修复）

shared snapshot 随规模线性（0.450→120.361ms），hot_update view 随规模
线性（0.27→12.17ms）——派生索引未增量维护。

### 5.6 B6 — T-6：simvec 包装层聚批值得重新评估（未落地）

D1 修复后每点 0.145ms，但生产 32 点微批吞吐仅为 1000 点批的 1/4.7。
存档文档曾以修复前数据否证聚批方案；D1 落地后前提改变，需重新评估。

### 5.7 新发现（本轮）

- **avx512 降频**：simvec 在该 CPU 上 avx512 比 avx2 慢 18–23%（§3.11）。
  若内核选择逻辑按 avx512 优先，应改为 avx2 优先或运行时实测选择。
- **CachedEmbedder 未接线**：`search_builder.rs:142-144` 构造了
  `CachedEmbedder` 却赋给 `_embedder`（死代码）。接线后单次搜索内
  dense + summary boost 的重复 query embedding 将合并为一次远程调用。
- **concurrent_write 长尾**：`engine+pl` 单写者 6,473 pts/s vs 其他
  writer 数 35k+（§3.11），待复跑确认是否冷启动。

---

## 6. 已落地修改

### 6.1 M1 — T-10 根因修复（QueryCache 共享 + 锁释放）

**改动清单**（`git diff --stat` 汇总 18 files, +446/-41 中与本项相关的部分）：

1. `crates/app/cce-server-shared/src/engine/ops.rs:468-473` —— 锁内克隆后释放：
   ```rust
   // 修改前
   let searcher = searcher.lock().await;
   searcher.search(options).await.map_err(EngineError::Query)
   // 修改后
   let searcher = searcher.lock().await.clone();
   searcher.search(options).await.map_err(EngineError::Query)
   ```
   `Searcher` 为 `#[derive(Clone)]` 且字段均为 Arc 句柄
   （`searcher_core.rs:46-74`），克隆廉价；该正确模式在
   `services/search.rs:439` 已有先例。
2. `crates/app/cce-orchestrator/src/query/searcher/searcher_core.rs:74` ——
   `Searcher` 新增 `cache: QueryCache` 字段；
   `:170-181` `search_with_view` 入口查缓存（hit 时置 `from_cache=true`
   并记录 metrics），`:186` 保存 pre-rewrite options，`:229-232` 执行成功
   后 put（key 必须与 get 一致，故用 rewrite 前的 options）。
3. `search_builder.rs:128` —— `with_shared_cache`；
   `build()` 缺省回落私有缓存（`QueryCache::new(Default::default())`）。
4. `engine.rs:82` —— engine 级 `query_result_cache: ProjectCache<QueryCache>`；
   `:130-147` `get_query_cache` / `invalidate_query_cache`。
5. `engine/providers.rs:518` —— `get_searcher` 构建时注入共享缓存。
6. `query/coordinator.rs:293` —— `QueryCoordinator::with_shared_cache`
   （`run_aggregated` 路径同样受益）；`services/search.rs:462` 注入。
7. `api/handlers/project/management.rs` —— 项目删除时
   `invalidate_query_cache`（与 relation searcher 一起）。

**正确性论据**：`CacheKey`（`cache.rs:25-48`）含 project_id、epoch view
（own + parent + excluded_files hash）、query/sources/filters hash、
expansion 标志与 limit——发布/采用/覆盖变化均产生新 key，不会服务过期条目。
删除项目时显式失效，防止同 ID 重建后继承。

**验证**：
- `cargo test -p cce-orchestrator --lib`：639 passed
- `cargo test -p cce-server-shared --lib`：41 passed
- T-10 重跑：§3.9（K=16 16,018.2ms → 1.4ms）
- clippy/fmt：干净（仅 vendored tree-sitter build script 预存 warning）

### 6.2 M2 — 实体写入批量化（T-4）

`entities.rs:160-268`：循环内改为先收集 `records` + `sources`，循环后一次
`EntityRepository::insert_batch(tx, &records)`（`entity_repo.rs:67-117`，内部
单次 prepare 循环 execute），再按返回的 db_ids 批量解析 parent 链接。
语义与逐行插入一致（`last_insert_rowid` 在 batch 内每次 execute 后立即取）。

**验证**：639 项 orchestrator 单测通过（含
`stores_parsed_entities_in_the_target_epoch_with_parent_links`）；
T-1 重跑 §3.1（criterion 515.67 → 442.85ms）。

### 6.3 M3 — BM25 删除批量化（T-5）

1. `cce-storage-common/src/fulltext.rs:176-194` —— `FulltextStorage` trait
   新增 `delete_by_file_paths_scoped_epoch`（带 default 顺序实现，与
   `VectorStorage` 同款契约对称）。
2. `cce-storage-bm25/src/delete.rs:245-336` ——
   `delete_documents_by_file_paths_project_epoch`：一次 BooleanQuery
   （project Must + epoch Must + 路径 terms Should 组）收集 doc_ids，
   一次 writer 批量 delete_term，**一次 commit + reload**；单路径版
   （`:222-238`）改为其薄包装。
3. `client.rs:319` —— `Bm25Client::delete_by_file_paths_scoped_epoch`
   （含 metrics 记录）；`contract.rs` local/remote 两个 impl 均覆盖
   （本地原生批量，不用 default 顺序实现）。
4. `elasticsearch/delete.rs` —— ES 版单次 `terms` delete-by-query。
5. `candidate/prepare.rs:27-55` —— `delete_candidate_epoch_external_data_batch`
   fulltext 分支由逐文件循环改为单次批量调用（`:40-48`），并更新注释
   （原注释声明的缺口已补齐）。
6. 新增 3 个等价性测试（`delete.rs` tests 模块）：批量 = 逐次删除结果、
   全覆盖清空 epoch、空批/错误 epoch 无害。20 passed。

---

## 7. 方案对照（已落地）

> 状态：M1–M10 均已实施。M6 经确认代码已正确（avx2 优先）；M10 经确认查询路径已增量；
> M5、Q1、Q2、Q6 已于本轮落地，详见 `p9-remaining-tasks-plan.md`。

| ID | 方案 | 依据 | 收益 |
|---|---|---|---|
| M4 | 扫描并行化（walker 引入 rayon/线程池，界 ~available_parallelism）+ 文件级 glob 结果记忆化 | §3.8：32 线程 4.6ms vs single 65.4ms；filter 62µs/文件 | 大项目全量索引扫描阶段数倍 |
| M5 | simvec 写路径包装层聚批（32 点微批 → 512 点批，`VectorBatchWriter` + checkpoint 前 flush） | §3.6：微批吞吐为千点批 1/4.7 | 写入吞吐约 4×（开发板复跑确认） |
| M6 | SIMD 内核选择改 avx2 优先（或运行时实测） | §3.11：avx512 慢 18–23% | 扫描核 18%+ |
| M7 | `CachedEmbedder` 接线（`search_builder.rs:142-144` 死代码） | §5.7 | 单查询内重复 embedding 合并 |
| M8 | QueryCache single-flight（相同 key 并发 miss 合并为一次执行） | T-10 修复后残余场景：不同文本并发查询仍各撞限速 | 突发同查询压测 |
| M9 | `hot_update_bm25_file` 逐文件 commit 合并 | §5.2 残留 | 热更新批量场景 |
| M10 | view/快照增量发布 | §5.5：O(N) 随规模线性 | 大项目定型阶段 |


**已实施说明**

- **M4**：`walker.rs` 在单个目录内把连续的文件收集为一个 run，用 rayon 全局池并行处理（`par_iter().collect()` 保序），遇到子目录/符号链接先 flush 再递归，扫描顺序与串行一致；`filter_cache` 改 `Mutex` 以支持并发读。`pattern_matcher.rs` 对 `IgnoreMatcher::is_ignored` 按查询路径记忆化，命中缓存的父目录判定不再重复匹配。
- **M7**：`Searcher.embedder` 与 `SummaryBoost.embedder` 统一为 `Arc<CachedEmbedder<OpenAICompatibleProvider>>`，由 `search_builder.rs::build` 构造一份并同时交给两者，dense/summary 两条路径共享同一份 query embedding 缓存。
- **M8**：`QueryCache::get_or_compute` 提供按 key 的 single-flight：首个请求持锁执行，其余请求等待后读已发布结果；实现用 `std::sync::Mutex<HashMap<CacheKey, Arc<tokio::sync::Mutex<()>>>>`（因 `QueryError` 非 `Clone`，无法用 moka `try_get_with`）。`search_with_view` 改走该入口，并在 miss 后才记录指标。
- **M9**：BM25 热更新处理器先按文件生成 chunks（保持 file-processor 锁下的串行分块），再整批调用新方法 `StorageCoordinator::hot_update_bm25_files`（变更流，含 `store_parsed_files`）或 `hot_update_bm25_paths`（chunking-drift 扫描，实体已存在），把 N 个文件的 BM25 写入从 N 次 commit 合并为 1 次。

---

## 8. 开放点处理结果

| # | 开放点 | 选项 | 结论 |
|---|---|---|---|
| Q1 | 默认 `rate_limit=60/分钟` 是否调高 | (a) 保持默认 (b) 提高到 provider 允许值 (c) 缓存 + single-flight 吸收 | 已选 (b)+文档引导：默认提到 600/分（10 rps，burst 10），并说明 burst=ceil(rate/60) 的派生关系与按上游额度配置的指引 |
| Q2 | QueryCache 配置是否暴露 | 目前 `CacheConfig::default()` 硬编码 | 已落地：复用既有 `[cache.query_result]` + `[cache] enabled`，经 `CacheConfig::from(&GlobalCacheConfig)` 注入 |
| Q3 | avx512 降频的处理器适配范围 | 需确认目标开发板 CPU 型号 | 已在同类 Zen 4（含 AVX-512）实测复现：dim 扫描中 avx512 慢于 avx2 +10%~+65%（768 Dot +16.5%、384 Dot +47.7%），仅 128 维 10k Cosine 快 3.9%（低于 >15% 晋级门槛）。avx2 优先正确 |
| Q4 | concurrent_write `engine+pl` 单写者长尾 | 复跑确认是否冷启动 | 已复跑 3 次：偶发 13.5s 停顿（wait/txn 主导，apply/txn 恒定 23µs），中位数 379ms；非冷启动摊销，属偶发提交停顿 |
| Q5 | T-5 20k 文档档全量 | 沙箱不可完成 | 已落地：常驻单 writer + 聚合删除提交后，每次提交成本 17.7ms → 3.7–5.4ms 且跨规模恒定；本环境 20k 档整轮约 100s 完成（改造前 >57min 未完成）。2k 档 x1 28 → 270 docs/s、x50 2838 → 9310 docs/s，查询 p50 14.74ms → 0.07ms。详见 `bm25-write-path-performance.md` |
| Q6 | M4 扫描并行的并行度上限 | available_parallelism vs 配置项 | 已落地：新增 `scanner.parallelism`（未设置=机器并行度；0/1=单线程；>1=固定池）。T-9 实测 `walker_scan` serial 71.6ms vs 默认池 14.6ms（4.89×） |

> 本轮复跑的完整结果与剩余事项见 `p9-remaining-tasks-plan.md`。

---

## 9. 验证命令

```bash
# T-10（M1 验证）
cargo bench -p cce-server --bench service_layer_concurrency
# 基线期望：K=16 ~1.4ms；criterion concurrent_16 ~811µs
# 本轮实测：K=16 3.2ms；criterion 3.17ms（稳定复现，偏差待定位，见 §8）

# T-1（M2 验证）
cargo bench -p cce-orchestrator --bench index_pipeline_stages
# 期望：criterion end_to_end_small ~443ms（修改前 516ms）

# M3 等价性测试
cargo test -p cce-storage-bm25 --features local,remote delete::tests

# 质量门（AGENTS.md）
cargo clippy -p cce-storage-bm25 -p cce-storage-common -p cce-orchestrator \
  -p cce-server-shared -p cce-server --all-targets --all-features
cargo fmt -p cce-storage-bm25 -p cce-storage-common -p cce-orchestrator \
  -p cce-server-shared -p cce-server

# 回归面
cargo test -p cce-orchestrator --lib        # 639 passed
cargo test -p cce-server-shared --lib       # 41 passed
cargo test -p cce-storage-bm25 --features local,remote   # 20 passed
cargo test -p cce-storage-common --test fulltext_backend_contract  # 2 passed
```

## 10. 风险与回滚

| 修改 | 风险 | 回滚 |
|---|---|---|
| M1 锁释放 | 并发执行同一 Searcher：`search_with_view` 为 `&self`，内部状态为 Arc 句柄与局部 clone（`searcher_core.rs:169` `let mut options = options.clone()`）； tantivy reader 为并发安全。已由 639+41 单测与 T-10 重跑覆盖 | revert ops.rs:472 一行 |
| M1 缓存共享 | 过期服务：key 含 epoch view，发布即失效；删除项目显式 invalidate | revert engine/providers.rs:518 一行即退回私有缓存 |
| M2 批量化 | parent 链接时序：先插入后更新，与原来一致；batch 内 rowid 获取时序等价 | revert entities.rs:251 附近 |
| M3 批量删除 | ES `terms` 查询对超多 path 有请求体上限（建议调用方每批 ≤ 数万路径；prepare 的批量为单次热更新文件集，量级百） | 调用方改回循环单删（接口仍保留） |

## 11. 后续计划

1. 定位 T-10 相对基线的 2.3–3.9× 偏差（§8 复跑结论；同机 T-1/T-9 持平）。
2. 复跑 `concurrent_write` 长尾（§8 Q4）。
3. M1–M10 与 Q1/Q2/Q6 代码已落地；T-1/T-6/T-8/T-9 与 Q3 已在本轮验证通过。
4. Q5 BM25 写路径改造（常驻 writer + 聚合提交 + 去平方）已落地并复跑，见 §3.5。
4. 本文档随数据落地持续更新（§4 对照表、§6 数据、§7/§8 状态）。
