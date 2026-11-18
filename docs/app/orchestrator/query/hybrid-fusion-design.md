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

**离群收缩**：键数 ≥ 8 时先把原始分截断到 10/90 分位再归一化（`ROBUST_MIN_KEYS` / `ROBUST_TRIM_QUANTILE`），避免单个异常高分占满区间、把主体压到低端；键数少于 8 时不截断，小样本上的分位估计没有意义。截断只作用于键粒度归一化，通用 `minmax_normalize`（boost 层复用）语义不变。

**退化输入**：单键或全等分路径没有可用的极差，仍映射为 1.0（"该路径内并列最优"），但归一化会返回 `degenerate` 标记，融合层把它写进 trace，使"满分"从隐式变成可观测。把退化输入改映射为 0.5 会让阈值行为依赖"该路径返回了几个命中"，因此不采用。

**已知局限**：跨路径的绝对分数校准（向量余弦 vs BM25 TF-IDF）不由 min-max 解决，需靠权重调优；min-max 会把路径内最差键打到 0，双路共识的尾键因此可能被阈值过滤，对这类分布优先切 `borda_count`。

**替代方案**：Z-score（需历史统计）、学习式校准（需标注数据）。基于排名的融合已经落地为运行时可选算法 `Rrf{k}` 与 `BordaCount`（见下文）。

### 第 2 步：每键保留最优块

对每个对齐键，每条路径只保留原始分最高的一个条目（`vector_by_key` / `bm25_by_key`）。融合契约要求输入已按单实体展开（每条结果至多一个 `entity_ids`）；若收到未展开的多实体结果，融合会发出 warning 并防御性地在内部重新展开（merger.rs 开头的展开检查）。

### 第 3 步：加权组合

双路径键的融合分数为**带归一除法的加权和**（`fuse_weighted_minmax`，merger.rs 约 343 行附近）：

```
fused = (alpha * norm_v + beta * norm_bm25) / (alpha + beta)
```

其中 `alpha = vector_weight`，`beta = bm25_weight`。归一除法使权重对不必和为 1 时分数仍落在 [0, 1] 量级（配置校验仍要求权重对和为 1，见下文配置节）。

融合分的语义是**证据强度**：该键占可用加权证据的比例。因此单路径键只贡献所在路径的加权项，上限为自身权重占比（等权 0.5），永远不会拿到满分；这与"另一条路径整体为空"时的取值一致（同一个 `fused_score` 助手，`merger.rs` 的 `fused_score()`），阈值不会因无关路况漂移。

输出块的路径分数字段写回归一化后的值，与融合分保持可比；基准块取加权贡献较大一侧的块，避免跨块身份混用。落败方不再被丢弃：其块标识、归一化分、原始分与行号区间记录在 `SearchResult::secondary_evidence`（`path` 标注来自哪条路径），正文仍只保留胜出方，重排与展示语义不变。

### 第 4 步：键集选择、去重与过滤

- `include_vector_only_keys` / `include_bm25_only_keys`（均默认 true）：分别控制向量独有键与 BM25 独有键是否加入并集（`union_keys`）。双路都命中的键不受两者影响，因此两者同时关闭等价于严格交集。历史上只有一个 `include_single_path`，且只门控 BM25 侧；文档声称"向量独有键始终保留"，但双路径主循环其实对两侧一并丢弃，只有空路径快速路径保留向量侧——这是文档与实现不一致，现已拆成显式对称开关。
- `require_strict_intersection`（默认 false）：开启后只保留双路径都出现的键，优先级高于两个单路开关。
- `dedup_by_chunk`（默认 true）：实体级对齐可能让同一物理块按其包含的多个实体各出现一次；开启后按 chunk id 归组为一条（`group_by_chunk`）。代表条目是最佳实体，`entity_ids` 仍保持单实体（下游去重键与回填命名都依赖这一契约），同块其余实体及其融合分写入 `entity_scores`，不再丢失。
- `min_score`：融合阈值，仅作 hygiene 用途；业务阈值在最终结果过滤（`result.min_score`）。融合 `min_score` 超过 0.05 会触发 warning，提示业务阈值应放在 `result.min_score`（merger.rs 约 128 行）。
- 最终按融合分降序排序，含确定性 tie-break（`sort_fused_by_score`）。

### 单路径快速路径

当一侧召回为空时不会绕过融合：空召回被视为空结果集，存活路径经 `fuse_single_path` 快速路径处理（merger.rs 的 `fuse_single_path`），跳过并集键集与空侧聚合，但计分公式、权重占比与 `min_score` 语义与双路径完全一致——同一键在"另一路为空"与"另一路返回了无关命中"两种情形下得分相同（单测 `test_single_path_key_scores_identically_across_other_path_state`）。

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

