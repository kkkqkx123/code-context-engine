# 性能基准补充执行与异常分析（2026-11-19）

## 范围与环境

本轮在 devs 编译机执行，源码同步包为 `cce-src-4d0d34c5.tar.gz`，`BUILD_INFO` 记录了本地工作树改动。机器为双路 Xeon E5-2667 v3、32 vCPU、31 GiB 内存、Linux 4.15、Rust 1.100.0；所有 Rust 基准使用 `bench`/`release` profile。原始日志和 TSV 已取回本地 `.bench_logs/` 保存，不进入版本库。

后续修复结果已回写到本文档末尾；本轮结论中的“向量批大小 32”属于探针口径，当前生产路径已使用 512 点聚合写入，具体见修复结果章节。

本轮执行内容：

- `full_pipeline_perf`：500 与 5000 文件两档完整索引流水线；
- `service_retrieval_matrix`：500 文件已索引服务上的 vector/bm25/hybrid × 并发 1/4/16 × 冷/热矩阵；
- 已有基准 `index_pipeline_stages`、`batch_concurrency`、`local_vector_store`、`bm25_write` 用于归因；
- 定向单测 `test_search_config_from_search_module_config` 用于验证默认配置修复。

## 指标结果

### 完整索引流水线

| 档位 | 文件 | 实体 | 向量 | 墙钟 ms | 吞吐 files/s | CPU ms | 峰值 RSS KiB | 读/写字节增量 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| small | 500 | 6,734 | 500 | 4,660.2 | 107.3 | 8,920 | 311,288 | 24,576 / 2,281,472 |
| medium | 5,000 | 67,484 | 5,000 | 121,080.3 | 41.3 | 158,450 | 1,244,904 | 16,982,016 / 20,713,472 |

两档均成功索引全部文件，失败数为 0，关系数为 0（合成文件之间没有跨文件调用，符合夹具形态），`IndexResult::elapsed_ms` 为 0。

作为对照，同机 `index_pipeline_stages` 的 500 文件扫描/解析/分组/切块/元数据阶段约 941.7 ms（Criterion p50 约 1.00 s）；`batch_concurrency` 在 120 文件、batch=100、20 workers 时达到 1,386 files/s。解析与文本生成本身不是本轮 10 倍规模退化的主要来源。

### 服务层检索矩阵

表中 p50/p95/p99 为请求级耗时，吞吐为每场景 4 轮请求的墙钟吞吐。修复默认超时后，所有场景错误数均为 0。

| 模式 | 并发 | 阶段 | p50 ms | p95 ms | p99 ms | req/s | 峰值 RSS KiB |
|---|---:|---|---:|---:|---:|---:|---:|
| vector | 1 | cold | 2 | 2 | 2 | 479.9 | 320,984 |
| vector | 4 | cold | 2 | 3 | 3 | 1,247.8 | 321,076 |
| vector | 16 | cold | 93 | 97 | 99 | 233.3 | 321,288 |
| vector | 16 | warm | 3 | 5 | 6 | 2,042.4 | 321,288 |
| bm25 | 1 | cold | 1 | 1 | 1 | 609.2 | 321,288 |
| bm25 | 4 | cold | 1 | 2 | 3 | 1,727.6 | 321,288 |
| bm25 | 16 | cold | 3 | 6 | 7 | 2,151.9 | 321,312 |
| hybrid | 1 | cold | 99 | 100 | 100 | 14.6 | 321,312 |
| hybrid | 4 | cold | 98 | 99 | 99 | 40.2 | 321,312 |
| hybrid | 16 | cold | 95 | 97 | 98 | 161.7 | 321,368 |
| hybrid | 4 | warm | 1 | 1 | 1 | 2,157.3 | 321,312 |
| hybrid | 16 | warm | 3 | 4 | 4 | 2,191.6 | 321,368 |

服务矩阵期间进程 RSS 稳定在约 320 MiB，块 I/O 增量基本为 0；路径已在页缓存中，I/O 不是该场景的瓶颈。

## 发现与归因

### 1. 默认搜索超时为 0，导致冷查询大量失败（已修复）

