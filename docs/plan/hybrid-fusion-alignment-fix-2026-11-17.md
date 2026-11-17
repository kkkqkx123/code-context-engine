# Hybrid 融合对齐：设计评估与落地改造方案

日期：2026-11-17
输入：`docs/issue/hybrid-fusion-alignment-2026-11-17.md`
状态：已完成评估，改造已落地（见第 10 节落地清单）
口径：只讨论对齐键推导、分数合成、去重与聚合语义；向量相似度与 BM25 词项权重本身不在范围内

## 0. 结论摘要

原文档提出的 9 条问题，**7 条事实成立、1 条归因不准确、1 条表述过宽**：

| 条目 | 判定 | 说明 |
| --- | --- | --- |
| 2.1 融合只留胜出块 | 成立 | 双路同键时非胜出方的分块被整体丢弃，仅贡献分数 |
| 2.2 孤路快捷路径计分不一致 | 成立，但**仅限 weighted_min_max** | RRF / Borda 无快捷路径，天然一致 |
| 2.3 单键退化为满分 | 成立，但**是 2.2 的同一处成因** | `fuse_single_path` 的 `fused_score = norm` 一行同时导致 2.2 与 2.3 |
| 2.4 归一化分布敏感 | 成立 | 但"退化映射"与"离群压缩"是两个独立子问题，需分开处理 |
| 2.5 展开与按块去重互斥 | 成立 | 且 `dedup_by_chunk` 默认开启，默认配置下多实体块的展开收益被立即抹平 |
| 2.6 融合早于回填 | 成立 | 回填补全 `entity_ids` 的能力在融合之后才生效 |
| 2.7 块标识兜底恒不对齐 | 成立 | 缺"不可对齐占比"上报，只有匹配率 |
| 2.8 聚合取最大分 | 成立 | 同键多子查询的累加证据被丢弃 |
| 2.9 单路保留隐式不对称 | 成立，且比原描述更严重 | 文档声称"向量独有键始终保留"，双路主循环其实两侧一并丢弃，只有空路径快速路径保留向量侧 |

设计方向（实体级对齐、分段兜底、展开提召回、回填后置）**合理，无需推翻**。
但原文档的三批方案在若干处把"可观测性缺口"与"计分缺陷"混为一谈，
且第 3.3 节的"退化输入不再映射满分"若按字面实现，会把阈值语义从
"证据强度"改成"命中数量"，比现状更难解释。本方案的取舍见第 3 节。

## 1. 事实核验

代码基线：`8ef6026e`（`doc temp`），融合模块自 `b029b62e` 后未再改动。

1. **对齐键优先级**：`crates/core/cce-types/src/types/alignment.rs:24-38`，
   实体 `e:` → 分段 `s:` → 块标识 `c:`。多实体只取首个（同文件 `tests::multi_entity_list_uses_first`）。
2. **双路同键只留一方**：`fusion/merger.rs:302-317`，按加权贡献择大者作为输出块，
   另一方的 id / 行号 / 正文全部丢弃，仅把归一化分写入 `bm25_score` / `vector_score`。
3. **孤路快捷路径**：`fusion/merger.rs:268-293` 仅在**整条路径为空**时进入
   `fuse_single_path`；其中 `fusion/merger.rs:635` 直接令 `fused_score = norm`。
   双路并存时的单路键走主循环 `fusion/merger.rs:319-337`，最终
   `fusion/merger.rs:343-344` 再除以 `alpha + beta`。同一键因此差一个权重因子。
4. **RRF / Borda 无快捷路径**：`fusion/merger.rs:426-483`、`502-562` 无空路径分支，
   单路键与双路键共用同一公式，故 2.2 只命中 weighted_min_max。
5. **退化归一化映射满分**：`fusion/normalizer.rs:27-30`，`range <= EPSILON` 时返回全 1.0。
6. **三种算法顶尖均为满分**：RRF 除以 `(alpha+beta)/(k+1)`（`merger.rs:469-470`），
   Borda 除以 `alpha+beta`（`merger.rs:549`），weighted_min_max 除以 `alpha+beta`（`merger.rs:344`）。
   三者名义区间一致，但底部分布差异极大：min-max 把路径最差键映射为 0，名次型不会。
7. **按块去重丢实体**：`fusion/merger.rs:650-666` 按 `result.id` 取最高分条目，
   `HybridFusionConfig::dedup_by_chunk` 默认 `true`（`cce-config/src/modules/search.rs:273`），
   而展开发生在融合前（`searcher_core.rs:334-335`），默认配置下展开后立即塌回一条。
