# 召回方式基准测试设计方案

## 1. 背景与动机

现有 benchmark（`examples/{rust,python}/benchmark_*.rs`）的评测轴是 **chunking 方案 × 检索器**：固定 3 个 baseline（full_pipeline / full_pipeline_raw_source / direct_chunking），对每个 baseline 独立评测 emb（余弦相似度）与 BM25（Okapi）两条单路召回路。它回答的问题是"哪种分块方案更好"，但没有覆盖**召回方式**这一维度：

- 生产系统（`cce_orchestrator::query`）支持三种召回方式：Dense（纯向量）、Sparse（纯 BM25）、Hybrid（按 entity_id 对齐的 min-max 归一化 + 加权融合，含按查询意图的动态权重）。现有 benchmark 从未评测 Hybrid。
- `docs/benchmark/query-aggregation-benchmark-design.md` 已提出融合评测的大框架，但矩阵过大（含关系扩展、组装、聚合等多维执行模式），未落地。

本方案的目标：新增一个**召回方式基准测试**，回答"对固定 chunking 方案，emb / bm25 / minmax-hybrid（各权重）/ rrf-hybrid（各 k 值）中哪种召回方式检索质量最优、最优权重是多少"，并与生产融合逻辑口径一致。

## 2. 现状分析与关键约束

### 2.1 数据现状（实测）

`data/benchmark/{baseline}/{fixture}/bge-m3/bench_data.rkyv` 的 `BenchmarkData` 已同时携带两条召回路径的完整数据：

| 字段 | 内容 |
|------|------|
| `embedding` RetrieverDataset | emb chunk 文本、1024 维向量（chunk + query）、dimension |
| `bm25` RetrieverDataset | bm25 chunk 文本（无向量） |
| `queries` / `query_texts` | 查询集 |
| `bm25_query_terms` | 已按生产 analyzer 物化的 BM25 查询词索引 |

relevance judgments 位于 `src/judgments/{once_cell,ripgrep,flask}.rs`，为 source-range 标注（`RelevantRange` + `RelevanceLevel`）。

**关键实测发现**：full_pipeline 下 emb 与 bm25 两条路径的 chunk 是独立分块的：

- 组号（group_id）在两路径间**完全不对齐**（once_cell: emb 侧 `group_13_emb_0`，bm25 侧 `group_130_bm25_0`，公共组数为 0）。
- 以 `(file_path, entity_name)` 为键则对齐率约 85-90%（ripgrep 564/683，flask 259/291，once_cell 32/35）。

`chunker.rs` 的 Alignment Contract 明确：跨路径对齐发生在 **entity 层**（两路径共享同一批 `content_entity_ids`），而非 chunk 层。当前 `ChunkData` 已补充 `entity_ids` / `segment_id`（与生产 `ChunkMetadata` 一致），可精确按生产口径对齐。

### 2.2 现有评测的语义差异

现有单路评测按 **chunk** 计数（top-k 内每个 chunk 单独判定），同一实体被切碎的多个分片会重复占位。生产 Hybrid 融合按 **entity** 对齐并去重（`fuse_hybrid_results` 的 alignment key 为 entity_id/segment_id，每 key 只保留最优 chunk）。因此：**召回方式对比必须统一在"按对齐键去重后的排名"上进行**，否则 hybrid 去重而单路不去重，属于苹果比橘子。

## 3. 核心决策

### 3.1 复用现有数据，不重新生成，不依赖 LLM

- 直接 `load_benchmark_data` 读取现有 rkyv；emb 分数用 `cosine_similarity` 即时计算，bm25 分数用 `compute_bm25_scores`（k1=1.5, b=0.75）即时计算。
- 融合分数在评测时从同一份数据在线计算，**不新增任何数据文件**。这与 `docs/benchmark/design.md` 的既有原则一致（"BM25 评分在 evaluator 中即时计算，不依赖 tantivy 索引"）。
- 全离线、确定性强、无需 API key。

### 3.2 存储结构

- 不为每种召回方式/权重预设单独落盘数据；单一 rkyv 即完整输入。
- `ChunkData` 补充 `entity_ids: Vec<i64>` 与 `segment_id: String`，由 `chunk_data_from_result` 从生产 `ChunkMetadata` 直读（`content_entity_ids` / `segment_id`）。rkyv 结构变更，**数据需用 `gen_bench_*` 重新生成**。
- 对齐键提取集中在 `fusion.rs::alignment_key`（实体→段→chunk id 三级），单路去重与融合共用。

