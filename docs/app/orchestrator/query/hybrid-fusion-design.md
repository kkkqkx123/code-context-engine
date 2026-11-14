# 混合融合（Hybrid Fusion）设计

## 问题描述

索引阶段，每个实体组产生两条独立的文本表示：

- **BM25 路径**：关键词密集文本，按 `max_bm25_terms` 切分
- **向量路径**：语义自然语言文本，按 `max_tokens` 切分

两条路径独立分块，同一逻辑实体的块数与边界可能不同。查询时两条路径并行检索，结果必须合并为单一有序列表。

核心挑战：**在块边界天然不一致的前提下，如何跨路径对齐结果？**

## 设计决策：实体级对齐

系统完全回避块级对齐，改为以**实体**为粒度、用对齐键（alignment key）聚合：

| 内容类型 | 对齐键 | 来源 |
|---|---|---|
| 代码（函数/类/方法） | `entity_id` | AST 解析 → `EntityId` |
| 文档（markdown/txt/log） | `segment_id` | 分组逻辑 → `source_group_id` |

### 为什么是实体级？

BM25 与向量路径之间的块级对齐在一般情况下不可解：BM25 按索引词条数切分，向量按 token 数切分，同一实体可能一侧产生 2 块、另一侧 3 块，二者之间没有确定性映射。

实体级对齐绕开了这个问题：每条路径为同一对齐键贡献其最优块，融合在键粒度上合并分数。键推导的单一事实来源是 `cce_types::alignment_key`，融合层在 `query/retrieval/primitives/fusion/aligner.rs`（crates/app/cce-orchestrator）中封装为 `alignment_key()` 并复用。

## Entity 与 Segment

两个概念并存但角色不同：

- **Entity**：仅代码具备的语义单元（函数/类/方法），有稳定 `EntityId`，为代码内容提供精确对齐。
- **Segment**：拥有块的逻辑分组单元（String 型）。代码的 segment 等于 `EntityGroup.group_id`，文档等于 `source_group_id`，**始终有值**。

融合优先级为 `entity_id > segment_id`，双键设计使所有内容类型走同一套融合逻辑。

## 融合算法

### 第 1 步：路径内分数归一化

每条路径在对齐键粒度上独立做 min-max 归一化到 [0.0, 1.0]（`normalizer.rs` 的 `normalize_path_scores` / `normalize_by_key`）。在键粒度而非块粒度归一化，可避免长实体（多碎片）拉伸 min/max 区间、压缩单碎片实体的归一化分数。

**设计理由**：min-max 简单且保持序关系；路径权重提供用户可调的贡献控制。

**已知局限**：单结果路径会被归一化为 1.0；跨路径的绝对分数校准（向量余弦 vs BM25 TF-IDF）不由 min-max 解决，需靠权重调优。

**替代方案**：Z-score（需历史统计）、学习式校准（需标注数据）。基于排名的融合已经落地为运行时可选算法 `Rrf{k}` 与 `BordaCount`（见下文）。

### 第 2 步：每键保留最优块

对每个对齐键，每条路径只保留原始分最高的一个条目（`vector_by_key` / `bm25_by_key`）。融合契约要求输入已按单实体展开（每条结果至多一个 `entity_ids`）；若收到未展开的多实体结果，融合会发出 warning 并防御性地在内部重新展开（merger.rs 开头的展开检查）。

### 第 3 步：加权组合

双路径键的融合分数为**带归一除法的加权和**（`fuse_weighted_minmax`，merger.rs 约 343 行附近）：

```
fused = (alpha * norm_v + beta * norm_bm25) / (alpha + beta)
```

其中 `alpha = vector_weight`，`beta = bm25_weight`。归一除法使权重对不必和为 1 时分数仍落在 [0, 1] 量级（配置校验仍要求权重对和为 1，见下文配置节）。单路径键在 `include_single_path` 下只贡献所在路径的加权项。输出块的路径分数字段写回归一化后的值，与融合分保持可比；基准块取加权贡献较大一侧的块，避免跨块身份混用。

### 第 4 步：键集选择、去重与过滤

- `require_strict_intersection`（默认 false）：开启后只保留双路径都出现的键，优先级高于 `include_single_path`（`union_keys`）。`include_single_path` 默认 true，且**非对称**：仅向量路径键始终保留，该开关只控制 BM25-only 键是否加入。
- `dedup_by_chunk`（默认 true）：实体级对齐可能让同一物理块按其包含的多个实体各出现一次；开启后按 chunk id 折叠为最高分条目（`dedup_by_chunk_id`）。
- `min_score`：融合阈值，仅作 hygiene 用途；业务阈值在最终结果过滤（`result.min_score`）。融合 `min_score` 超过 0.05 会触发 warning，提示业务阈值应放在 `result.min_score`（merger.rs 约 128 行）。
- 最终按融合分降序排序，含确定性 tie-break（`sort_fused_by_score`）。