8. **融合早于回填**：`searcher_core.rs:359-360` 融合，`374-380` 才回填；
   回填补全实体关联在 `entity_mapper.rs:225-233`（`entity_ids` 为空时用 SQLite 值填充）。
9. **块标识恒不对齐**：`index/storage_coordinator/mapping.rs:77-87`，
   `segment_id` 为空时回退为块 id；块 id 形如 `group_1_emb_0` / `group_1_bm25_0`，
   跨路永不相等，于是键退化为 `s:{块 id}`（注意不是 `c:`：`segment_id` 非空，
   只是内容等于块 id），两路各自成键。`c:` 兜底只在 `segment_id` 为 `None`/空串时出现。
10. **聚合取最大分**：`query/coordinator.rs:656-676` 按统一去重键取最高加权分；
    之后统一阈值与截断（`coordinator.rs:679-688`）。
11. **单路保留不对称**：并集构造（`merger.rs:173-184`）里向量键无条件加入、
    `include_single_path` 只控制 BM25 键，但双路主循环（`merger.rs:319-337`）的两个
    单路分支都以该开关为门、开关关闭时两侧一起丢弃，只有快捷路径（`merger.rs:614`）
    保留向量侧。文档描述与实现不一致，见 2.9。
12. **覆盖率指标只记匹配率**：`cce-metrics/src/domain/search.rs:61-68`，
    分母固定为向量键数，`bm25_keys` 参数未被使用。

## 2. 原设计评估

### 2.1 成立且优先级最高

双路同键只留一方，让"混合增益"在结果层面不可见：用户看到的分块可能来自
向量摘要，而真正的关键词命中在另一片子块。原方案"融合结果携带双路代表引用"
方向正确，但需明确**只携带引用、不携带正文**，否则回填成本与重排语义都会抖动。

### 2.2 / 2.3 是同一处成因而非两条问题

`fuse_single_path` 的整段语义被注释描述为"与双路完全一致"（`merger.rs:588-602`），
实际只有这一行不一致。修一处即可同时消除 2.2 与 2.3。
原文档要求"退化输入不再映射满分"，若真把 `minmax_normalize` 的全等分映射从 1.0 改成 0.5，
结果是：一条只返回 5 个等分命中的路径，其每个键的融合分都只有 0.5×权重，
而多命中且分数有梯度时首键仍是满分——阈值行为反而开始依赖"这条路径返回了几个命中"。
**这不是改进，是把确定性缺陷换成统计性缺陷。**

### 2.4 成立，但应拆成两个子问题

- 离群压缩：可修，用分位截断（第 3 节 D4）。
- 阈值跨算法不可移植：不是代码缺陷，是**文档缺失**。三种算法的顶部都归一到 1.0，
  底部行为不同（min-max 会把尾键打到 0，RRF/Borda 不会）。给出各算法推荐阈值区间即可，
  不应为了"可移植"去改公式。

### 2.5 成立，但"合并实体列表"的解法会破坏下游不变量

融合输出的"单条目单实体"是后处理去重键（`unified_dedup_key` → `alignment_key` 取首实体）
与回填命名（`entity_mapper.rs:208-223` 按命中实体选 kind / name）的前提。
若把多实体块合并成一条并写入多个 `entity_ids`，这两处都会退化为"取首个实体"的隐式行为。
原文档"保留实体列表与各自分项"的目标可以用**不破坏单实体不变量**的方式达成（D5）。

### 2.6 成立，但原方案"把回填拆成轻量键补全"成本被低估

SQLite 每次批量取记录本身不贵，贵在**候选规模**：两路各 50–200 条，
键补全若不加限制会把回填从"融合后幸存集"提前到"融合前全集"。
可行，但必须：只补**缺键**的条目、设上限、失败即降级。

### 2.7 成立；缺的是上报口径而不是策略

块标识兜底是刻意设计（跨路不可对齐即不做伪对齐），不需要改行为。
需要的是把"无键占比"变成指标，让下游能区分"正常并集"与"索引缺字段"。

### 2.8 成立，但"加权求和"会破坏阈值口径

子查询分已在 `[0,1]`，加权求和上限是子查询数，全局阈值随之失去意义。
原文档给的备选"名次式累加"又丢掉了分数幅度。需要一种**保持在 [0,1] 且单调**的累加式（D6）。

