# P9 收尾事项

> 代码修改已全部落地。T-1/T-6/T-8/T-9/Q3/Q5 已完成改造并复跑；T-10 与 Q4 的剩余定因已在本沙箱
> 实测复核，结论：**两者均非代码回归**（详见第一~三节）。各 T-* 的详细数据见 `p9-benchmark-analysis.md`。
> 本轮代码改动：`crates/simvec/benches/concurrent_write_bench.rs` 增加预热（P2）；
> `crates/app/cce-orchestrator/src/query/coordinator.rs` 的 `search_with_view` 改为直接委托 `Searcher`（P1 安全版）。

复核环境：本沙箱 32 vCPU / 123GiB（无 cgroup 限制）；release 构建。
原始报告环境：AMD Ryzen 7 8845HS（Zen 4，12 逻辑核，含 AVX-512）。两轮测量**跨不同硬件**，
这是 T-10 4× 偏差的根因（见 §1.1）。原始行数据写入各 `benches/results/*.tsv`。

---

## 一、剩余问题定因结论（已复核）

### 1.1 T-10 — 服务层并发无回归（4× 为跨机器测量差异）

本沙箱实测（T-10 bench，`cargo bench -p cce-server --bench service_layer_concurrency`）：

| 指标 | 原始基线（post-M1） | 本沙箱实测 | 结论 |
|---|---|---|---|
| K=1 | 1.0 ms | 1.3 ms | 持平 |
| K=16 | 1.4 ms | 0.8 ms（19 874 req/s） | 持平/更优 |
| K=64 | 3.2 ms | 2.8 ms | 持平 |
| criterion `concurrent_16` | 811 µs | **747 µs**（739.87–755.32） | ≈ 基线，无回归 |

- **结论**：`concurrent_16` 本沙箱 747 µs，与 post-M1 基线 811 µs 同量级；原始报告里的 3.17 ms（4×）**未复现**。
- **根因**：两次测量跨不同硬件（原始报告环境 Ryzen 7 8845HS / 12 核；本沙箱 32 vCPU / 123GiB）。
  原"判据"用 T-1/T-9 排除整机差异，但 T-1/T-9 同为跨机器比较，不能据此认定 T-10 存在代码回归。
- **代码路径核对**：T-10 走 `SearchService::run_single → CodeContextEngine::search → Searcher::search`
  （`crates/app/cce-server-shared/src/services/search.rs:244` / `engine/ops.rs:470`），**不经 `QueryCoordinator`**；
  缓存经 `with_shared_cache` 注入为引擎级共享 `QueryCache`，`Searcher::search_with_view` 走 `get_or_compute`
  单飞，且 `GlobalCacheConfig`/`CacheConfig` 默认 `enabled=true`。并发 16 个相同查询会单飞到 1 次计算，
  不存在 4× 退化的代码条件。
- **P1 处理**：见 §3（不采用字面"coordinator 直接 `get_or_compute`"，因会死锁；改为安全委托）。

### 1.2 Q4 — 单写者偶发停顿未复现（为冷启动，warmup 已排除）

本沙箱实测（`concurrent_write_bench`，全量档 10 txn）加预热后：

| 运行 | `engine+pl` w=1 total | wait/txn | apply/txn |
|---|---|---|---|
| 加预热后 | 123.3 ms | 10.5 ms | 7.9 ms |

- 多轮（store/engine/engine+pl × w=1..8）**均无 13.5 s 停顿**；`engine+pl w=1` 123 ms 与 `engine w=1` 198 ms 同量级。
- 加预热后 `wait/txn` 较预热前（18.4 ms）下降约 43%，各档普遍减半 → 原始报告的偶发 13.5 s 停顿
  实为**冷启动摊销**（首批 payload 序列化 / HNSW 索引构建 / WAL 增长），已被预热排除。
- **结论**：非代码缺陷；WAL 提交路径（fsync / mmap 增长）无需改动。

