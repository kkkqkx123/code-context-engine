# 模块表磁盘化前置指标采集与性能定位（medium）

本轮在 devs 上完成剩余任务的前置指标采集。主要验证环境为 medium（5,000 文件）；large 档单次运行耗时过长，只作为历史数据用于判断非线性，不再作为迭代反馈。

## 采集方法

- 索引：`CCE_PERF_SIZE=medium CCE_RELATION_RSS_TRACE=1 cargo run --release -p cce-e2e-tests --example full_pipeline_perf`
- 查询：`cargo bench -p cce-server --bench relation_query_matrix -- --nocapture`
- 质量门禁：`cargo test --workspace --lib`、`cargo clippy --all-targets --all-features`、`cargo build -p cce-cli -p cce-server`
- 日志：`bench-metrics-medium-final.log`、`bench-metrics-query.log`、`test-metrics-raw.log`、`clippy-metrics-raw.log`、`build-metrics-raw.log`
- 结构化数据：`outputs/benchmark/full_pipeline_perf.tsv`、`outputs/benchmark/full_pipeline_stages.tsv`

为定位关系阶段，stage TSV 新增 `relation_symbol_register_ms`、`symbol_propagation_ms`、`relation_register_ms`、`relation_resolve_ms`、`relation_artifact_ms`、`relation_publish_ms`、`relation_publish_store_ms`、`relation_state_mark_ms`、`finalize_index_ms` 九列。

## medium 结果

| 指标 | 数值 |
| --- | ---: |
| 文件数 / 成功数 / 失败数 | 5,000 / 5,000 / 0 |
| 实体数 / 向量数 | 67,484 / 5,000 |
| 总 wall ms | 388,031.2 |
| relation 阶段 ms | 348,634 |
| 批次结束 RSS KiB | 384,944 |
| 关系结束 RSS KiB | 707,848 |
| 关系阶段增量 KiB | 322,904 |
| 峰值 RSS KiB | 707,988 |
| 相对 shard 基线 1,173,636 KiB | -39.7% |
| shard 数 / 目录条目 / 目录字节 | 18 / 5,018 / 35,831,808 |
| canonical 写出批次 / 缓冲峰值字节 | 152 / 15,920,410 |

关系阶段增量仍低于 500 MiB（512,000 KiB），与上一轮 324,500 KiB 一致。峰值在 694,992–707,988 KiB 区间波动，没有回到磁盘化之前的 866,824 KiB。

## 关系阶段分解

| 子阶段 | ms | 占 relation 阶段 |
| --- | ---: | ---: |
| 符号注册 / 局部表落盘 | 2,170 | 0.6% |
| 跨文件传播 | 79,432 | 22.8% |
| 关系实体注册 | 13,978 | 4.0% |
| 关系解析 | 901 | 0.3% |
| canonical artifact 写出 | 39,874 | 11.4% |
| 发布（总计） | 212,160 | 60.9% |
| ├─ 发布存储与快照装载 | 210,925 | 60.5% |
| └─ relation 状态标记 | 567 | 0.2% |
| finalize_index | 0 | 0% |

发布存储是 medium 关系阶段的主要成本。`TestRelationPublisher` 只构造一次 `RelationSnapshotIndex::from_store`，因此 210,925 ms 不是重复装载造成的；它来自把 72,484 条 canonical 行写入 SQLite 并从 store 重建分片视图。生产发布路径此前存在一次重复 `from_store`，已改为复用第一次构造的 `Arc`，但基准使用测试 publisher，所以这项修复不会体现在本轮 medium 数字中。

跨文件传播耗时在两次 medium 运行间从 37,309 ms 增至 79,432 ms。这个波动需要继续观察；它目前不是 medium 的主要瓶颈，但 large 档会因同名候选无界扫描被放大。

## large 非线性分析

历史 large stage TSV 与 medium 对比：

| 指标 | medium | large | 倍数 |
| --- | ---: | ---: | ---: |
| 文件数 | 5,000 | 50,000 | 10x |
| relation_ms | 256,322 | 33,010,512 | 129x |
| symbol_propagation_ms | 37,309 | 7,274,302 | 195x |
| relation_publish_ms | 172,540 | 未细分 | — |
| 峰值 RSS KiB | 694,992 | 4,631,348 | — |