### 单路径快速路径

当一侧召回为空时不会绕过融合：空召回被视为空结果集，存活路径经 `fuse_single_path` 快速路径处理（merger.rs 约 603 行），跳过并集键集与空侧聚合，但加权与 `min_score` 阈值语义与双路径完全一致。

## 融合算法选择

算法由 `[search.fusion].algorithm`（`cce-config` `src/modules/search.rs` 的 `RecallFusionAlgorithm`）选择，经工厂函数 `create_fusion_algorithm`（merger.rs 约 29 行）实例化为 `FusionAlgorithmImpl`：

| 算法 | TOML 形式 | 说明 |
|---|---|---|
| `WeightedMinMax`（默认） | `"weighted_min_max"` | 上述加权 min-max 融合 |
| `Rrf{k}` | `{ rrf = { k = 60 } }` | `score = w_v/(k+rank_v) + w_b/(k+rank_b)`，只用排名，对分数分布偏斜鲁棒；`k` 需大于 0（配置校验） |
| `BordaCount` | `"borda_count"` | 权重感知 Borda：每路径按 `(key_count - rank + 1) / key_count` 给点，融合分为加权点积，与 WeightedMinMax 同量纲；线性衰减优于 RRF 的双曲衰减 |

所有算法共享同一管线（对齐键、覆盖率统计、去重、排序）；查询意图自适应只切换权重对，从不隐式切换算法。

## 多实体展开

单个块可能包含多个实体（如含方法的类）。融合前这类块会被展开，使每个实体获得同分的独立条目，从而实现实体级对齐。实现已迁移至 `query/retrieval/primitives/fusion/aligner.rs` 的 `expand_multi_entity_results`；`searcher_core.rs` 保留 `pub use` re-export 以维持历史导入路径。Searcher 在调用融合前对两条路径的结果执行展开。

## 聚合检索去重

`search_aggregated()` 运行多个子查询并合并结果，去重使用与融合相同的对齐键：`entity_id` → `segment_id` → chunk `id`。

## 融合后关系标注

`query::annotation` 作为在线后处理阶段，在排序与阈值过滤之后对 top-N 命中执行关系标注，不改变排序结果。开启 `search.annotation.enable_annotation` 后：

1. **语义单元抽取**：抽取带正文的结果为保持结构的语义单元（含行范围）。
2. **单元去重**：按文件路径、行范围与正文的稳定内容哈希去重。
3. **结构拼接**：主单元与调用方提供的关系单元拼接为连贯代码块；仅同文件未标记的主片段参与合并，空隙以省略说明标记。单个超限正文降级为路径+行范围引用；缺失文件降级为引用；其余单元按分数选取（主单元固定首位）后按文件顺序渲染。召回数量由 top-N 与分数阈值控制，无批量 token 上限。

整文件替换已不存在：原高覆盖场景渲染为文件路径引用。

## 召回后扩展（Post-recall Expansion）

在线 `Searcher` 侧的图遍历位于 `query/expansion.rs` 与 `query/expansion_scoring.rs`（crates/app/cce-orchestrator）：

- **NeighbourResolver**：由调用方（如 MCP/API 的 opt-in 请求）传入时启用扩展；通过 `RelationSearcher` 在快照关系索引上解析 callee/caller 邻居。种子经快照行范围查找解析（关系索引 ID 空间），而非命中结果的检索实体 ID（SQLite 行空间），因此由容器表示的方法组仍能浮出成员边。call 类边渲染为**调用点窗口**（调用行附近的几行上下文），无可用调用点的边才回退到完整实体体。
- **ExpansionShaping**：可配置的渲染上限（`max_seeds_per_hit`、`callsite_context_lines`、`max_expansion_unit_tokens`），从 `RelationAnnotationConfig` 派生。
- **ExpansionPathFilter**：扩展单元沿用与主检索一致的目录前缀、内容类型排除与类别 include/exclude 过滤，share 同一实现。
- **window_excerpt**：超出 `max_expansion_unit_tokens` 预算的正文渲染为窗口摘录（头部 24 行 + 尾部 4 行 + 最多 8 条调用形中间行），每处裁剪都带省略标记。
- **ExpansionScoring**（`expansion_scoring.rs`）：对邻居按 `expansion_score` 排序而非保持遍历发现序，包含查询词重合因子（`expansion_query_scoring`）、caller fan-in 因子（`expansion_fanin_scoring`）、seed 重叠权重等因子；总开关 `expansion_scoring` 关闭时恢复历史全零分（先到先得占用预算）。