首次执行矩阵时出现大量 `QUERY_ERROR: Operation timed out after 0ms`：hybrid 全部请求失败，vector c16 全部失败，bm25 c16 约 39% 失败。根因是 `SearchModuleConfig` 使用派生 `Default`，字段 `timeout_ms` 被置为 0；`#[serde(default = "default_search_timeout_ms")]` 只对配置文件反序列化生效，`AppConfig::default()` 构造的测试/嵌入环境仍得到 0 ms 预算。

已将 `SearchModuleConfig` 改为手写 `Default`，把 `timeout_ms` 设置为文档约定的 30,000 ms，并在 `SearchConfig::from(SearchModuleConfig::default())` 测试中锁定该值。重跑后矩阵错误数为 0。定向单测通过。

这说明“默认值来源”必须同时覆盖 serde 路径和 Rust `Default` 路径，否则基准测到的是错误路径而不是性能路径。

### 2. 完整索引在 5k 文件出现明显超线性退化

文件数从 500 增至 5,000（10 倍），实体数约 10 倍，但墙钟耗时从 4.66 s 增至 121.08 s（约 26 倍），吞吐从 107.3 降至 41.3 files/s，CPU 时间从 8.92 s 增至 158.45 s（约 17.8 倍），峰值 RSS 从 311 MiB 增至 1.22 GiB。

现有微基准可以排除部分候选：

- 扫描/解析/分组/切块/元数据阶段在 500 文件仅约 0.94 s，且批并发能把 120 文件处理提升至约 1,386 files/s，未见同步线性瓶颈；
- 关系快照在 100k 实体的历史结果中，共享发布约 169 ms、深拷贝约 458 ms，远不足以解释 100 秒量级；
- 向量写入的生产微批次成本很高：`local_vector_store` 中 5,000 点按 32 点批次写入耗时 6,713 ms，按 512 点批次仅 599 ms，相差 11.2 倍；payload 转换仅 3.3 ms，说明成本集中在 WAL/fsync/发布固定开销；
- BM25 提交频率影响更大：仓库已有 2,000 文档结果中 commit x1 为 178.6 s，x10 为 7.5 s，x50 为 0.88 s；本轮将 5,000 文档 commit x1 运行超过 10 分钟仍未完成，随后终止。

因此最可能的超线性来源是后端写入的提交/批次放大与随之增长的元数据/checkpoint 事务，而不是 AST 解析或关系图构建。当前 `IndexResult::elapsed_ms` 为 0 也说明阶段耗时没有被汇总，无法直接把 121 s 分摊到各后端阶段，这是下一轮必须先补的埋点。

### 3. vector 与 hybrid 冷路径存在约 100 ms 尾延迟悬崖

修复超时后，vector c1/c4 冷请求 p50 为 2 ms，但 c16 冷请求 p50 升至 93 ms；hybrid 在所有并发档的冷请求 p50 都约为 95～99 ms，吞吐跌至 14.6～161.7 req/s。对应热请求在 c4/c16 恢复到 1～3 ms、2,000 req/s 以上，说明这不是稳定的 CPU 或内存瓶颈。

hybrid c1 热阶段 p50 为 0 ms、但 p95 仍为 99 ms，呈现双峰：每个新查询模式/查询文本的首请求仍支付约 100 ms 初始化成本，后续请求走缓存。由于 bm25 单独路径没有该现象，问题更可能位于向量查询的嵌入调用、首次缓存填充或 fusion/annotation 的一次性初始化，而非 SQLite 或 Tantivy 本身。

现有矩阵只记录错误码，不能区分“首次嵌入请求”“缓存未命中”“关系标注初始化”。需要在响应或指标中补充每请求的子查询执行数、缓存命中、嵌入调用耗时与来自缓存标记，再做一次分阶段复测。

### 4. 资源指标的可解释边界