### 3.3 跨路径对齐键选择（核心决策）

| 方案 | 说明 | 取舍 |
|------|------|------|
| A. `(file_path, entity_name)` | 从现有 ChunkData 派生，无需再生成 | 对齐率约 85-90%；同名方法在同一文件多处定义时（如 lib.rs 中 unsync/sync 两个 OnceCell 的 `get`）会坍缩为一个键，造成 recall 低估 |
| B. `content_entity_ids` / `segment_id`（已采用） | ChunkData 增加 `entity_ids` 与 `segment_id`，与生产对齐契约一致 | 精确无碰撞；需要带 embedder 的 API key 重新跑 `gen_bench_*` |

**决策**：采用方案 B，对齐键与生产完全一致（`e:{entity_id}` → `s:{segment_id}` → `c:{chunk_id}`），并满足：

1. 融合模块的 minmax 族**直接调用生产 `fuse_hybrid_results`**（含生产 `expand_multi_entity_results` 预展开），保证评测即生产行为；RRF 族作为候选算法保留本地实现，但共享同一套对齐键语义；
2. 报告继续输出 `alignment_coverage`，使跨路径对齐失真可见。

## 4. 召回方式定义与融合算法

### 4.1 方法矩阵

| 召回方式 | 计算 | 参数预设 |
|----------|------|----------|
| `emb` | 余弦相似度 | - |
| `bm25` | Okapi BM25（复用 `compute_bm25_scores`） | k1=1.5, b=0.75 |
| `minmax-α` | 每路径 min-max 归一化后加权线性组合 | 7 组权重：0.9/0.1、0.7/0.3、0.6/0.4、0.5/0.5、0.4/0.6、0.3/0.7、0.1/0.9（vector/bm25） |
| `rrf-k` | 倒数排名融合 | k = 30 / 60 / 100 |

权重预设与 k 预设沿用 `query-aggregation-benchmark-design.md` §3.1 的取值，便于两文档结论互参。

### 4.2 融合流程（每个查询）

1. 分别对全部 emb chunk、bm25 chunk 打分。
2. 两路径内分别按对齐键聚合，每键保留该路径最优 chunk 与最优分。
3. 路径内 min-max 归一化（单结果路径按生产口径记为 1.0）。
4. 融合分：
   - 两路径均存在的键：`fused = α·norm_emb + β·norm_bm25`；
   - 仅单路径存在的键：`fused = α·norm_emb` 或 `β·norm_bm25`（对应生产 `include_single_path=true`）。
5. 按键融合分降序 → 得到"每键一个代表 chunk"的去重排名，交给现有 `evaluate_query_range_based` / `is_relevant_to_query` 判定。

**公平性约定**：`emb`、`bm25` 单路在本基准中同样按对齐键去重后再评测，与 hybrid 口径一致。现有 baseline benchmark 的去重语义保持不变，不受影响。

### 4.3 与生产融合的口径一致性

- minmax 族：将两路径 top-k 原始结果映射为生产 `SearchResult`，经生产 `expand_multi_entity_results` 展开后**直接调用生产 `fuse_hybrid_results`**（`include_single_path=true`、`min_score=0`），并把融合结果按 chunk id 映射回 `ChunkData` 评测。评测即生产路径，权重 `α/β` 即生产权重。
- rrf 族：本地实现（RRF 为待评估候选，非生产算法），但使用与生产相同的对齐键与多实体展开语义。
- 单路 `emb`/`bm25`：仍按"公平性约定"以主对齐键去重后评测；融合输入使用**原始 top-k**（生产融合在内部完成每键取优），与生产召回口径一致。

## 5. 评测与指标

### 5.1 复用现有指标

- `RangeBasedPerQueryScore` 全套：precision / recall / F1（strong / related / any），沿用 `evaluate_query_range_based` 与加权 recall（Strong:Related = 5:1）。
- top-k 集合沿用 `TOP_K_VALUES = [5, 10, 20, 30, 50]`。
- 按查询类型聚合复用 `generate_aggregate_by_query_type_csv` 的报告格式。

### 5.2 新增指标