### 2.9 成立，但根因是文档与实现不一致，不是"刻意的非对称设计"

`HybridFusionConfig::include_single_path` 的文档（`cce-config/src/modules/search.rs`
改造前的字段注释）写的是"向量独有键始终保留，该开关只控制 BM25-only 键"。
实测双路主循环并非如此：`merger.rs:319-337` 的两个单路分支都以
`if config.include_single_path` 为门，开关关闭时两侧单路键一起丢弃
（集成测试 `test_fuse_hybrid_results_exclude_single_path` 断言的正是这个行为）。
真正保留向量侧的只有空路径快速路径（`merger.rs:614`）。

因此 2.9 不是"把隐式的不对称写进配置"就能收尾，而是要**先确定语义再落地**：
本方案选择对称语义（两路各有开关、默认都开、双路都命中的键永不受门控），
因为按路径分开关的语义可解释、可验证，且与"严格交集"这一显式意图不冲突。

## 3. 设计决策

### D1 统一单路计分：改为"证据强度"口径，取消快捷路径满分

融合分统一定义为 `score = (w_v·n_v + w_b·n_b) / (w_v + w_b)`，缺失路贡献 0，
**快捷路径与主循环共用同一公式**。等权下：

| 场景 | 现状 | 改造后 |
| --- | --- | --- |
| 双路同键，两路均为该路首键 | 1.0 | 1.0 |
| 双路同键，仅向量命中（另一路非空但无此键） | 0.5 | 0.5 |
| 另一路整体为空，单路首键 | **1.0** | **0.5** |
| 另一路整体为空，单路第 2 键（共 5 键） | 0.75 | 0.375 |

语义变为"该键占可用加权证据的比例"，`min_score` 因此可跨查询、跨路况比较。
代价：单路场景下最大分从 1.0 降到 `max(w_v, w_b)`，`result.min_score` 需按下表重标（第 5 节）。

### D2 退化归一化保持 1.0，但显式标记低置信

`minmax_normalize` 的全等分/单键输入仍映射 1.0（"该路径内并列最优"），
理由见 2.2/2.3 评估。新增**返回是否退化**的入口，融合层把退化路径计数进对齐统计与 trace，
使"满分"从隐式变成可观测。boost 层继续复用同一函数，语义不变。

### D3 对齐键补全前移，但只补缺键、只补标识

在展开与融合之间插入一次**轻量键补全**：仅对 `entity_ids` 为空（键退化为 `s:`/`c:`）的候选，
按块 id 批量取 `ChunkRecord`，回填 `entity_ids` 与缺失的 `segment_id`。
上限 200 条/路；无元数据存储、查询失败或未命中时整体降级为现状并计数。
正文回填仍然只在融合后做一次。

### D4 离群收缩：分位截断，只作用于键粒度归一化

键数 ≥ 8 时，先把原始分截断到 `[P10, P90]` 再 min-max；键数少时不截断（分位在小样本上无意义）。
只改 `normalize_by_key`，不动通用 `minmax_normalize`，避免 boost 归一化被牵连。

### D5 双证据保留：只带引用；多实体按"分项"而非"并表"保留

- 融合输出新增 `secondary_evidence`：另一路代表块的 `chunk_id`、归一化分、原始分、行号区间。
  不带正文，重排仍只消费主文本，展示默认不变。
- 按块去重改为按块归组：代表条目的 `entity_ids` 保持**单实体**（维持下游不变量），
  同块其它实体及其融合分进 `entity_scores`（含代表实体，首位即代表）。
  "次优实体丢失"由此变成"降级为分项"，不再丢信息，也不产生新的键空间歧义。

### D6 聚合累加：noisy-OR，可回退 max

`score = 1 - Π(1 - s_i)`，`s_i` 为子查询加权分（截断到 `[0, 0.999]`）。
性质：单命中等于原分；多命中单调不减；恒在 `[0,1]`，全局阈值口径不变。
配置项 `aggregation.score_strategy = "noisy_or" | "max"`，默认 `noisy_or`，
需要旧行为可切 `max`。权重 > 1 会饱和，配置文档中标注推荐 `weight <= 1`。

### D7 单路保留显式化且有确定语义

`include_single_path` 拆为 `include_vector_only_keys` 与 `include_bm25_only_keys`（均默认 true）。
语义三条，写进配置注释与设计文档：

