# BM25 参数调优基准验证结论

## 背景

为验证并更新 BM25 检索参数，cce-e2e-tests 在三个 fixture（flask、once_cell、ripgrep）上执行了参数扫描（bm25_parameter_sweep）。本文件记录扫描结论与参数更新依据。

## 实验设置

| 项 | 值 |
|---|---|
| 语料 | flask（Python，1130 文档）、once_cell（Rust，141 文档）、ripgrep（Rust，2754 文档） |
| 查询 | 每 fixture 39-45 条，含 qualified / semantic / fuzzy 三类 |
| 参数组合 | 25 组：阶段一 k1×b（9 组）+ 阶段二 title_w×keywords_w（16 组） |
| 分词 | 非字母数字与下划线切分 + 小写化 |
| 指标 | MRR@10、F1@10、Recall@20 |

完整数据见 `crates/cce_e2e_tests/outputs/benchmark/{flask,once_cell,ripgrep}/bm25_parameter_sweep/`。

## 各 fixture 最优组合

| fixture | 最优组合 | MRR@10 | Recall@20 |
|---|---|---|---|
| flask | k1=1.20, b=0.75, title_w=2, kw=1 | 0.5317 | 0.9667 |
| once_cell | k1=1.50, b=0.40, title_w=2, kw=4 | 0.6322 | 0.8766 |
| ripgrep | k1=1.80, b=0.60, title_w=2, kw=4 | 0.3440 | 0.7407 |

## 验证结论

1. **title_w 是影响最大的参数，最优值为 2**：三个 fixture 一致呈现 title_w=2 > 4 > 6。ripgrep 上 title_w 从 2 增至 6 时 MRR@10 由 0.291 降至 0.175、Recall@20 由 0.744 降至 0.523；title 权重过高会严重损害长尾召回。原默认 title_w=4 并非最优，且 t=4、kw=2 的 algorithm 基线组合在全部测试中从未胜出。

2. **keywords_w 的最优值随语料特性变化，kw=2 为稳妥折中**：flask（小文档）kw=1 最优（MRR 0.521 vs kw=4 的 0.391）；ripgrep（大语料）kw=4 最优（0.344 vs 0.241）；once_cell 中 kw=4 的 MRR 最优但 kw=1 的 F1 最优。三个 fixture 的 F1@10 平均值以 kw=1 与 kw=2 并列最优，kw=4 明显下降。原默认 kw=2 保留。

3. **k1、b 影响较小，k1=1.8、b=0.6 为平均最优**：算法参数引起的 MRR 波动仅约 4%（flask 0.450-0.490，ripgrep 0.175-0.203）。对三个 fixture 的 algorithm 组合求平均，k1=1.8、b=0.6 的 MRR@10 均值最高（0.423），标准值 k1=1.2、b=0.75 亦在噪声范围内。原默认 k1=1.8 保留，b 由 0.4 更新为 0.6。

4. **语料难度差异显著**：once_cell（F1≈0.63，Recall≈0.88，接近饱和）≫ flask ≫ ripgrep（MRR 仅 0.34）。大语料下 BM25 受命中限制，召回率为主要瓶颈，字段权重（尤其 title_w）对召回的调节能力直接决定上限。

5. **查询类型差距与调参极限**：qualified（MRR≈0.60）> fuzzy/semantic（≈0.40-0.49）。per-query 数据显示 `FZ-G1Q8-naming_affix`、`FZ-G1Q9-paraphrase` 及 G2Q5/G2Q6 等查询在全部 25 组参数下 top-20 均无命中，属于分词无法弥合的措辞差异，调参无法解决，需依赖同义词扩展或向量检索兜底。

## 参数更新

| 参数 | 原默认 | 新默认 | 依据 |
|---|---|---|---|
| k1 | 1.8 | 1.8 | 三 fixture 平均最优，保持不变 |
| b | 0.4 | 0.6 | 三 fixture algorithm 组合 MRR 均值最优 |
| title_w | 4.0 | 2.0 | 三 fixture 一致最优 |
| keywords_w | 2.0 | 2.0 | F1 均值最优折中，保持不变 |
| content_w | 1.0 | 1.0 | 基准中恒为 1.0 |

涉及代码位置：

- `crates/cce_core/src/config/modules/storage.rs`（Bm25AlgorithmConfig 默认值）
- `crates/cce_core/src/config/modules/search.rs`（Bm25FusionConfig.field_weights 默认值）
- `crates/cce_infrastructure/src/storage/bm25/search.rs`（search 回退权重）
- `crates/cce_orchestrator/src/query/retrieval/core/bm25.rs`（检索回退权重）
- `config.example.toml` / `config.toml`

注意：k1/b 在 Tantivy 索引创建时写入索引设置，索引重建后生效；field_weights 为查询期参数，热生效。