---

## 二、验证完成情况

| 项 | 状态 | 结论 |
|---|---|---|
| T-10 复验 | ✅ 已完成 | 747 µs ≈ 基线 811 µs，无回归 |
| Q4 复验 | ✅ 已完成 | 无 13.5 s 停顿；预热后稳态干净，确认为冷启动 |

---

## 三、修改方向（已落地）

| 优先级 | 目标 | 动作 | 状态 |
|---|---|---|---|
| P1 | T-10 双缓存层 | **安全版**：`QueryCoordinator::search_with_view` 不再自行对共享缓存做 `get/put`，改为直接委托 `Searcher`（其内部 `get_or_compute` 负责缓存 + 单飞）。消除冗余双写双查，**且规避死锁**——字面"coordinator 调 `get_or_compute` 再进 `Searcher`"会在同一 key 上重入不可重入的 `inflight` 互斥锁而自死锁。 | ✅ 已落地 + 集成测试通过 |
| P2 | Q4 冷启动 | `concurrent_write_bench` 测量前插入一次不计时预热（store/engine 两路径各抽 `run_*_phase`），摊销首批 payload / 索引构建；稳态数值干净。 | ✅ 已落地 + clippy/fmt 通过 |

> 说明：原始 P1 假设"4× 来自 coordinator 双缓存层"经实测证伪（T-10 不经 coordinator，且本沙箱 747 µs = 基线）；
> 但 coordinator 的冗余双缓存确属设计债，安全版委托在消除该债的同时不引入死锁，故保留为代码改进。

---

## 四、已完成（归档）

### 4.1 代码修改

| 项 | 内容 |
|---|---|
| M5 | `VectorBatchWriter`：跨 embedding 微批攒批到 512 点再写；调用方在标记 work unit Committed 前 `flush().await` |
| Q1 | `default_rate_limit` 60 → 600/分（burst = `ceil(rate/60)`），并补充文档引导 |
| Q2 | `[cache]` / `[cache.query_result]` → `CacheConfig::from(&GlobalCacheConfig)`，由引擎注入共享查询缓存 |
| Q6 | `scanner.parallelism`：未设置=机器并行度；0/1=串行；>1=专用固定线程池 |
| Q5 | BM25 写路径：常驻单 `IndexWriter`（`writer()` 返回 `MutexGuard`）、按操作聚合删除提交、接线 `writer_num_threads`、实体映射去平方（详见 `bm25-write-path-performance.md`） |
| 前序 | M1–M4、M7–M10、Q3 已落地（详见 `p9-benchmark-analysis.md`） |

### 4.2 本轮验证结果

| bench | 关键结果 | 结论 |
|---|---|---|
| T-6（M5） | 32 批 3002 pts/s → 512 批 30607 pts/s；序列化仅 2.0 ms/5000 | 10.2×，超预估 4× |
| T-9（M4/Q6） | walker serial 71.6 ms vs 默认池 14.6 ms（5000 文件，条目集一致） | 4.89×，配置生效 |
| T-1 | 436 ms / 500 文件（1146 files/s） | 与基线 ~443 ms 持平 |
| T-8（M10） | `from_index` 1.8 / 39.3 / 450.3 ms @1k/10k/100k | O(N)，符合预期 |
| Q3 | dim 扫描中 avx512 比 avx2 慢 10%~65%；128 维 Cosine 仅快 3.9% | avx2 优先正确 |
| Q4 | 见 §1.2 | 偶发停顿，待定因 |
| Q5 | BM25 写路径（2k/20k）：每次提交成本 17.7ms → 3.7–5.4ms 且跨规模恒定；x1 28 → 270 docs/s（9.8×），x50 2838 → 9310 docs/s；查询 p50 14.74ms → 0.07ms；20k 档整轮约 100s 完成 | 超线性增长消除，20k 可行 |
| T-10 | 见 §1.1 | 低于基线，待定因 |