1. 双路都命中的键**永不**受两个开关影响（开关只决定"独有键"是否加入并集）。
2. 因此两者同时关闭等价于严格交集；`require_strict_intersection` 仍是显式表达方式，
   与"关闭任一开关"组合会报配置错误（死配置）。
3. 空路径快速路径按存活路径自己的开关判断，与并集键集规则一致。

### D8 不可对齐占比上报（比原方案"无键占比"更准确）

"完全没有键"在实际数据里几乎不存在：索引总会给 `segment_id` 兜底，
兜底值就是块 id。所以真正需要上报的是**不可对齐**条目，定义两条：

- 三条键来源全空（`c:` 都推不出来）；或
- 键退化为 `s:{chunk_id}`，即 `segment_id` 为空或等于块 id（块 id 带路径后缀，跨路永不相等）。

`FusionAlignmentStats` 增加 `vector_unalignable` / `bm25_unalignable` 与之比方法，
指标新增 `search_hybrid_alignment_unalignable_ratio`（分母为全部候选条目），
占比 > 0.5 时 warn 一次。配置与设计文档同步说明"块标识兜底属正常并集，
占比持续升高才说明索引缺字段"。

### D9 新维度必须先落成离线基线，否则无法对照

本次改造引入了三个"线上可见、离线不可见"的维度：单路开关、聚合计分、融合分尺度。
只看 F1 表无法判断它们是否按预期生效（排序不变时 F1 完全不变），因此基准侧补齐三张基线：

1. **单路开关**：`minmax-0.5-{vec,bm25,strict}` 与 `rrf-60-strict`，同权重只差开关，
   把"保留这一侧单路键值多少 F1"变成可直接读的行。
2. **聚合计分**：`agg-max` / `agg-noisy-or`，键与计分公式直接复用生产函数（不复制实现），
   附多来源重叠率，避免"没有差异"与"合并失效"无法区分。
3. **融合分尺度**：`fusion_score_baseline.md` 记录每条方法的路径覆盖拆分、达到"理论
   上限"的条目占比、均值/最大值。单路键的上限是自身权重占比，因此"单路键拿到 1.0"
   这一回退会在该表立刻显形。
4. **对齐覆盖率**：`alignment_coverage.md` 增加每路不可对齐块数与占比，口径与线上
   `search_hybrid_alignment_unalignable_ratio` 一致。

## 4. 修改方案（按文件）

### 4.1 `crates/core/cce-config/src/modules/search.rs`

- `HybridFusionConfig`：删除 `include_single_path`，新增 `include_vector_only_keys`、
  `include_bm25_only_keys`（均默认 true）；`dedup_by_chunk` 注释改为"按块归组"。
- 新增 `AggregationScoreStrategy`（`Max` / `NoisyOr`）与 `AggregationConfig`，
  挂到 `SearchModuleConfig` 的 `aggregation` 字段，并在 `Validate` 里串联。
- 校验：两开关同时关闭 + `require_strict_intersection` 判为死配置并报错。

### 4.2 `crates/app/cce-orchestrator/src/query/types/search_result.rs`

- 新增 `RecallPath`、`SecondaryEvidence`、`EntityScore` 三个类型与两个字段
  `secondary_evidence` / `entity_scores`，默认 `None` / 空 vec。

### 4.3 `fusion/normalizer.rs`

- `normalize_by_key` 走新的"截断 + min-max"并返回退化标记；新增 `NormalizedPath` 结构。
- `minmax_normalize` 保持公开签名与语义（boost 层依赖）。

### 4.4 `fusion/aligner.rs`

- `FusionAlignmentStats` 增加两个不可对齐计数与 `unalignable_ratio()` / `match_ratio()`；
  `compute_alignment_coverage` 同步统计。
- 新增 `is_alignable`（块 id 兜底判定）与 `complete_keys_from_records`（纯逻辑键补全）。
- `best_per_key` 保持不变（不可对齐键仍参与各自路径的归一化与排序）。

### 4.5 `fusion/merger.rs`

- 统一 `fuse_score` 助手，主循环与快捷路径共用；删除 `normalize` 参数。
- 双路同键：写入 `secondary_evidence`（败方引用），并按块归组写 `entity_scores`。
- `dedup_by_chunk_id` → `group_by_chunk`：归组而非丢弃，代表条目为最高分者。
- `union_keys` 改读两个新开关。

### 4.6 `query/searcher/searcher_core.rs`