`resource_snapshot` 使用 `VmHWM` 与 `/proc/self/io`。`VmHWM` 是进程生命周期峰值，阶段增量只能作为方向性指标；服务矩阵在索引完成后运行，RSS 已稳定，因此增量多为 0。块 I/O 受页缓存影响，也不适合作为精确写入量。完整索引的 1.22 GiB 峰值 RSS 与 20.7 MiB 写字节增量说明内存增长主要来自进程内索引/缓存结构，而非文件写入量。

## 后续任务

按优先级排列：

1. 优化 50k 档位的向量写入、BM25 写入与批次剩余开销；该档位已从 2,113 秒降到 1,473 秒，但仍呈明显超线性。
2. 在具备外部服务的环境补真实插件、远程 Qdrant/Elasticsearch/PostgreSQL、真实高亮和恢复失配基准；离线 devs 不具备这些条件。
3. 如需继续压缩 hybrid 冷请求，需在 llm-suite 传输实现内部拆分连接建立与首字节响应耗时。

## 修复结果（同日补充）

分阶段方案见 `docs/plan/performance-regression-remediation.md`。已完成并复测的修改如下：

- `IndexResult::stages` 与 `QueryResult::stages` 提供阶段级耗时，完整流水线和服务检索矩阵写入单独的阶段 TSV。
- 状态投影在全量索引期间按批次批量提交，避免每文件一次 SQLite 事务；该项收益约 1.5 秒，属于正确性收益而非主要瓶颈。
- 关系符号表从逐文件累积改为最终一次性构建；修复两处 O(N²) 重建：无符号删除时不再全量重建全局索引，增量类型更新后重载集合改为脏标记惰性重建。
- 扁平符号索引改为按 `(simple_name, file_path)` 去重、按名称一次排序的批量插入，消除重复名字场景下的逐次全向量扫描。
- `EmbeddingTiming` 将嵌入请求拆为传输提供者耗时与本地编排耗时，`SearchModuleConfig::default()` 的 30 秒超时回归断言保留。

最终复测（同一 devs、同一构建模式）：

| 档位 | 修复前墙钟 | 修复后墙钟 | 修复后主要阶段 |
|---|---:|---:|---|
| 500 文件 | 4,660 ms | 3,731 ms | BM25 1,430 ms、关系 616 ms、实体 458 ms |
| 5000 文件 | 121,080 ms（阶段首测 92,218 ms） | 26,782 ms | 关系 4,686 ms（其中符号表 1,121 ms）、向量 6,956 ms、BM25 5,975 ms、实体 4,962 ms |
| 50000 文件 | 2,113,710 ms（批量构建后首测） | 1,472,700 ms | 向量 622,037 ms、BM25 590,880 ms、关系 69,715 ms（其中符号表 14,366 ms）、批次剩余 87,008 ms |

5000 文件相对首轮阶段测试提速 3.44 倍；500 到 5000 文件的规模比降到约 7.2 倍。符号表优化在 5000 文件档把符号构建从 11,909 ms 降到 1,121 ms，在 50,000 文件档从 785,778 ms 降到 14,366 ms。

服务检索矩阵重跑后错误数仍为 0，延迟分布未出现回归。hybrid 冷请求 c=1 的阶段数据为 `embedding_ms=55.50`、`embedding_provider_ms=55.25`、`embedding_overhead_ms=0.25`，说明该冷请求耗时在传输提供者内部，而非 resilience、批处理或本地编排；未做降级式“修复”。

原始阶段数据见 `.bench_logs/full_pipeline_stages.tsv`、`.bench_logs/service_retrieval_stages.tsv`，复测日志见 `.bench_logs/bench-4d0d34c5-*.log`。

## 证据路径

- `docs/tests/performance_benchmark_supplement.md`：本轮修改方案；
- `.bench_logs/full_pipeline_perf.tsv`、`.bench_logs/service_retrieval_matrix.tsv`：新增基准原始结果；
- `.bench_logs/bench-4d0d34c5-*.log`：devs 执行日志；
- `.bench_logs/test-4d0d34c5-config-default.log`：默认超时修复的定向单测；
- `crates/infra/cce-storage-vector-local/benches/results/local_vector_store.tsv`、`crates/infra/cce-storage-bm25/benches/results/bm25_write.tsv`：归因用微基准结果。
