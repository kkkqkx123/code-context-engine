# 性能基准补充方案

## 目标

现有性能基准覆盖扫描、解析、存储、关系图、热更新、恢复、融合缓存和服务并发，但存在三个结构性问题：全链路基准关闭了向量/BM25/关系阶段；检索服务基准只使用 3 文件 fixture；除少数基准外没有统一的 CPU、内存峰值与块 I/O 指标。本次补充优先补齐这三项，并把结果纳入可复现的 TSV 趋势文件。

本方案只测耗时、吞吐与资源开销，不重复 `docs/benchmark/` 中的召回率、精确率和重排质量评测。执行结果和异常分析见 `docs/issue/performance-benchmark-supplement-2026-11-19.md`。

## 修改内容

### 1. 进程资源采样器

在 `crates/bench/cce-bench-kit/src/resource.rs` 增加只读 `/proc` 的采样器，提供当前 RSS、`VmHWM` 峰值 RSS、CPU tick、块读写字节数以及差值计算。采样器不依赖 `unsafe`，非 Linux 环境返回空值而不是中断基准。

`VmHWM` 是进程级单调计数，因此 TSV 同时记录绝对峰值和阶段内增量；CPU tick 按 Linux 常见 100Hz 转换为毫秒。块 I/O 受页缓存影响，只作为量级指标，不作为精确写入量。

### 2. 完整索引流水线基准

新增 `crates/app/cce-e2e-tests/examples/full_pipeline_perf.rs`，通过 `QueryWorkflowTest` 执行真实索引链路，包含扫描、解析、分组、双路径文本生成、规则摘要、Mock 嵌入、本地向量库、本地 BM25、SQLite 元数据和关系快照发布。

默认使用 500 文件的确定性合成项目，`CCE_PERF_SIZE=medium` 可切换到 5000 文件，`CCE_PERF_SIZE=large` 可切换到 50,000 文件。结果追加到 `crates/app/cce-e2e-tests/outputs/benchmark/full_pipeline_perf.tsv`，阶段明细追加到 `full_pipeline_stages.tsv`，记录文件数、实体数、关系数、向量数、阶段自报耗时、端到端墙钟时间、吞吐、CPU、峰值 RSS 和块 I/O 增量。

### 3. 服务层检索矩阵基准

新增 `crates/app/cce-server/benches/service_retrieval_matrix.rs`，在 500 文件合成项目的已索引服务上扫描 `vector`/`bm25`/`hybrid` 三种模式、并发 1/4/16 和冷/热缓存两类负载。每场景执行 4 轮请求，记录请求级 p50/p95/p99、吞吐、错误数、CPU、峰值 RSS 与 I/O 增量。

结果追加到 `crates/app/cce-server/benches/results/service_retrieval_matrix.tsv`，阶段明细追加到 `service_retrieval_stages.tsv`，其中包含嵌入传输耗时与本地编排耗时拆分。该基准补充原有 `service_layer_concurrency` 的 3 文件单点测量，不承担召回质量断言。

### 4. 文档状态同步

`docs/tests/benchmark_gaps.md` 增加状态说明，标明哪些原计划缺口已经有对应基准，哪些仍然未覆盖；`docs/tests/README.md` 增加本方案入口，并修正端到端性能脚本的存放说明。

## 运行方式

本地不执行全量编译；按 `docs/dev/devs-sync.md` 同步到 devs，使用暖构建缓存编译并在远端运行：

```bash
ssh devs 'export PATH=$HOME/.cargo/bin:$PATH; cd /data/kkkqkx/cce/repo && \
  cargo run --release -p cce-e2e-tests --example full_pipeline_perf && \
  cargo bench -p cce-server --bench service_retrieval_matrix'
```

两项基准均向既有 TSV 追加结果；对比时只比较同一档位、同一机器、同一构建模式下的行。

## 执行状态

已在 devs 使用同步包 `cce-src-4d0d34c5.tar.gz` 完成编译、两档完整流水线、检索矩阵和归因微基准，原始日志与 TSV 取回 `.bench_logs/`。矩阵首轮暴露 `SearchModuleConfig::default()` 的超时为 0 ms，已修复并重跑；最终矩阵全部请求成功。后续修复与复测结果见 `docs/plan/performance-regression-remediation.md` 与 `docs/issue/performance-benchmark-supplement-2026-11-19.md`。

## 本轮不覆盖的范围

以下缺口仍保留在 `benchmark_gaps.md`；本轮改动了索引与检索阶段采集、关系符号表构建和默认超时，但不引入外部依赖：

- 真实 Lua/native 插件和远端 Qdrant、Elasticsearch、PostgreSQL、模型服务的调用开销；
- 真实高亮器与源文件行读取的逐命中放大；
- 配置指纹/版本失配的恢复代价与关系 spool 双遍回放；
- 聚合融合中的词元估算、摘要加权和真实双路召回；
- 文档型文件流水线的专项基准；
- CI 回归门槛和跨机器基线归一化。

这些项目需要新增外部服务夹具、生产代码埋点或较大规模 fixture，单独排列优先级，避免把微基准改成依赖外部状态的集成测试。