- 新增 `complete_alignment_keys`（D3），在展开前调用；统计补全/未命中/降级条数并 trace。
  纯逻辑拆到 `fusion/aligner.rs` 的 `complete_keys_from_records`，便于脱离存储单测。
- 融合后把不可对齐计数交给指标。

### 4.7 `query/coordinator.rs`

- 去重分支按 `aggregation.score_strategy` 走 max 或 noisy-OR；代表条目取单次贡献最大者。

### 4.8 `crates/core/cce-metrics/src/domain/search.rs`

- `record_hybrid_alignment` 增加不可对齐计数参数与 `search_hybrid_alignment_unalignable_ratio` 直方图。

### 4.9 测试

- 融合单测（`fusion.rs`）：单路一致性（同一键在另一路空/非空时等分）、双证据引用、
  按块归组保留分项、分位截断、退化标记、双开关对称矩阵、不可对齐统计。
- 归一化单测（`normalizer.rs`）：退化标记、离群截断、小样本不截断。
- 键补全单测（`aligner.rs`）：兜底键判定、实体/分段回填、记录缺失降级、已可对齐不被动。
- 聚合单测（`coordinator.rs`）：noisy-OR 单调与有界、权重大于 1 饱和、max 保留、来源合并。
- 基准：`benches/fusion_cache.rs` 已有 "fuse vector-only n200" 用例覆盖改动后的快捷路径。

### 4.10 `crates/app/cce-e2e-tests`（召回基准）

- `retrieval_method/fusion.rs`：`include_single_path: bool` 换成 `SinglePathPolicy`
  四值枚举（union / vector-only / bm25-only / intersection），开关对由 `switches()` 给出；
  `FusedEntry` 增加 `paths: PathCoverage`，按**键归属**判定（不能用分数字段：双路键的
  向量归一化分可能就是 0.0，与"无向量分"同值）；新增 `chunk_is_alignable` /
  `count_unalignable`，直接复用生产 `is_alignable`。
- `retrieval_method/aggregation.rs`（新）：多来源合并的离线镜像，键走生产
  `unified_dedup_key`、计分走生产 `combine_aggregate_scores`，代表条目取最强贡献
  （生产规则），不做区间并集；附 `AggregationStat` 统计多来源重叠。
- `retrieval_method/benchmark.rs`：方法矩阵扩展（策略变体、strict、聚合两种策略），
  `AlignmentStat` 增加两路不可对齐计数与占比，新增 `ScoreSummary` / `ScoreStat` 与
  `AggregationBaseline` 收集。
- `retrieval_method/report.rs`：新增 `single_path_policy.md`、`aggregation_strategy.md`、
  `fusion_score_baseline.md`，`alignment_coverage.md` 与 `run_manifest.txt` 增加不可对齐列。
- `docs/outputs_guide.md`：输出清单与评测矩阵同步。
- 为让基准复用生产公式，`coordinator::combine_aggregate_scores` 由私有改为公开。

### 4.11 文档

- 本文件（评估 + 方案）。
- `docs/app/orchestrator/query/hybrid-fusion-design.md`：配置段、分数语义、已知局限同步。
- `docs/app/orchestrator/query/score-normalization-analysis.md`：补记录退化与截断行为。
- `deploy/configs/config.example.toml`：新开关与新段落注释。

## 5. 分数语义变更对照

| 场景（等权 0.5/0.5） | 改造前 | 改造后 | 阈值影响 |
| --- | --- | --- | --- |
| 双路同键双首键 | 1.0 | 1.0 | 不变 |
| 双路同键，单侧命中 | 0.5 | 0.5 | 不变 |
| 单路整体为空，首键 | 1.0 | 0.5 | 需下调阈值 |
| 单路整体为空，末键 | 0.0 | 0.0 | 不变 |
| 聚合同键 2 次命中（0.6 / 0.6） | 0.6 | 0.84 | 需上调阈值 |
| 聚合同键 1 次命中 | 0.6 | 0.6 | 不变 |

`result.min_score` 默认 0.25（`search_config.rs:175`）。单路场景改造后首键 0.5，
阈值 0.25 仍可通过；但若部署把阈值设在 0.6 以上，单路退化查询将全部被过滤——
这正是"现状把单路查询当满分"的隐性依赖，需在发布说明中标注。

## 6. 配置变更与迁移