这些行为由 `RelationAnnotationConfig` 中的 `expansion_*` 系列字段与 `batch_shared_dedup` 控制（cce-config `src/modules/search.rs`）：

- `expansion_enabled`（默认 false）：是否在标注结果上附加预解析邻居。
- `forward_max_expanded_units` / `backward_max_expanded_units`（默认各 4）：每结果前向（callee）/后向（caller）扩展单元数上限；`expansion_include_callers` 控制是否含 caller 侧。
- `expansion_domains`（默认 `["call"]`）：解析的关系域；`allow_structural_edges`（默认 false）控制是否附加继承/实现/import 等结构边。
- `filter_stdlib` / `filter_external`（默认 true）：丢弃标准库与无工作区源码的外部目标（调用方已先过滤，属于纵深防御）。
- `omit_self_evident_edges`（默认 true）：丢弃主正文中字面可见的直接/静态/构造调用边。
- `max_seeds_per_hit`（默认 4）、`callsite_context_lines`（默认 1）、`max_expansion_unit_tokens`（默认 512）：种子数、调用点上下文行数与单元 token 预算。

**行为修正说明**：开启 `batch_shared_dedup`（默认 true）后，一次批量标注中所有结果共享去重状态（已见实体 ID 与内容哈希），同一实体不会作为不同命中的扩展被重复拼接；扩展单元按 `ExpansionScoring` 竞争每结果的扩展预算，而非历史上按发现序无条件填满 top-N——这修正了旧的"每结果固定 top-N 邻居"行为，代价是排序靠后的命中可能拿到更少的扩展单元。

## 配置

```toml
[search.fusion]
vector_weight = 0.5          # 静态默认权重；启用意图自适应时按查询意图覆盖
bm25_weight = 0.5
algorithm = "weighted_min_max"   # 或 { rrf = { k = 60 } } / "borda_count"
enable_intent_based_weights = true
include_single_path = true
require_strict_intersection = false
min_score = 0.0
dedup_by_chunk = true

[search.fusion.intent_weights]   # 按查询意图的权重对，均要求和为 1
semantic = { vector_weight = 0.8, bm25_weight = 0.2 }
keyword  = { vector_weight = 0.2, bm25_weight = 0.8 }
hybrid   = { vector_weight = 0.5, bm25_weight = 0.5 }
entity   = { vector_weight = 0.7, bm25_weight = 0.3 }

[search.annotation]
enable_annotation = false
max_annotated_length = 8000
include_file_markers = true
dedup_strategy = "by_entity_id"
annotation_top_n = 3
enable_segment_merge = true
segment_merge_gap = 2
omit_primary_body = false
expansion_enabled = false
forward_max_expanded_units = 4
backward_max_expanded_units = 4
expansion_include_callers = true
expansion_domains = ["call"]
allow_structural_edges = false
filter_stdlib = true
filter_external = true
omit_self_evident_edges = true
batch_shared_dedup = true
expansion_scoring = true
expansion_query_scoring = true
expansion_fanin_scoring = true
max_seeds_per_hit = 4
callsite_context_lines = 1
max_expansion_unit_tokens = 512
```

说明：

- `[sources.hybrid]` 下的 `vector_weight` / `bm25_weight` 已被 `cce-config` `src/modules/search.rs` 中 `SearchConfig.fusion`（`HybridFusionConfig`，crates/core/cce-config）取代。
- 权重按查询意图动态选择：`enable_intent_based_weights` 默认 true，`ExecutionStrategy::from_options`（`query/types/execution_strategy.rs`）依据显式 `query_intent` 或 `intent_detector::detect_intent` 从 `intent_weights` 的 semantic/keyword/hybrid/entity 四组中选取权重对。
- 配置校验要求 `vector_weight + bm25_weight = 1.0`（静态对与四组意图对均校验），`Rrf.k > 0`，各权重在 [0, 1] 内。

## 已知局限

1. **每实体每路径仅一个块**：只有每个对齐键在每条路径上的最优块参与融合；实体的多块内容若覆盖不同侧面，仅有最高分块入选。
2. **无跨路径块边界对齐**：系统有意不建立 BM25 块 N 到向量块 M 的映射，细粒度位置对应关系丢失。
3. **Segment 聚合仅在标注开启时生效**：`SegmentAggregator` 只在关系标注开启时运行；标准混合结果可能包含同文件的碎片化 segment。
