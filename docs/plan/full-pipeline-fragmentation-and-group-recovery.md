# full-pipeline 相对 direct / raw 的下滑分析与修改方案

## 1. 问题定义

基于 `crates/app/cce-e2e-tests/outputs/benchmark/flask` 的基准数据，`full_pipeline` 在部分条件下相较 `direct_chunking`、`full_pipeline_raw_source` 出现下滑。下滑不是整体检索能力下降，而是集中在以下场景：

- 精确范围评测（无 `_group` 后缀）中的小 top-k 召回
- 被切分为多个 fragment 的长方法
- BM25 单路检索中的个别语义查询
- embedding 单路检索中的 qualified / fuzzy / semantic 查询

生产对齐的 group 展开路径（`*_group`）大多恢复，说明问题主要来自 fragment 粒度与 top-k 窗口，而不是 group span 本身。

## 2. 数据与评测依据

### 2.1 基线与规模

`outputs/benchmark/flask/run_manifest.txt` 记录：

- `full_pipeline`：embedding 843 chunks，BM25 1198 chunks
- `full_pipeline_raw_source`：embedding 843 chunks，BM25 1198 chunks
- `direct_chunking`：embedding 593 chunks，BM25 1816 chunks

`outputs/benchmark/flask/chunk_shape_diagnostics.md` 记录：

- direct BM25 重复文本占比 `0.137`
- full_pipeline BM25 重复文本占比 `0.071`
- direct BM25 chunk 数 `1816`
- full_pipeline BM25 chunk 数 `1198`

### 2.2 总体指标

`outputs/benchmark/flask/aggregate_metrics_top5.md` / `top10.md` 中，`impl_only` 变体的关键对比：

| 条件 | full_pipeline | direct | raw_source |
|---|---:|---:|---:|
| BM25 top5 `R_s` | 0.7111 | 0.7778 | 0.6444 |
| BM25 top10 `R_s` | 0.7556 | 0.7778 | 0.7333 |
| BM25_group top5 `R_s` | 0.9111 | 0.9333 | 0.9111 |
| emb top10 `R_s` | 0.7333 | 0.7111 | 0.4222 |
| emb top5 `R_s` | 0.5333 | 0.5333 | 0.2889 |

结论：full-pipeline 的 embedding 明显优于 raw-source，BM25 精度/F1 优于 direct；但 BM25 小 top-k 精确召回和部分 group top5 略低于 direct。

## 3. 逐查询下滑证据

`outputs/benchmark/flask/per_query_results_top10.md` 中，`impl_only`、top10 的 `R_s` 下滑样例：

| 查询 | 类型 | retriever | full_pipeline | direct | raw_source | group 是否恢复 |
|---|---|---|---:|---:|---:|---|
| `G1Q7` | qualified | emb | 0.0000 | 1.0000 | 0.0000 | 是 |
| `G2Q1` | semantic | emb | 0.0000 | 1.0000 | 0.0000 | 是 |
| `G2Q17` | semantic | emb | 0.0000 | 1.0000 | 0.0000 | 是 |
| `FZ-G1Q1-paraphrase` | fuzzy | emb | 0.0000 | 1.0000 | 0.0000 | 是 |
| `G2Q14` | semantic | BM25 | 0.0000 | 1.0000 | 0.0000 | 是 |
| `G2Q19` | semantic | emb_group | 0.0000 | 1.0000 | 0.0000 | 否 |

`G2Q19` 是唯一在 group 路径上仍失败的样例，说明其所有相关 fragment 都没有进入 top-k。

## 4. chunk 与源码对照

### 4.1 `Blueprint.register` 被切成多个 fragment

查询 `G1Q7` 对应 `fixtures/python/review/flask/src/flask/sansio/blueprints.py` 的 `register` 方法，目标范围为 `273-377`。

`outputs/scenarios/python/chunks/flask/emb/src/flask/sansio/blueprints.py.txt` 中：