| 旧 | 新 | 迁移 |
| --- | --- | --- |
| `[search.fusion] include_single_path` | `include_vector_only_keys` / `include_bm25_only_keys` | 旧值 true → 两者 true；旧值 false → 两者 false（旧实现双路模式下就是两侧一起丢弃） |
| 无 | `[search.aggregation] score_strategy` | 默认 `noisy_or`；要旧行为写 `max` |

按项目"不做向后兼容"约定，旧键直接移除，不做别名。

## 7. 影响面

- 召回侧不变：两路检索、over-fetch、过滤逻辑未触碰。
- 排序侧：融合分口径变化 → 命中集合在阈值附近会变化（预期）。
- 展示侧：默认不变；`secondary_evidence` / `entity_scores` 作为可选字段进入搜索响应
  （空值不序列化），前端不用即无感。
- 索引侧：不改；D3 只读 `ChunkRecord`。
- 性能：D3 增加一次批量 SQLite 查询（仅当存在缺键候选）；聚合 noisy-OR 为 O(n)。

## 8. 验证方案

1. `cargo test -p cce-orchestrator --lib query::` 覆盖融合与后处理单测。
2. `cargo test -p cce-orchestrator --test alignment_integration`。
3. `cargo bench -p cce-orchestrator --bench fusion_cache` 对比 `fuse vector-only n200`。
4. `cargo clippy --all-targets --all-features`、`cargo fmt`。
5. 上线首周观察三项：匹配键占比（应上升）、不可对齐占比（应下降）、单路键占比。

## 9. 待拍板开放点

1. **分数下限**：min-max 把路径尾键打到 0，双路共识的尾键会被任何阈值过滤。
   是否引入 `score_floor`（例如 0.2 起算）需基准数据支撑，本轮不做。
2. **聚合权重上限**：noisy-OR 下 `weight > 1` 会饱和，是否在校验层直接拒绝 > 1 的权重，
   需与 `SubQuery` 调用方（LLM 分解结果）对齐后再定。
3. **双证据是否透出到 API**：本轮只落 `SearchResult` 字段并在服务层可选透出，
   是否进入公开响应结构取决于前端是否需要"双路命中"提示。
4. **RRF/Borda 的退化映射**：两算法在单键路径上同样给满分（经权重缩放后为权重占比），
   是否也需要低置信标记，等 D2 的观测结果。

## 10. 落地清单

### 10.1 代码

| 文件 | 改动 |
| --- | --- |
| `crates/core/cce-config/src/modules/search.rs` | 拆单路开关；新增 `AggregationScoreStrategy` / `AggregationConfig` 并挂到 `SearchModuleConfig`；校验改判死配置 |
| `crates/app/cce-orchestrator/src/query/types/search_result.rs` | 新增 `RecallPath` / `SecondaryEvidence` / `EntityScore` 与两个字段，`Default` 补齐 |
| `.../query/types/search_config.rs`、`query/types.rs` | 透出聚合配置与新类型 |
| `.../fusion/normalizer.rs` | `NormalizedPath` + `degenerate` 标记；键数 ≥ 8 时 10/90 分位截断；`minmax_normalize` 语义不变 |
| `.../fusion/aligner.rs` | `is_alignable`（块 id 兜底判定）、`complete_keys_from_records`、`KeyCompletionOutcome`；统计加不可对齐计数与比值 |
| `.../fusion/merger.rs` | `fused_score` 统一计分（含快捷路径）、`secondary_evidence` 写入、`group_by_chunk` 归组并写 `entity_scores`、`union_keys` 对称化 |
| `.../searcher/searcher_core.rs` | 融合前 `complete_alignment_keys`（上限 200/路，失败降级）、指标改传不可对齐数 |
| `.../query/coordinator.rs` | `combine_aggregate_scores`（noisy-OR / max）与 `merge_sources`，聚合去重改用累加 |
| `crates/core/cce-metrics/...` | `record_hybrid_alignment` 增参、新增 `search_hybrid_alignment_unalignable_ratio` 直方图与描述 |
| `crates/app/cce-api/src/models/search.rs` | `secondary_evidence` / `entity_scores` DTO（空值不序列化） |
| `crates/app/cce-server-shared/src/services/search.rs` | DTO 映射 |
| `crates/app/cce-e2e-tests/src/retrieval_method/fusion.rs`（子模块） | 基准镜像跟随开关重命名；`SinglePathPolicy` 四值策略、`PathCoverage`、不可对齐统计 |
| `crates/app/cce-e2e-tests/src/retrieval_method/aggregation.rs`（子模块） | 新增：多来源合并镜像（复用生产键与计分），`AggregationStat` |
| `crates/app/cce-e2e-tests/src/retrieval_method/benchmark.rs`（子模块） | 方法矩阵扩展；`AlignmentStat` 不可对齐列；`ScoreSummary` / `AggregationBaseline` |
| `crates/app/cce-e2e-tests/src/retrieval_method/report.rs`（子模块） | 三份新报告 + 对齐覆盖率/清单列扩展 |
| `deploy/configs/config.example.toml` | 新开关、聚合段、各算法推荐阈值 |

