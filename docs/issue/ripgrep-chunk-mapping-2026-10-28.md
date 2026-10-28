# Ripgrep 回归问题清单与修复方案（逐查询 chunk 映射）

状态：BM25 已恢复，emb 待转换器文本治理
对比基线：`ref/ripgrep/bench`（旧，`6db71d7`） vs `crates/app/cce-e2e-tests/outputs/benchmark/ripgrep`（新，2026-10-28 重跑 + 本轮修复后）
口径：`variant=all, baseline=full_pipeline, k=5`，只看 `R_s/s_m`，`R_r` 两边语义不一致不纳入对比。

## 1. 总览（2026-10-28 重跑修正）

旧磁盘 `bench_data`（`704/889`，52 文件）为脏数据：缺 `walk.rs/globset/lib.rs/matcher/lib.rs/searcher/mod.rs/types.rs` 等 39 文件，直接导致旧 `per_query` 21/16 全零。已重跑 `gen_bench_ripgrep`，当前 `bench_data` 为 `full 2579/4213`（88+3 文件，`Excluded 50 doc emb/76 doc bm25`），`direct 2636/3888`。

同口径新鲜对比（`all/full_pipeline/top5`）：
- `direct BM25 R_s 0.4222->0.4333`，`direct emb 0.4556->0.4444`：基本恢复，非回归。
- `full BM25 0.40->0.3222 (-0.078)`，`full emb 0.5889->0.4333 (-0.156)`。
- `raw BM25 0.4778->0.3444`，`raw emb 0.4889->0.3111`。
- `per_query` 真回归收窄到 `emb 9` + `BM25 8`（下节），`G1Q2 walk.rs` 已恢复，旧映射中 walk 簇结论作废。

场景导出（`export_rs`，`Both` 单文件口径）`emb 1638->2615 (+60%)`，`bm25 1584->3735 (+136%，修复前 4258，位置计数与段落路由后 -12%)`，与 bench `1645->2578 (+57%) / 2698->3688 (+37%，修复前 4213)` 同向。`export_rs` 直接看单文件产物，用于定位；`bench` 决定 `per_query` 分数。两者必须分开，不可拿 scenario 数解释 bench 缺文件问题。

本轮修复后同口径（`all/full_pipeline/top5`）：
- `full BM25 R_s 0.3222->0.4333`，已超过旧基准 `0.40`；`raw BM25 0.3444->0.4778`，与旧基准持平。
- `direct BM25 3888->3595`，`full BM25 4213->3688`。
- `full emb R_s 0.4333->0.4333`，`emb 2617->2615`，基本未动（文本膨胀在转换器层，不在分块预算内）。
- `per_query`：BM25 下跌由 8 收窄到 5（`FZ-G1Q2/FZ-G1Q8/FZ-G1Q5-paraphrase` 恢复，`FZ-G1Q5-naming_case/FZ-G1Q6/FZ-G1Q9/G1Q6/G1Q9/G2Q5` 上升），emb 仍为 9。

## 2. 逐回归查询与 chunk 对应表（修复后：emb 9，BM25 5）

目标区间来自 `ref/ripgrep/bench/relevance_top5.md`。
`export_rs` 文件为 `ref/ripgrep/chunks/{emb,bm25}/...` vs `outputs/scenarios/rust/chunks/ripgrep/{emb,bm25}/...`（`export_rs` 单文件产物，直接可见）。

修复后 `per_query_top5`（`all/full_pipeline`）下跌清单：
- emb 9（未动）：`FZ-G1Q5-paraphrase, G1Q1, G1Q10, G1Q8, G2Q11, G2Q14, G2Q16, G2Q3, G2Q4`，1 上升 `FZ-G1Q6`。
- BM25 5（8->5）：`FZ-G1Q1, FZ-G1Q4, G1Q4, G2Q13, G2Q17`。`FZ-G1Q2, FZ-G1Q8, FZ-G1Q5-paraphrase` 已恢复；新增上升 6 个（`FZ-G1Q5-naming_case/FZ-G1Q6/FZ-G1Q9/G1Q6/G1Q9/G2Q5`）。
- `G1Q2/FZ-G1Q2 walk.rs` 保持恢复。

### 2.1 literal 复制块（G2Q16，`11-42/44-123`，emb 未修复）

`export_rs emb literal.rs.txt 28->57`（修复前后 56->57，基本未动）。
- ref `CHUNK 1/28 lines 11-42 tokens 499 MemberBoundary`：单块覆盖 struct 全 doc。
- cur `CHUNK 1/57 lines 11-40 tokens 494 SentenceBoundary` + `CHUNK 2/57 lines 41-41 tokens 497 SentenceBoundary`：同 doc 近重复块，`source_span` 失准（`41-41` 却含全文），`R_s` 无法命中 `11-42`。
判定：同一 group 文本内 doc 出现两次（模板自然语言版 + 侧车原始版），属转换器文本重复，分块层修不动。按方案留到摘要文本治理，不关富化（关富化等于删检索信号）。
- BM25 侧 `literal 95->81`，随位置计数回落。

### 2.2 pathutil 合并丢失（G2Q17，`5-45`，BM25 部分恢复）