| 指标 | 定义 | 用途 |
|------|------|------|
| `fusion_gain` | `(F1_hybrid - max(F1_emb, F1_bm25)) / max(...)`，按查询类型分组 | 量化融合相对最优单路的提升 |
| `weight_sensitivity` | 同一查询下不同权重预设 F1 的标准差 | 判断权重调优是否值得 |
| 首次命中排位 | 每个 judgment range 对应的第一个命中 chunk 在 top-k 中的位次 | 弥补 `storage_retrieval_workflow_background.md` §4.4 的行区间重叠判定缺陷 |
| 同 range 重复占位 | 同一 range 在 top-k 中被多少个不同 chunk 重复覆盖 | 同上 |

### 5.3 意图权重验证（可选阶段）

生产 `Bm25FusionConfig` 支持按查询意图选择权重（semantic 0.8/0.2、keyword 0.2/0.8、hybrid 0.5/0.5、entity 0.7/0.3）。可作为二期对比项：将 `QueryType` 映射到意图，对比"固定权重 minmax-0.5"与"意图自适应权重"的表现。本方案不强制要求，作为可扩展点保留。

## 6. 输出布局

沿用现有 `outputs/benchmark/{fixture}/` 目录与 markdown 报告风格，新增独立子目录 `retrieval_method/`，不与既有 baseline 报告混放：

```
outputs/benchmark/{fixture}/retrieval_method/
├── run_manifest.txt                  # 数据源、对齐键、方法/权重矩阵、运行时间
├── alignment_coverage.md             # 跨路径对齐覆盖统计（§3.3 要求）
├── aggregate_top{k}.md               # 按 (method × baseline) 聚合 P/R/F1
├── aggregate_by_query_type_top{k}.md # 按 (method × query_type) 聚合
├── per_query_top{k}.md               # 逐 query × 逐 method 明细（含首次命中排位、重复占位）
├── fusion_gain_by_query_type_top{k}.md
├── weight_sensitivity.md             # minmax 7 权重敏感性
├── rrf_k_sensitivity.md              # rrf 3 个 k 值对比
└── relevance_top5.md                 # top-5 相关性明细（复用 RelevanceInfo 格式）
```

数据文件仍为 `data/benchmark/{baseline}/{fixture}/bge-m3/bench_data.rkyv`，路径与既有 `BenchmarkPaths` 保持一致，仅输出目录新增子级。

## 7. 实施状态

| 阶段 | 内容 | 状态 |
|------|------|------|
| 1. 融合模块 | `src/retrieval_method/`：生产对齐键（entity→segment→chunk）、minmax 委托生产 `fuse_hybrid_results`、rrf 本地实现、去重排名构造 | ✅ 已完成 |
| 2. 评测编排 | 复用 `evaluate_query_range_based` 完成方法矩阵评测；融合输入用原始 top-k | ✅ 已完成 |
| 3. 报告生成 | 按 §6 布局写报告，输出 alignment_coverage | ✅ 已完成 |
| 4. 三个 fixture 的 example | `benchmark_retrieval_{oncecell,ripgrep,flask}` | ✅ 已完成（数据需重新生成） |
| 5. （可选）意图自适应权重 | QueryType → 意图映射，加入对比 | 待办 |

> 注意：`ChunkData` 结构已变更（新增 `entity_ids`/`segment_id`），旧 rkyv 数据不兼容；需用 `gen_bench_*` 重新生成后才能运行 `benchmark_retrieval_*`。

## 8. 验证方法

1. `emb`、`bm25` 单路结果与现有 `outputs/benchmark/{fixture}/aggregate_metrics_top5.md` 的去重口径差异可解释（对比时应说明去重语义差异）。
2. `minmax-0.5/0.5` 在 full_pipeline 下的 F1 应 ≥ 单路最优（或给出合理的反例分析）。
3. `alignment_coverage.md` 展示各 baseline/fixture 的对齐率（此时为实体/段精确对齐），失真的查询可在 `per_query` 报告中定位。
4. `cargo clippy --all-targets --all-features`、`cargo fmt` 通过。

## 9. 非目标

- 不评测关系扩展、SPSR-Graph 组装、多子查询聚合等执行模式（由 `query-aggregation-benchmark-design.md` 另行规划）。
- 不引入 Qdrant/Tantivy 真实存储服务，评测完全基于离线 rkyv 数据。
- 不修改现有 baseline benchmark 的评测语义（其 chunk 级不去重口径保持不变）。
- min-max 归一化的离群/单结果失真问题交由本基准的评测结果数据驱动决策（如改用 RRF 或加权 RRF），不在本轮直接修改生产算法。