- chunk 13：`group_2319_emb_9`，覆盖 lines `273-303`
- chunk 14：`group_2319_emb_10`，覆盖 lines `304-331`
- chunk 15：`group_2319_emb_11`，覆盖 lines `332-377`

BM25 版本中同样被切成：

- `group_2646_bm25_10`，覆盖 lines `312-346`
- `group_2646_bm25_11`，覆盖 lines `347-377`

因此 `R_s` 只有在多个 fragment 同时被召回时才能恢复。top10 精确评测中，full-pipeline embedding 未能覆盖整段，`R_s=0`；`emb_group=1`，说明至少一个同 segment fragment 被召回并展开为 group span。

direct 的单块 raw source 不需要多 fragment 联合覆盖，因此 `R_s=1`。

### 4.2 `add_url_rule` 被切成多个 fragment

查询 `G2Q9` 对应 `src/flask/sansio/app.py:605-661`。

`outputs/scenarios/python/chunks/flask/bm25/src/flask/sansio/app.py.txt` 中：

- `group_1312_bm25_24`，覆盖 lines `604-649`
- `group_1312_bm25_25`，覆盖 lines `650-661`

BM25_group top5 中 full-pipeline 为 `0.0000`，direct 为 `1.0000`。这表明 full-pipeline 的两个 fragment 都未进入 top5，group 展开也无法恢复。

### 4.3 `get_flashed_messages` 被切成两个语义 fragment

查询 `G2Q19` 对应 `src/flask/helpers.py:360-400`。

`outputs/scenarios/python/chunks/flask/emb/src/flask/helpers.py.txt` 中：

- `group_5845_emb_0`，覆盖 lines `360-379`
- `group_5845_emb_1`，覆盖 lines `380-399`

full-pipeline 的 `emb_group` top10 为 `0.0000`，direct 为 `1.0000`。这说明两个 fragment 都未被召回，group 展开失效。

## 5. 代码依据

### 5.1 精确召回只在 fragment 联合覆盖时恢复

`crates/app/cce-e2e-tests/src/range_evaluator.rs` 中的 `coverage_over_retrieved`：

- `recovered[index]` 要求 retrieved chunks 的 intervals 合并后覆盖整个 `[start, end]`。
- 因此一个 fragment 即使命中目标方法，也只有在所有必要 fragment 被召回时才算 recovered。

### 5.2 full-pipeline 与 direct 的 chunk 生成差异

- `crates/app/cce-e2e-tests/src/baselines/full_pipeline.rs`：单次 parse 后按 `ChunkPath::Embedding / ChunkPath::Bm25` 拆分，保留 NL 摘要与混合文本。
- `crates/app/cce-e2e-tests/src/baselines/direct_chunker.rs`：直接 `GroupChunker` 生成 chunk，embedding 文本为 raw source，粒度更粗。
- `crates/app/cce-e2e-tests/src/baselines/full_pipeline_raw.rs`：保留 full-pipeline 的 chunk 结构，但将文本替换为按 `source_ranges` 切出的 raw source，并截断到 7200 tokens。

### 5.3 group 展开与去重规则

`docs/benchmark_group_alignment.md` 与 `crates/app/cce-e2e-tests/src/judgments/evaluate.rs` 中：

- `RangeMode::GroupFilled` 将同 `segment_id` 的所有 chunk 的最小 `start_line` / 最大 `end_line` 作为 group span。
- 评测扫描 ranked list 时按 `segment_id` 去重；同一 segment 的后续 fragment 被跳过。

这意味着 full-pipeline 的多个 fragment 在 top-k 中可能只贡献一个 group slot，小 top-k 下 distinct group 覆盖会下降。

## 6. 根因归纳

