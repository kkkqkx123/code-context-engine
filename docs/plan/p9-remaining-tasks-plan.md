# P9 收尾事项

> 代码修改已全部落地。本轮在可运行基准测试的环境完成 T-1/T-6/T-8/T-9/T-10/Q3/Q4/Q5 复跑。
> Q5（BM25 写路径）已完成改造并复跑（2k + 20k 档，见第四节）。
> 剩余工作 = **2 个待定因问题** + **对应复验**，见第一~三节。
> 已完成内容仅作归档，见第四节。各 T-* 的详细数据见 `p9-benchmark-analysis.md`。

验证环境：AMD Ryzen 7 8845HS（Zen 4，12 逻辑核，含 AVX-512）；release 构建；原始行数据写入各 `benches/results/*.tsv`。

---

## 一、剩余问题（需定因）

### 1.1 T-10 — 服务层并发低于既有基线

| 指标 | 基线（post-M1） | 本轮实测 | 复现性 |
|---|---|---|---|
| K=16 | 1.4 ms | 3.2 ms | 稳定 |
| K=64 | 3.2 ms | 12.5 ms | 稳定 |
| criterion `concurrent_16` | 811 µs | 3.17 ms | 连跑 4 次均稳定 |

- **判据**：同机 T-1 = 436 ms（基线 443 ms）、T-9 与基线同量级 → 排除整机性能差异。
- **已排除**（代码分析）：Q2（`AppConfig` 派生默认与原硬编码 `CacheConfig::default()` 完全一致）、
  M7（只减少 embedding 调用）、M8（单查询路径功能等价）。
- **待办**：用 profiler 拆分 查询缓存/manifest 读取 / 检索 / 融合 / rerank 各段耗时，定位 4× 来源。

### 1.2 Q4 — 单写者偶发秒级提交停顿

| 运行 | `engine+pl` w=1 total | wait/txn | apply/txn |
|---|---|---|---|
| 1 | 379.2 ms | 35.4 ms | 23.6 µs |
| 2 | 358.6 ms | 33.5 ms | 22.5 µs |
| 3 | **13 480.3 ms** | 1 345.4 ms | 33.6 µs |

- **现象**：3 次中 1 次秒级停顿，全部落在 `wait/txn`（WAL 提交等待），`apply/txn` 恒定 ~23 µs → 非冷启动摊销，属偶发提交停顿。
- **待办**：定位 simvec WAL 提交路径（fsync / mmap 增长）的偶发停顿。

---

## 二、需补充的验证

| 项 | 现状 | 触发条件 |
|---|---|---|
| T-10 复验 | 待 §1.1 定因并修复后执行 | — |
| Q4 复验 | 待 §1.2 定因并修复后执行 | — |

---

## 三、修改方向（按优先级）

| 优先级 | 目标 | 动作 | 预期 |
|---|---|---|---|
| P1 | T-10 | 先 profiler 定位耗时段。已知不一致点：`QueryCoordinator::search_with_view` 仍用 `get_result_for_view`/`put_result_for_view`（无 single-flight、且与 `Searcher` 形成双写双查），而 `Searcher` 已改用 `CacheKey::get_or_compute`。若确认 4× 来自该层，统一改用 `get_or_compute`，消除双缓存层与并发重复计算 | 恢复 ~811 µs 量级 |
| P2 | Q4 | 先在 `concurrent_write` bench 前加 warmup 复核；若停顿仍在，排查 simvec WAL 提交的 fsync / mmap 增长路径 | 消除秒级停顿 |

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