`export_rs emb pathutil.rs.txt 4->7`（未动）；`bm25 10->7`。
- ref `CHUNK 1/4 lines 5-57 tokens 511 MemberBoundary`：3 个 cfg 变体 `is_hidden` 合一。
- cur emb 仍拆三块，目标 `5-45` 横跨两块。
- G2Q17 BM25 仍下跌，emb 侧未覆盖查询（emb 下跌表无 G2Q17，见 §2 头部清单以 `per_query` 为准）。
判定：group 合并丢失 + 转换器富化变长仍在，属下一步文本治理范围。

### 2.3 其余目标文件（export_rs 直接可见，修复后）

- `walk.rs emb 159->157 / bm25 228->200`，`pathutil bm25 10->7`，`globset emb 75->75`，`matcher/lib emb 81->82`，`regex/matcher emb 40->41`。
- emb 侧小目标（`pcre2 32-39 7 行`，`pathutil 5-45`，`globset 341-344 3 行`，`literal 11-42`）仍命中，待文本治理。
- BM25 侧 `G1Q4/FZ-G1Q1/FZ-G1Q4/G2Q13/G2Q17` 5 个剩余下跌见 §2 清单。

## 3. 根因与处置（已验证，不做兼容性回退）

1. BM25 计量由分词器词条总数改为位置计数（`budget.rs`，原词与切分词共享位置计一，CJK 保持词典粒度；检索分词不变）。`full BM25 4213->3688`，`R_s 0.3222->0.4333` 超过旧基准。不做空白词回退（中文会退化）。
2. 装饰预扣归零实验（`chunk_builder.rs` return 0）已失败回退：目标文件未收敛且 log 与磁盘不一致。证伪：单纯放大 `body` 不修复重复/拆分。
3. 续片短身份保持不变，不恢复签名（签名长于正文会稀释 budget，属遗留做法，按规则不恢复）。
4. 默认值漂移已按仓库文档修复（`ast_to_nl.rs`：`overlap 25->50/min 200->150/overlap_bm25 3->5/min_bm25 60->45`，与结构体注释及三个红测试一致；旧 `min_bm25_words 80` 是 words 口径遗留值，当前 terms 口径以文档 `45` 为准）。三红测试已绿。
5. BM25 分割恢复粗粒度：实体超限按段落细分，不经过句子层；emb 保持句子细分（`splitter.rs` 路径感知 `next/subdivide`）。
6. 单实体小幅超限整体保留：单块适配 `pre_injection` 即保留，不再拆成大块加小尾（`splitter.rs` 单块分支 + `segment_limit.rs` 同阈值）。仅整块保留才安全：单段无续片身份追加，定语注入以 hard 为界，最终不超 hard。
7. 转换器富化（`control flow/calls`、层级前缀、术语表）与 group 合并丢失是 emb 侧剩余 9 个下跌的主因，属摘要文本治理范围，不在本轮分块修复内，也不做富化开关（删信号）。literal 重复为转换器同 doc 双份，非分块重复。

## 4. 修复方案（已执行，单编译验证）

1. 实验 B 已做：装饰归零，失败回退，不再做。
2. 位置计数（替代实验 A 的空白词回退）：`ChunkUnit::Terms` 按位置计数，`bm25 4213->3688`，`FZ-G1Q2/FZ-G1Q8/FZ-G1Q5-paraphrase` 恢复，emb 不动。空白词回退不做。
3. 默认值对齐仓库文档（`50/150/5/45`），三红测试已绿。
4. BM25 段落路由 + 单实体小幅保留（替代实验 E 的富化开关与实验 C 的签名恢复）：`walk bm25 228->200`，`pathutil bm25 10->7`；富化开关与签名恢复不做（删信号/遗留回退）。
5. 下一步（文本治理，不属分块）：转换器同 doc 双份（literal）、cfg 变体合并丢失（pathutil）、层级前缀重复。按方案 `docs/plan/ripgrep-benchmark-regression-fix.md` 留到摘要文本质量阶段。

## 5. 验证方法与本轮结果（export_rs + bench 已重跑）

每个实验必须同时输出三项：
1. `cargo run --example export_rs` 后抽查（以 `export_rs` 产物为准，直接可见）：
   `grep -c "=== CHUNK" <file>` + `grep -o "split_reason: [A-Za-z]*" | sort | uniq -c` + 头块 `lines/tokens` 是否重复。注意 `log` 与磁盘不一致时以磁盘为准，0 chunk 文件说明有残留旧文件，必须 `rm` 后重跑或看 log。
2. 重跑 `gen_bench_ripgrep` + `benchmark_ripgrep`（`bench_data` 决定分数），同口径（`all/full_pipeline/emb,BM25/top5`）逐查询 `s_m/R_s`，目标 `>=1/回到 1.0/0.5`。`aggregate_top5` 仅辅助，必须注 `R_r` 新旧语义不一致。
3. 本轮已验证结果：单元测试 `cce-text 160 + cce-parser 1260 + cce-config 36` 全绿，`cargo clippy --all-targets --all-features` 干净；`export_rs 2615/3735`，`bench full 2578/3688 / direct 2646/3595`；`full emb R_s 0.4333（持平）/ BM25 0.4333（+0.111，超旧基准 0.40）/ raw BM25 0.4778（持平旧基准）`，`per_query` emb 9 未动、BM25 8->5。

通过标准示例（部分达成）：`walk bm25 228->200`，`pathutil bm25 10->7`；未达成：`literal emb 56->57 仍重复`，`pathutil emb 7->7 未合并`，`G2Q16/G2Q17 R_s 未回 1.0`（待文本治理）。