展开之后若开启 `dedup_by_chunk`，同一块的多个实体条目归组为一条，但实体信息不丢：代表实体留在 `entity_ids`，全部实体（含代表）及其融合分写入 `entity_scores`。

## 对齐键补全

融合只使用索引携带的字段推导对齐键，而回填补全实体关联发生在融合之后。位于两者之间的补全步骤只处理**路径内键**：既没有实体、`segment_id` 又为空或等于块 id 的条目（块 id 带路径后缀，跨路永不相等）。这类条目按块 id 批量读取 `ChunkRecord`，回填 `entity_ids` 与有意义的 `segment_id`。

约束：每条路径最多 200 个候选（`MAX_KEY_COMPLETION_CANDIDATES`）；只改标识，不动正文、分数与行号；无元数据存储、查询失败或记录缺字段时整体降级为原行为并记录 trace。补全只会把"永不匹配"变成"可能匹配"，不会把已可对齐的键改坏。

## 跨路径对齐指标

每次 hybrid 融合通过 `FusionAlignmentStats` 记录三项比率，分别进入 `search_hybrid_alignment_match_ratio`、`search_hybrid_alignment_single_path_ratio` 与 `search_hybrid_alignment_unalignable_ratio` 直方图：

- 匹配率 = 双路共有键数 / 向量路键数，延续向量主路的口径；
- 单路键占比 = 仅一路返回的键数 / 去重后的键总数，路径内兜底键不计入分子与分母，避免与下一项重复计数；
- 不可对齐占比 = 路径内兜底条目数 / 所有候选条目数，覆盖“完全没有键”和“键退化为块 id”两种情形。

离线侧复用同一统计结构：`alignment_report` 输出逐查询与跨查询汇总的三项比率，`retrieval_method` 基准的 `alignment_coverage.md` 输出语料级键计数与单路键、不可对齐条目占比。两者口径一致，分母分别为逐查询观测与全语料，不可直接混读。

## 聚合检索去重

`search_aggregated()` 运行多个子查询并合并结果，去重使用与融合相同的对齐键：`entity_id` → `segment_id` → chunk `id`。

同键多次命中的分数组合由 `[search.aggregation] score_strategy` 决定：

- `noisy_or`（默认）：`score = 1 - Π(1 - s_i)`，把每个子查询的命中当作独立证据累加，同键多次命中必然抬分，结果仍在 [0, 1]，全局 `result.min_score` 口径不变。子查询权重 > 1 会把贡献截断到 1 以下（饱和而非越界）。
- `max`：保留历史行为，只取最强的一次贡献，排序由单个大权重子查询主导。

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
include_vector_only_keys = true
include_bm25_only_keys = true
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
- 配置校验要求 `vector_weight + bm25_weight = 1.0`（静态对与四组意图对均校验），`Rrf.k > 0`，各权重在 [0, 1] 内。`require_strict_intersection` 与"关闭任一单路开关"同时出现会报配置错误（严格交集已覆盖，属于死配置）。
- 融合 `min_score` 与 `result.min_score` 同为 [0, 1] 证据强度口径。推荐区间：`weighted_min_max` 0.25–0.5、`borda_count` 0.3–0.6、`rrf` 0.3–0.7；单路键上限为自身权重占比（等权 0.5），阈值高于该占比时单路退化查询会被整体过滤。

## 已知局限

1. **每实体每路径仅一个块**：只有每个对齐键在每条路径上的最优块参与融合；实体的多块内容若覆盖不同侧面，仅有最高分块入选。落败方的块标识与分数记录在 `secondary_evidence`，但正文与行号只在胜出方上物化。
2. **无跨路径块边界对齐**：系统有意不建立 BM25 块 N 到向量块 M 的映射，细粒度位置对应关系丢失。
3. **Segment 聚合仅在标注开启时生效**：`SegmentAggregator` 只在关系标注开启时运行；标准混合结果可能包含同文件的碎片化 segment。
4. **块标识兜底无法对齐**：`segment_id` 为空时索引写入块 id 作为分段键，该键带路径后缀，跨路永不匹配；插件或异常分块因此恒走单路并集。此类占比由 `search_hybrid_alignment_unalignable_ratio` 上报，占比高说明索引缺字段而非正常并集。
5. **min-max 的最差键为 0**：路径内最差键归一化为 0，双路共识的尾键可能被任何阈值过滤；需要"幅度无关的共识排序"时切换 `borda_count`。
