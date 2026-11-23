# 写路径优化后剩余问题（2026-11-23）

## 背景

本文承接 `docs/issue/performance-writepath-followup-2026-11-20.md`，记录
`docs/plan/write-path-scale-remediation.md` 实施并完成 devs 复测后仍未关闭的问题。

最新证据位于 `.bench_logs/`：`bench-writepath-cursor-final.log`、
`bench-writepath-service-medium-rerun.log`、`full_pipeline_stages.tsv`、
`service_remote_backend_index_stages.tsv`、`service_remote_backend_matrix.tsv`。

已完成的提交边界聚合、实体 ID 游标增量加载、批次子阶段拆分和阶段内存指标不再重复登记。
下列条目按优先级排列，既包含性能缺口，也包含本轮复测暴露的测量与工程债务。

## 1. 50,000 文件向量阶段仍是主要瓶颈

最新 50,000 文件完整流水线墙钟约 752 秒，其中向量阶段约 542 秒，占比约 72%。
5,000 文件服务档向量阶段约 24 秒，10 倍文件增长带来约 22 倍耗时，规模曲线仍偏超线性。
WAL flush 已从 50 次降到 10 次，提交等待约 0.7 秒，HNSW 构建为 0，因此剩余成本不在 WAL 提交或图构建。

验收口径：把向量阶段拆成嵌入调用、点构造、chunk/实体映射落库和状态等待四个可测子项；
50,000 文件档向量阶段绝对耗时和占比明显下降，规模曲线接近线性；召回与持久化语义不变。

## 2. 峰值内存没有下降，缺少容量归属结论

50,000 文件档峰值 RSS 约 9.5 GiB，与修复前基本持平。阶段采样显示批次循环结束约 1.5 GiB，
关系构建后升至约 9.2 GiB。当前只能确认关系阶段是主要增长点，无法区分关系快照、符号表和导出缓存各自容量。

验收口径：在阶段 TSV 中补充主要缓存条目数与字节数；明确哪些结构允许随文件数增长、
哪些必须设置上限；在不改变召回和持久化语义的前提下给出峰值 SLA 或降低峰值。

## 3. 完整流水线向量的 WAL 计数器与真实写入不一致

完整流水线阶段 TSV 中向量事务、写入点数和 WAL flush 全为 0，但结果报告 50,000 个向量；
服务矩阵同一实现能正常采集到 10 次 flush、5,000 个写入点。说明基准夹具的 collection 指标快照口径
或 `IndexStageTimings` 的 before/after 采样点存在失配。

同理，BM25 的 `bm25_documents` 与 `bm25_staged_documents` 在 5,000 与 50,000 文件档均为 2,100，
需要确认这是真实的文档规模还是计数器只覆盖了一次提交。

验收口径：完整流水线 TSV 的向量和全文计数器与服务路径一致；计数器定义写入基准文档说明；
错误数保持为 0，不能用零值推断引擎未落盘。

## 4. 本地依赖锁文件与工作区声明不一致

本地 `Cargo.lock` 中 `mlua` 为 0.11.6，而根 `Cargo.toml` 声明 `mlua = "0.12"`。
devs 首次执行 cargo 时自动解析到 0.12.3 并连带更新 `lua-src`、`mlua-sys` 等五项依赖。
本轮已按要求在本地执行一次 `cargo check`（约 7 分钟，成功结束），锁文件已更新到 `mlua` 0.12.3，
与 devs 当前解析结果一致。`Cargo.lock` 被仓库 `.gitignore` 忽略，因此不会出现在 `git status` 中；
后续需确定锁文件同步基线，避免不同机器因首次解析产生不同依赖集。

验收口径：本地与 devs 使用同一 `Cargo.lock`，`cargo check --locked` 可直接通过。

## 5. 外部服务环境的资源限制

首次 medium 服务矩阵中 Qdrant 因开发环境 `ulimit -n=1024` 出现
`No file descriptors available`，集合创建与 upsert 失败；重启服务后四档全部通过、错误数为 0。
这是环境容量问题，不应通过关闭持久化或降低刷新语义规避。

验收口径：服务矩阵运行前明确文件句柄与磁盘容量前置条件；若再次出现，记录为环境失败而不是代码回归。

## 6. 独立 SLA 与覆盖缺口

以下问题仍按独立事项治理，不并入写路径验收：

- Elasticsearch JVM 堆外常驻开销约 1.65 GB，冷查询 p95 高于本地 Tantivy，需单独记录首字节延迟 SLA。
- Qdrant medium 档冷查询曾出现单次约 136 ms 长尾，样本数不足，需提高采样确认。
- 生产 provider 默认 600/min、burst 10，chat/embedding/rerank 共享令牌桶，交互查询仍需限流预算 SLA。
- 恢复失配输入（文件列表哈希、磁盘内容、版本字段）尚未纳入自动化基准。
- 50,000 文件 Qdrant/Elasticsearch 四档服务矩阵尚未按容量条件执行。

## 本轮已处理的工程债务

- e2e 基准 TSV 行生成已改为在 `cce-bench-kit` 中使用单一列定义生成表头与行，并校验字段数和字段内分隔符；
  `full_pipeline_perf.rs` 与 `aggregated_search_perf.rs` 已迁移。列变化时趋势文件会按新表头重建。
- `cce-bench-kit` 增加三则单测，覆盖表头与行一致性、错误宽度/内嵌分隔符拒绝和 schema 变化时重置旧行。
- 本地 `cargo check` 已同步锁文件；报告入口分散与容量 SLA 仍需后续独立决策。

## 明确不做

- 不通过关闭 WAL、降低 `synchronous`、跳过 flush 或伪造缓存命中改善指标。
- 不把 mock provider 的限流处理带入生产默认值。
- 不新增版本化模块或旧路径兼容层；报告器改造直接修改现有基准与 `cce-bench-kit`。
- 不在缺少阶段拆分数据时继续调整批量参数。
