# 性能问题索引

本目录收录依据 `docs/tests/performance_hotspots.md` 与 `docs/tests/benchmark_gaps.md`
落地的一批微基准测试的实测分析结果。所有基准均为小数据量、对比导向，
使用 dev（debug）模式编译运行，数值只具相对意义，不作为绝对 SLO。

## 基准一览

| 基准 | 所在 crate | 运行命令 | 趋势文件 | 覆盖的 gaps 条目 |
| ---- | ---------- | -------- | -------- | ---------------- |
| scan_hash | cce-scanner | `cargo bench --profile dev -p cce-scanner --bench scan_hash` | `crates/infra/cce-scanner/benches/results/scan_hash.tsv` | 1.1 |
| parse_nl_chunk | cce-parser | `cargo bench --profile dev -p cce-parser --bench parse_nl_chunk` | `crates/parser/cce-parser/benches/results/parse_nl_chunk.tsv` | 3.1、3.2、3.4 |
| mixed_tokenizer | cce-text | `cargo bench --profile dev -p cce-text --bench mixed_tokenizer` | `crates/core/cce-text/benches/results/mixed_tokenizer.tsv` | 3.6 |
| bm25_query | cce-storage-bm25 | `cargo bench --profile dev -p cce-storage-bm25 --bench bm25_query` | `crates/infra/cce-storage-bm25/benches/results/bm25_query.tsv` | 5.1、5.2 |
| metadata_read | cce-storage-sqlite | `cargo bench --profile dev -p cce-storage-sqlite --bench metadata_read` | `crates/infra/cce-storage-sqlite/benches/results/metadata_read.tsv` | 5.3 |
| relation_perf | cce-relation | `cargo bench --profile dev -p cce-relation --bench relation_perf` | `crates/parser/cce-relation/benches/results/relation_perf.tsv` | 2.3、2.5、5.4 |
| checkpoint_codec | cce-orchestrator | `cargo bench --profile dev -p cce-orchestrator --bench checkpoint_codec` | `crates/app/cce-orchestrator/benches/results/checkpoint_codec.tsv` | 1.3 |
| hot_update_scaling（已有） | cce-orchestrator | `cargo bench -p cce-orchestrator --bench hot_update_scaling` | `benches/results/hot_update_scaling.tsv` | 2.1 |

详细结果与问题分析见 `performance_analysis.md`。

## 问题严重程度速览

| 编号 | 问题 | 严重程度 | 对应分析节 |
| ---- | ---- | -------- | ---------- |
| ISSUE-01 | 全文检索高亮逐命中成本线性放大 | 高 | 2.1 |
| ISSUE-02 | 索引期同一文件被读取两次 | 高 | 2.2 |
| ISSUE-03 | 自然语言双路径耗时直接相加 | 高 | 2.3 |
| ISSUE-04 | 未传作用域时增量差分退化为全量对比 | 高 | 2.4 |
| ISSUE-05 | 文本块批量查询与逐行查询的倍数差 | 高 | 2.5 |
| ISSUE-06 | 大文件单文件解析耗时占比过高 | 中 | 2.6 |
| ISSUE-07 | 层叠快照规范化耗时随基规模增长 | 中 | 2.7 |
| ISSUE-08 | 检查点编解码在大文件上不可忽略 | 中 | 2.8 |
| ISSUE-09 | 中文分词单位字符成本高于标识符切分 | 低 | 2.9 |
| ISSUE-10 | 未复现项与尚未覆盖的基准缺口 | 跟踪 | 3、4 |