### 10.2 文档

| 文件 | 改动 |
| --- | --- |
| `docs/plan/hybrid-fusion-alignment-fix-2026-11-17.md` | 本文件（评估、决策、方案、落地） |
| `docs/app/orchestrator/query/hybrid-fusion-design.md` | 归一化/计分/键集/归组/补全/聚合六节同步，配置示例与已知局限更新 |
| `docs/app/orchestrator/query/score-normalization-analysis.md` | 追加"落地更新"段，说明为何不采用 0.5 中性值 |
| `crates/app/cce-e2e-tests/docs/outputs_guide.md` | 召回基准的评测矩阵与输出清单同步（三份新报告） |

### 10.3 验证结果

| 命令 | 结果 |
| --- | --- |
| `cargo test -p cce-config -p cce-metrics` | 361 passed |
| `cargo test -p cce-orchestrator --lib query::` | 333 passed |
| `cargo test -p cce-orchestrator --test alignment_integration` | 12 passed |
| `cargo test -p cce-orchestrator` | 607 lib + 12 集成通过；`bm25_alignment_integration::test_multi_entity_chunks_surface_all_entities` 失败 |
| `cargo clippy -p cce-orchestrator -p cce-config -p cce-metrics -p cce-api -p cce-server-shared --all-targets` | 无新增告警（仅 `simvec` 既有告警） |
| `cargo fmt --check` | 通过 |
| `cargo check -p cce-e2e-tests --all-targets` | 通过 |
| `cargo bench -p cce-orchestrator --bench fusion_cache` | 通过：`fuse vector-only n200` 0.21ms、`overlap full n200` 0.35ms、`overlap none n200` 0.57ms、`multi-entity n200` 0.68ms |
| `cargo test -p cce-e2e-tests --lib` | 128 passed（其中 `retrieval_method` 23） |
| `cargo test -p cce-e2e-tests --test e2e retrieval_method` | 5 passed；`fused_evaluation_hits_entity_regardless_of_representative_chunk` 失败（见下） |
| `cargo clippy -p cce-e2e-tests -p cce-orchestrator --all-targets` | 无新增告警（仅既有 `judgments/report.rs`、`full_pipeline_raw.rs`） |

`retrieval_method_alignment::fused_evaluation_hits_entity_regardless_of_representative_chunk`
同样与本次改造无关，成因是**评测器的去重计分规则与逐键区间并集冲突**：该用例里
`e12` 的两个实体键（`e:3` / `e:4`）共用同一对块，逐键并集后两条条目的区间完全相同；
`coverage_over_retrieved` 只在"该条目的区间不被其它条目完全覆盖"时才计一次贡献，
于是两条互相覆盖、贡献计数为 0，而 `recall_any` 仍为 1.0（断言 `any_matches == 2`
因此失败）。该规则与并集语义均在本次改造前就已存在于子模块，与今日改动无关；
修法属评测口径问题（重复条目的贡献归属），留待单独处理，不并入本次基线补充。

前置失败说明：`test_multi_entity_chunks_surface_all_entities` 在未改动的基线
（`8ef6026e` 单独 worktree 复现）上同样失败，断言期望的 "struct+impl 组含多实体"
与索引产出的分片粒度不符，与本次改造无关，已在第 9 节开放点之外单列。

## 11. 术语

- **对齐键**：`e:{entity_id}` / `s:{segment_id}` / `c:{chunk_id}`，跨路匹配的唯一依据。
- **单路键**：只在一条路径出现的对齐键。
- **无键条目**：三条来源全空、无法推导对齐键的条目（当前实现中仅出现在显式空 id 的构造）。
- **块标识兜底**：`segment_id` 为空时用块 id 当分段键，跨路不可对齐。
- **证据强度**：本方案定义的融合分语义，等于该键占可用加权证据的比例。