合成夹具每个文件重复使用 `fn_00000` 等函数名。`CrossFilePropagator.name_index`、`name_to_ids`、`param_name_index` 和 `ProjectSymbolTable.simple_name_index` 都按名称持有无界候选向量；跨文件传播与名称解析需要逐个比较 arity 和参数形状。候选数随文件数线性增长，导致整体接近二次复杂度。这是 large 远慢于 medium 的主因，不能用磁盘 I/O 解释。

结论：当前确实存在性能问题。medium 的主要成本是发布存储写入与快照重建；large 的额外非线性来自同名候选无界扫描。前者需要批量化发布写入，后者纳入模块表磁盘化设计中的 `(name, arity)` 有界候选索引。

## 一致性与查询

- workspace `cargo test --lib`、`cargo clippy --all-targets --all-features`、CLI/server 构建全部通过。
- 已有单测验证磁盘关系 artifact 与参考 canonical artifact 的双 fingerprint 和行计数一致；局部表往返单测验证实体内容、数量与作用域链一致。
- `relation_query_matrix`：cold `caller_out` p95 58 ms、warm p95 0 ms、错误数 0；`both_2` cold/warm p95 0 ms。

## 当前结论

- medium 关系阶段增量达标，峰值下降约 40%，查询延迟与一致性通过。
- 剩余任务的设计文档为 `docs/plan/relation-index-module-table-diskization-design.md`。
- 后续只使用 medium 验证；large 在修复有界候选索引后再做一次冒烟。

## 细化修改与复测结果

本轮先落地两项已定位的根因，不提前引入完整磁盘化改造：

1. 发布写入把规范符号查询改为先按项目、epoch、路径定位文件，再用文件符号索引查实体。原查询从实体侧按名称扫描，热门同名符号会让每次实体查找退化为全桶扫描；新查询的 SQLite 执行计划固定为文件唯一索引加 `(project_id, relation_epoch, file_id)` 索引。实体、关系、导出仍在同一事务内按批次写入。
2. 跨文件重载候选按 `(name, declared arity)` 分桶，解析时只访问接受当前实参个数的桶；候选排序由每次插入都排序改为每次解析排序一次；按调用点形状缓存解析结果，文件增删时失效。缓存容量有上限，超出后整体清空，避免常驻集合随项目规模增长。

复测环境仍为 devs，medium 档，使用同一批命令与结构化输出。编译与质量门禁结果：workspace 全目标 `cargo check`、`cargo build -p cce-cli -p cce-server`、`cargo test --workspace --lib` 全部通过。同步清理了 clippy 的预存告警（布尔折叠、冗余借用、未使用依赖与条件编译引入的未使用项），`cargo clippy --all-targets --all-features` 最终为零 warning。

| 指标 | 修改前 | 修改后 |
| --- | ---: | ---: |
| 文件数 / 成功数 / 失败数 | 5,000 / 5,000 / 0 | 5,000 / 5,000 / 0 |
| 实体数 / 向量数 | 67,484 / 5,000 | 67,484 / 5,000 |
| 总 wall ms | 388,031.2 | 127,877.8 |
| relation 阶段 ms | 348,634 | 91,100 |
| `symbol_propagation_ms` | 79,432 | 41,753 |
| `relation_publish_store_ms` | 210,925 | 4,404 |
| `relation_artifact_ms` | 39,874 | 29,187 |
| 关系阶段增量 KiB | 322,904 | 323,756 |
| 峰值 RSS KiB | 707,988 | 688,372 |
| 相对 shard 基线 1,173,636 KiB | -39.7% | -41.3% |
| canonical 写出批次 | 152 | 152 |
| `caller_out` cold p95 ms | 58 | 52 |
| `caller_out` warm p95 ms | 0 | 0 |
| 查询错误数 | 0 | 0 |

两次优化后运行的 wall 时间分别为 201,869.7 ms 与 127,877.8 ms，关系阶段分别为 163,915 ms 与 91,100 ms；表中列出最后一次完整复测。发布存储耗时从占关系阶段 60.5% 降到 4.8%，跨文件传播也从 79,432 ms 降到 41,753 ms。两次运行存在波动，未进一步归因；实体、向量、fingerprint、关系数量和查询结果均未变化。模块记录磁盘化、视图工作集指标与候选裁剪失败语义仍按 `docs/plan/relation-index-module-table-diskization-design.md` 作为后续任务，本轮未引入第二套解析算法。
