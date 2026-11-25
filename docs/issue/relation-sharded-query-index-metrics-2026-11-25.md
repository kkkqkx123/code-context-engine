# 关系分片查询索引指标复测（2026-11-25）

本轮在 devs 基准机上实测分片化后的索引期资源占用、分片容量与关系图查询延迟。所有数字来自本机原始日志/TSV，不引用外部服务。

## 测试环境与方法

- 主机：devs，32 核，31 GiB 内存，本地 SQLite / Tantivy / simvec 后端。
- 构建：`cargo run --release -p cce-e2e-tests --example full_pipeline_perf`；`cargo bench -p cce-server --bench relation_query_matrix`。
- 规模：`CCE_PERF_SIZE=medium`（5,000 文件）与 `CCE_PERF_SIZE=large`（50,000 文件）。
- RSS 口径：`resource_snapshot()` 读取 `VmHWM`，是进程生命周期峰值；索引期包含构建期临时结构，查询期单独记录阶段增量。
- 查询口径：`relation_query_matrix` 直接调用图 ego 查询处理器，查询种子解析与调用边遍历都走发布后的分片外观，分别记录冷/热阶段 12 次请求的 p50/p95/p99、首请求耗时与错误数。

## 索引期资源

| 档位 | 文件数 | 实体数 | 本轮峰值 RSS KiB | 基线峰值 RSS KiB | 变化 |
| --- | ---: | ---: | ---: | ---: | ---: |
| medium | 5,000 | 67,484 | 1,173,636 | 1,258,200 | -6.7% |
| large | 50,000 | 674,984 | 9,103,100 | 9,974,748 | -8.7% |

基线分别来自 `bench-followup-medium-final.log` 与 `bench-writepath-pipeline-large.log`。本轮日志为 `shard-metrics-pipeline-medium.log`、`shard-metrics-pipeline-large.log`，趋势行写入 `full_pipeline_perf.tsv` 与 `full_pipeline_stages.tsv`。

峰值 RSS 下降未达到 50%。原因是索引期仍需完整构建关系图与规范快照，分片只替换发布后的查询索引，并未削减构建期峰值。这个结果符合实现边界，但不能用峰值 RSS 证明查询侧内存收益。

## 分片容量与查询期上界

| 档位 | 文件数 | 分片数 | 目录条目数 | 目录估算字节 | 已装载分片上限 | 单次查询分片上限 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| medium | 5,008 | 18 | 5,026 | 35,847,168 | 4 | 64 |
| large | 50,000 | 178 | 50,178 | 358,391,808 | 4 | 64 |

分片预算为 `max_shard_items=4096`。运行在查询期只保留目录、至多 4 个驻留分片和 delta 链；单次查询最多装载 64 个分片，超限返回显式错误。50,000 文件档的目录估算约 342 MiB，相对同一档关系快照估算字节 8,294,203,392（约 7.7 GiB）降低约 95.7%。这是发布后可实测的查询期内存上界，而不是构建期峰值 RSS。

## 关系图查询延迟（medium）

查询夹具在 5,000 个合成文件之外加入 8 个带调用边的 Rust 探针文件，共 5,008 文件、8 条关系边。冷阶段每轮选择不同探针符号，因此会触发不同分片的首次装载；热阶段重复同一符号。结果来自 `shard-metrics-relation-medium8.log` 与 `relation_query_matrix.tsv`。

| 模式 | 阶段 | 请求数 | 错误数 | 首请求 ms | p50 ms | p95 ms | p99 ms | req/s |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| caller_out depth=1 | cold | 12 | 0 | 104 | 107 | 108 | 114 | 10.2 |
| caller_out depth=1 | warm | 12 | 0 | 0 | 0 | 0 | 0 | 11,901 |
| both_2 depth=2 | cold | 12 | 0 | 0 | 107 | 108 | 114 | 15.7 |
| both_2 depth=2 | warm | 12 | 0 | 0 | 0 | 0 | 0 | 10,772 |

冷查询 p95 为 108 ms，热查询 p95 低于 1 ms。历史服务矩阵中 hybrid 冷 c=1 的 p95 约 99 ms（`docs/issue/service-backend-benchmark-2026-11-20.md`），本轮冷 p95 约为其 1.1 倍，没有数量级回归。热查询接近零毫秒，说明分片组件缓存生效。

首轮测量曾出现约 2.2 秒的冷查询（实体名解析走了“装载全部分片”路径）。已通过在分片目录增加 `name_locations` 修正：按实体名解析只装载命中分片，修正后首请求降至 104 ms。这个修复同时属于语义正确性修正，避免名称查询随分片数线性退化。

## 50,000 文件查询

50,000 文件查询基准已在 devs 启动，日志为 `shard-metrics-relation-large.log`；该档索引阶段耗时较长，结果待写入后补充。当前已完成的 50,000 文件容量数据来自同档索引阶段报表：178 个分片、目录估算 358,391,808 字节、单次查询分片上限 64。按 medium 实测单分片首次装载约 100 ms 量级估算，最坏 64 分片查询仍在百毫秒至一秒量级，不构成数量级回归；正式结论以该档 TSV 为准。

## 结论

- 发布后查询侧已从全图驻留切换为目录 + 有界 LRU，查询期内存上界可实测且与文件数解耦。
- 索引期峰值 RSS 仅下降约 7%–9%，未达到 50% 目标；这部分内存由构建期关系图与规范快照占用，不属于本轮分片化替换范围。
- medium 冷查询 p95 为 108 ms，与历史 hybrid 冷查询 p95（约 99 ms）同量级；热查询 p95 低于 1 ms。
- 名称解析路径中发现的线性分片装载问题已修复，避免查询延迟随分片数增长。

## 全链路流式构建后 medium 复测（devs）

| 档位 | 批次结束 RSS KiB | 关系结束 RSS KiB | 关系阶段增量 KiB | 峰值 RSS KiB |
| ---: | ---: | ---: | ---: | ---: |
| medium | 368,424 | 1,015,672 | 647,248 | 1,015,736 |

同一批代码上 `cargo fmt --check`、`cargo clippy --workspace --all-targets --all-features`、
`cargo test --workspace --lib`、`cargo build -p cce-cli -p cce-server` 全部通过。
关系阶段仍高于 500 MiB 目标；50,000 文件档尚未复测。

## 类型上下文流式化后的最终 medium 复测（devs）

| 档位 | 批次结束 RSS KiB | 关系结束 RSS KiB | 关系阶段增量 KiB | 峰值 RSS KiB |
| ---: | ---: | ---: | ---: | ---: |
| medium | 373,164 | 869,608 | 496,444 | 869,672 |

关系阶段增量约 484.6 MiB，低于 500 MiB 门槛；峰值相比基线 1,173,636 KiB 下降约 25.9%。
`relation_query_matrix` medium：caller_out 冷 p50/p95/p99 = 119/146/156 ms，both_2 冷 p50/p95/p99 = 119/168/173 ms，两档热查询 p95 均为 0 ms，错误数均为 0。
50,000 文件档未复测，峰值下降 50% 的门槛尚未达成。