1. **fragment 精确评测要求整段覆盖**：full-pipeline 的方法被切成多个 fragment，top-k 必须拿到足够 fragment 才能 recovered；direct 的粗粒度 chunk 天然更容易 recovered。
2. **group 展开有前提**：只有至少一个同 segment fragment 进入 top-k，group span 才能恢复；`G2Q19` 证明该前提可能失败。
3. **小 top-k slot 浪费**：full-pipeline 的多个 fragment 可能来自同一 method/class，在 group 去重后只算一个 group，降低 distinct target 覆盖。
4. **NL 转换改变 BM25 词面信号**：full-pipeline BM25 使用 NL + title + keywords，而不是 raw source；对大多数 query precision 更高，但个别方法的强词面信号被分散到多个 fragment。
5. **embedding fragment 语义稀释**：切分后的 fragment 可能只包含部分实现细节，语义 paraphrase query 的鲁棒性低于 direct 的完整 raw source chunk。

## 7. 修改方案

### 7.1 保留 fragment，但保证每个 fragment 的可检索性

- 每个 fragment 必须保留方法名、类名、参数名、返回类型与关键调用链。
- 对 `SentenceBoundary` / `MemberBoundary` 产生的 fragment，避免只保留局部实现语句。
- BM25 文本中重复出现方法名、类名、异常名和关键 API 名，防止 fragment 之间互相竞争时丢失强信号。

### 7.2 为长方法增加方法级聚合 chunk

- 对 token 超过阈值的方法，除了现有 fragment，再生成一个方法级 chunk。
- 方法级 chunk 文本使用 NL 摘要 + 关键 raw source 片段，embedding 路径优先保证方法整体语义。
- BM25 路径可保留细粒度 fragment，用于精确定位；embedding 路径增加方法级 chunk，用于整体召回。

### 7.3 检索层增加 segment 级二次扩展

- 当 top-k 中出现某个 `segment_id` 的 fragment 时，自动扩展该 segment 的其他 fragment 或 group span。
- 对未被 top-k 命中的 segment，不扩展，避免噪声。
- 该机制应与 `docs/benchmark_group_alignment.md` 中的 group span 物化保持一致。

### 7.4 调整 chunker 的合并策略

- 增大 BM25 合并 ceiling / min threshold，使同方法的 sentence fragment 能合并为更大的 chunk。
- 保留 Entity → Sentence → Token 的逐级降级，不直接跳过 sentence 层。
- 对 `merged_group` 与 `FunctionWithMembers` 保持 owner 语义，避免标题退化为泛化名称。

### 7.5 embedding 摘要保留强标识符

- NL 摘要中保留方法名、类名、异常类型、关键调用。
- 避免摘要只描述通用行为，导致 `register`、`dispatch_request`、`wsgi_app` 等方法互相混淆。
- 对 paraphrase query，增加方法名与调用链的重复权重。

### 7.6 评测与回归

- 同时运行 precise 与 group 两套指标，分别反映 chunk 质量与生产 API 覆盖。
- 对 `G1Q7`、`G2Q1`、`G2Q14`、`G2Q17`、`G2Q19`、`FZ-G1Q1-paraphrase` 建立专项回归检查。
- 重新生成 Flask chunks 后，确认：
  - 长方法不再完全依赖多个小 fragment 才能 recovered
  - group 指标不低于 direct 的对应值
  - BM25 precision 不显著下降

## 8. 预期效果

- full-pipeline 的 precise `R_s` 接近 direct 的粗粒度 chunk。
- group 指标稳定不低于 direct。
- embedding 对 qualified / fuzzy / semantic query 的鲁棒性提升。
- BM25 保持高 precision，同时降低长方法被切分后的召回损失。

## 9. 风险与边界

- 增加方法级 chunk 会增加 chunk 数量和 embedding 成本。
- 保留更多 raw source 可能增加 BM25 噪声，需要通过 title/keywords 控制。
- group 展开只能恢复已被召回的 segment，不能替代 chunk 级召回质量。
- 评测改进不应掩盖 chunk 本身的质量问题；precise 指标仍需保留。