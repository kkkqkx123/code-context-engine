# 混合召回（BM25 × Embedding）片段级对齐改进方案：放弃结论

## 背景

Hybrid 召回（`HybridRecall`）为默认召回方式：vector（Qdrant）与 BM25（tantivy）两条路并行召回，在实体级对齐键（代码块 `entity_id`、文档块 `segment_id`）上做 min-max 归一化 + 加权融合（`crates/cce_orchestrator/src/query/retrieval/post_processing/fusion.rs`）。

针对"融合结果只保留 vector 路最佳分块、BM25 命中片段的内容被丢弃"的顾虑，曾评估一组片段级改进方案：

| 方案 | 内容 |
|---|---|
| A. 片段级源码定位 | 为碎片维护 sub-entity 源范围，命中时精确定位匹配行 |
| B. 保留 BM25 命中内容 | 融合时把 BM25 最佳分块的 snippet 并入结果 |
| C. 两路源范围并集 | 对双路 best chunk 的源范围取并集后重读源码 |
| D. 每键输出多分块 | 同键两路 best chunk 都保留 |
| E. 基块按源覆盖选择 | 以 span 集合更全者作为 base |
| F. dedup_by_chunk 默认开启 | 压制多实体重复膨胀 |

## 关键事实（影响评估前提）

1. **片段的源覆盖是实体级而非片段级**：硬限切分（TokenLimit/HardLimit）产生的碎片，其 `content_entity_ids` 继承整个实体，`source_coverage_for_entity_ids` 返回实体全跨度（见 `chunk_builder.rs`、`source_coverage.rs`、`segment_limit.rs`）。同一实体的所有碎片在 SQLite 中的 `raw_code` 与 `start_line/end_line` 完全相同（等于整个实体源码）。
2. **两条路的 chunk 记录均入库**（`storage_coordinator.rs:1750-1763`），召回时在融合前完成 SQLite 富化，两路结果都自带对齐键与内容。
3. 单实体碎片场景下，BM25 命中碎片与 vector 命中碎片返回的 raw_code 一致，不存在"内容丢失"；多实体成员块按不同实体键对齐，也不会产生跨碎片伪共识。

## 放弃理由

1. **每路径每实体至多保留最佳分块，浪费有界**：融合对每个对齐键、每条路径只取最高分的一个分块参与打分。理论上每条路径最多浪费 2 个片段（双路命中时两路各自的次优分块），整体可接受，该组方案收益有界且小。
2. **内容并未真正丢失**：因源覆盖为实体级，被丢弃碎片与保留分块的 raw_code 相同；真正差异只在 `content`（NL）字段，且富化时被 raw_code 覆盖，不对外呈现。
3. **可靠实现成本高**：唯一能带来实质增量的是方案 A（片段级源码定位），但需重构 chunker 使碎片携带 sub-entity 源范围，属于索引管线级改造，且需与召回基准测试结果一起评估价值，短期收益/成本不划算。
4. **剩余小问题可通过低成本配置缓解**：多实体重复膨胀（方案 F，`dedup_by_chunk` 已实现）与源定位精度为既有已知限制，均已文档化，不构成放弃默认召回路径的理由。

## 结论

**放弃片段级对齐改进方案，维持现有实体级对齐设计。**

- 融合算法（min-max 归一化、权重标定）的取舍交由召回基准测试数据驱动，待 benchmark 落地后另行评估，不在本结论范围内。
- 若未来出现"必须精确定位命中行"的强需求，再单独立项方案 A（管线重构，使碎片携带 sub-entity 源范围）。

涉及代码位置：

- 融合实现：`crates/cce_orchestrator/src/query/retrieval/post_processing/fusion.rs`
- 多实体展开：`crates/cce_orchestrator/src/query/searcher.rs`（`expand_multi_entity_results`）
- 碎片源覆盖：`crates/cce_parser/src/ast_to_nl/chunker/{chunk_builder.rs, source_coverage.rs, segment_limit.rs}`
- 索引侧对齐字段写入：`crates/cce_orchestrator/src/index/storage_coordinator.rs`
