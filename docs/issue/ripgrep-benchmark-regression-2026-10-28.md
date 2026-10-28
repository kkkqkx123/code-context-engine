# Ripgrep 基准检索退化：问题清单与归因（2026-10-28）

问题编号：ISS-2026-10-28-001  
状态：分析中  
对比对象：`crates/app/cce-e2e-tests/outputs/benchmark/ripgrep`（当前工作区） vs `ref/ripgrep/bench`（旧基线快照，对应代码基线 `6db71d7`）  
证据来源：`run_manifest.txt` 的 chunk 数、`aggregate_metrics_top{5,10,20,30,50}.md`、`aggregate_metrics_by_query_type_top{5,50}.md`、`per_query_results_top{5,50}.md`、`chunk_shape_diagnostics.md`、`outputs/scenarios/rust/chunks/ripgrep` 的 chunk 文本转储  
关联文档：`docs/issue/flask-benchmark-regression-2026-10-27.md`（Flask 同轮分析）、`docs/plan/full-pipeline-fragmentation-and-group-recovery.md`

## 0. 口径说明

1. **`R_r` 不作为退化指标**：旧基线的 `R_r` 实际上是 `R_a` 的重复计算，新旧 `R_r` 语义不一致，因此本文不把 `R_r` 的变化列为回归问题。主指标为 `R_s` / `R_a` / `P_s` / `F1_s`。
2. **BM25 chunk 数多于 emb 不是问题本身**：emb 的 token 是估算 token，与 BM25 的 term 属于同一量级概念；两种路径的 chunk 数差异不直接说明退化。判断依据是与旧基线的 chunk 数变化和指标变化。
3. **ref 时间戳不可靠**：ref 为从其他设备复制的快照，时间戳不代表生成顺序；代码对应基线以 `6db71d7` 为准。
4. **验证口径**：除 top-5 外，本文同时引用 top-50 与按查询类别聚合指标，以区分“top-k 挤占”与“语义召回能力下降”。

## 1. 结论摘要

本轮 ripgrep 基准的主特征是 **语义查询（semantic）的召回能力大幅下降，且 full-pipeline 路径比 direct_chunking 退化更严重**：

- `R_s` 在 top-5 上：`full_pipeline` emb 0.5889→0.4333（−0.1556），BM25 0.4000→0.3222（−0.0778）；`full_pipeline_raw_source` emb 0.4889→0.3333（−0.1556），BM25 0.4778→0.3444（−0.1334）；`direct_chunking` 基本持平或略升（emb +0.0111，BM25 +0.0111）。
- `R_a` 在 top-5 上同步下降：`full_pipeline` emb 0.5906→0.4296（−0.1610），BM25 0.4084→0.3074（−0.1010）；raw_source emb 0.4832→0.3407（−0.1425）。
- top-50 上语义查询的 `R_s` 塌陷最严重：`full_pipeline` emb semantic 0.8000→0.3500（−0.4500），BM25 semantic 0.5000→0.2250（−0.2750）；`direct_chunking` emb semantic 0.5250→0.3500（−0.1750），BM25 semantic 0.5250→0.2250（−0.3000）。说明退化不是 top-k 挤占，而是语义召回能力本身下降。
- `P_s` 全面下降：`full_pipeline` BM25 0.1644→0.0800（−51%），emb 0.1422→0.1022（−28%）；raw_source BM25 0.2044→0.0800（−61%），emb 0.1289→0.0711（−45%）。
- chunk 数量显著膨胀：`full_pipeline` emb 1645→2579（+57%），BM25 2698→4213（+56%）；`direct_chunking` BM25 1109→3888（+250%），emb 2177→2636（+21%）。更多 fragment 把 top-k 窗口占满，是 `P_s` 下降的直接原因，但不能单独解释 top-50 语义召回的大幅下降。
- chunk 形态本身没有 Flask 那样的同区间重复爆炸（共享区间占比仅 0.3%–1.2%），空 `source_ranges` 为 0。退化不是“声明区间重复”，而是“fragment 数量过多、粒度过细”叠加“语义信号减弱”。

## 2. 指标变化（top-5，`all` 变体）

| 基线 | 检索器 | R_s 旧→新 | R_a 旧→新 | P_s 旧→新 | F1_s 旧→新 |
|------|--------|-----------|-----------|-----------|------------|
| full_pipeline | BM25 | 0.4000→0.3222 | 0.4084→0.3074 | 0.1644→0.0800 | 0.2191→0.1204 |
| full_pipeline | emb | 0.5889→0.4333 | 0.5906→0.4296 | 0.1422→0.1022 | 0.2241→0.1630 |
| full_pipeline_raw_source | BM25 | 0.4778→0.3444 | 0.4852→0.3296 | 0.2044→0.0800 | 0.2650→0.1238 |
| full_pipeline_raw_source | emb | 0.4889→0.3333 | 0.4832→0.3407 | 0.1289→0.0711 | 0.1969→0.1164 |
| direct_chunking | BM25 | 0.4222→0.4333 | 0.4242→0.4370 | 0.0933→0.0756 | 0.1513→0.1249 |
| direct_chunking | emb | 0.4556→0.4667 | 0.4556→0.4667 | 0.1244→0.0889 | 0.1902→0.1460 |

`no_test` 变体趋势一致（`full_pipeline` emb `R_s` 0.5444→0.4111，BM25 0.3778→0.3222）。`impl_only` 为新增维度，历史基线不存在。

## 3. 按查询类别聚合（top-5，`all` 变体）

| 基线 | 检索器 | 类别 | R_s 旧→新 | P_s 旧→新 | R_a 旧→新 |
|------|--------|------|-----------|-----------|-----------|
| full_pipeline | BM25 | fuzzy | 0.6000→0.4000 | 0.2133→0.0800 | 0.6000→0.4000 |
| full_pipeline | BM25 | qualified | 0.7000→0.7000 | 0.3000→0.2000 | 0.7000→0.7000 |
| full_pipeline | BM25 | semantic | 0.1000→0.0750 | 0.0600→0.0200 | 0.1189→0.0417 |
| full_pipeline | emb | fuzzy | 0.6667→0.6667 | 0.1467→0.1467 | 0.6667→0.6667 |
| full_pipeline | emb | qualified | 0.9000→0.6000 | 0.1800→0.1400 | 0.9000→0.6000 |
| full_pipeline | emb | semantic | 0.3750→0.1750 | 0.1200→0.0500 | 0.3788→0.1667 |
| direct_chunking | BM25 | fuzzy | 0.6667→0.8000 | 0.1333→0.1467 | 0.6667→0.8000 |
| direct_chunking | BM25 | qualified | 0.6000→0.7000 | 0.1200→0.1000 | 0.6000→0.7000 |
| direct_chunking | BM25 | semantic | 0.1500→0.0250 | 0.0500→0.0100 | 0.1545→0.0333 |
| direct_chunking | emb | fuzzy | 0.6667→0.7333 | 0.1867→0.1333 | 0.6667→0.7333 |
| direct_chunking | emb | qualified | 0.8000→0.8000 | 0.2200→0.1400 | 0.8000→0.8000 |
| direct_chunking | emb | semantic | 0.1250→0.1000 | 0.0300→0.0300 | 0.1250→0.1000 |

观察：

- **semantic 类别是主战场**：`full_pipeline` emb semantic `R_s` 从 0.3750 降到 0.1750（−53%），BM25 semantic 从 0.1000 降到 0.0750；`direct_chunking` BM25 semantic 从 0.1500 降到 0.0250。
- **qualified 类别**：`full_pipeline` emb qualified 从 0.9000 降到 0.6000（−33%），BM25 qualified 持平（0.7000）；raw_source emb qualified 从 0.6000 降到 0.4000。
- **fuzzy 类别**：`full_pipeline` BM25 fuzzy 从 0.6000 降到 0.4000，emb fuzzy 持平（0.6667）；`direct_chunking` fuzzy 反而提升（BM25 0.6667→0.8000，emb 0.6667→0.7333）。

## 4. top-50 验证（`all` 变体）

| 基线 | 检索器 | 类别 | R_s 旧→新 | P_s 旧→新 |
|------|--------|------|-----------|-----------|
| full_pipeline | BM25 | semantic | 0.5000→0.2250 | 0.0320→0.0060 |
| full_pipeline | emb | semantic | 0.8000→0.3500 | 0.0310→0.0100 |
| full_pipeline | BM25 | qualified | 1.0000→0.9500 | 0.0820→0.0240 |
| full_pipeline | emb | qualified | 1.0000→1.0000 | 0.0300→0.0140 |
| full_pipeline | BM25 | fuzzy | 0.9333→0.9667 | 0.0813→0.0200 |
| full_pipeline | emb | fuzzy | 1.0000→1.0000 | 0.0320→0.0133 |
| direct_chunking | BM25 | semantic | 0.5250→0.2250 | 0.0200→0.0060 |
| direct_chunking | emb | semantic | 0.5250→0.3500 | 0.0210→0.0090 |

结论：semantic 类别的 `R_s` 在 top-50 上仍然大幅下降（full_pipeline emb −0.45），说明退化不是 top-k 窗口挤占，而是语义召回能力本身下降。fuzzy/qualified 在 top-50 上基本可恢复（`R_s` ≥0.93），说明这些类别的退化主要是精度/排序问题，而非召回缺失。

## 5. chunk 数量变化

| 基线 | emb 旧→新 | BM25 旧→新 |
|------|-----------|------------|
| full_pipeline | 1645→2579（+57%） | 2698→4213（+56%） |
| full_pipeline_raw_source | 1645→2579 | 2698→4213 |
| direct_chunking | 2177→2636（+21%） | 1109→3888（+250%） |

BM25 路径膨胀幅度远超 emb 路径（direct_chunking BM25 +250%），但如 §0 所述，BM25 chunk 数多于 emb 并非问题本身；问题在于与旧基线相比的膨胀幅度和指标同步下降。`direct_chunking` 的 BM25 chunk 数从 1109 涨到 3888 是最明显的异常。

## 6. 证据：chunk 输出形态

### 6.1 chunk 形态诊断（当前）

`chunk_shape_diagnostics.md`：

| 基线 | 检索器 | chunks | 空区间 | 共享区间占比 | 最大同区间组 | 重复文本占比 |
|------|--------|--------|--------|--------------|--------------|--------------|
| full_pipeline | emb | 2579 | 0 | 0.5% | 2 | 0.8% |
| full_pipeline | BM25 | 4213 | 0 | 1.2% | 3 | 5.1% |
| raw_source | emb | 2579 | 0 | 0.5% | 2 | 2.2% |
| raw_source | BM25 | 4213 | 0 | 1.2% | 3 | 8.9% |
| direct_chunking | emb | 2636 | 0 | 0.4% | 2 | 1.6% |
| direct_chunking | BM25 | 3888 | 0 | 0.3% | 2 | 9.3% |

与 Flask 不同，ripgrep 当前 **没有** 同区间重复爆炸、**没有** 空 `source_ranges`。共享区间占比 ≤1.2%，说明 range 归因本身是健康的。问题出在数量：chunk 总数膨胀，top-k 窗口被同一目标的多个 fragment 占满，`P_s` 随之下降。

### 6.2 单个实体被切成大量 fragment

`group_14421`（`WalkBuilder` 的 inherent impl）在 emb 路径产生 29 个 chunk，`group_14309`（`DirEntry`）产生 12 个。这些 fragment 各自声明不同的方法/行区间，但都属于同一 group，检索时彼此竞争 top-k 名额。

### 6.3 BM25 侧文本重复

`direct_chunking` 的 BM25 重复文本占比 9.3%，`full_pipeline` BM25 为 5.1%。这些重复主要来自不同实体生成了完全相同的摘要文本（同一成员的重复摘要），与区间重复无关，属于摘要生成侧的独立问题。

### 6.4 GroupFilled 视图的作用

生产对齐的 `*_group` 视图（同 `segment_id` 折叠为 group span）下，`R_s` 从 precise 的 0.32–0.43 升到 0.51–0.57，说明 fragment 级召回失败中有一部分可通过 group 展开恢复。但 semantic 类别的 group 视图 `R_s` 仍仅 0.1667–0.2167（top-5），远低于旧基线，说明 semantic 退化不是 fragment 粒度问题，而是 chunk 文本/语义信号本身退化。

## 7. 根因分析

### 7.1 full-pipeline chunking 变更导致语义信号减弱

semantic 类别在 top-5 和 top-50 上均大幅下降，且 full-pipeline 比 direct_chunking 退化更严重。这指向 full-pipeline 路径的 NL 生成/混合文本组装发生了变化，削弱了语义查询所需的信号。需要对比 `6db71d7` 与当前的 `ast_to_nl` / `chunker` / `chunk_builder` 改动，定位具体变更。

### 7.2 fragment 数量膨胀，top-k 被 Strong 目标占满

chunk 数从 1645/2698 涨到 2579/4213，同一目标方法被切成更多 fragment。top-5 只有 5 个名额，Strong 目标的一个方法就能占掉 2–3 个名额，`P_s` 随之下降（full_pipeline BM25 0.1644→0.0800）。这是 `P_s` 下降的直接原因，但不能单独解释 top-50 语义召回的大幅下降。

### 7.3 merge 策略只按 min 触发，不按 ceiling 填充

当前 merge 判据是“合并后不超过 ceiling，且任一侧低于 min”。一旦两侧都超过 min，即使离 ceiling 还有大量空间也不再合并。结果 chunk 尺寸在刚过 min 后就停止增长，产生大量“贴着 min 的小 fragment”。这与 Flask 同轮分析（`flask-benchmark-regression-2026-10-27.md` §6.3）的结论一致。

- emb：min 200 tokens、max 512 tokens；fragment 中位数落在 min 与 ceiling 之间的偏下区间。
- BM25：min 60 terms、max 150 terms；fragment 只占 ceiling 的 40%–60%。

把 min 调大只会让“停止点”更高一点，不会消除 fragment；正确方向是让合并以 ceiling 为目标、以 min 为下限。

### 7.4 分片深度下探过深

工作区当前的未提交改动把 `Entities` 分片策略扩展到 `Sentences`/`Tokens` 层。对一个包含几十个方法的 inherent impl，这会产生“方法级 + 句子级”的双层 fragment，且句子级 fragment 仍然声明方法级 range。分片深度应该止于“能够独立成 chunk 的最小语义单元”，而不是句子/Token。

### 7.5 BM25 路径的分裂激进度

BM25 路径的 chunk 数膨胀（+56% 至 +250%）远超 emb 路径，说明 BM25 侧的 term 计量或分裂阈值比 emb 侧更激进。`direct_chunking` 的 BM25 chunk 数从 1109 涨到 3888 是最明显的异常，需要检查 BM25 侧的 term 计量与 min 阈值。

### 7.6 对比口径风险

- `ref/ripgrep` 为从其他设备复制的快照，时间戳不可靠；代码对应基线以 `6db71d7` 为准。
- e2e 测试子模块在基线与当前之间也有版本差异，评测语义（GroupFilled、retriever 命名、`R_r` 计算）可能不完全一致。

## 7.8 按查询映射（top-50 仍未恢复的语义查询）

以下 8 个语义查询在 `full_pipeline` emb 的 group 视图下 top-50 仍未完全恢复（`R_s < 1`）。每个查询的目标 chunk 在当前输出中均存在且语义相关，但未进入 top-50，或仅部分 fragment 进入。

| 查询 | 目标范围 | 相交 chunk（节选） | group 视图 top-50 R_s | 判定 |
|------|----------|--------------------|----------------------|------|
| G2Q1 | `regex/src/matcher.rs:409-421`（find_at）；`:53-85`（RegexMatcherBuilder） | `group_323_emb_0` (409-507)；`group_287_emb_2` (49-85) | 0.0000 | 目标 group 未被召回，chunk 数膨胀稀释 |
| G2Q6 | `searcher/src/searcher/mod.rs:373-391`（multi_line）；`glue.rs:142-164` | `group_17407_emb_5` (373-391)；`group_17723_emb_0` (149-351) | 0.0000 | 目标 group 未被召回 |
| G2Q7 | `searcher/src/searcher/mmap.rs:13-25`；`:49-56` | `merged_group_17315_emb_0` (5-105)；`group_17322_emb_1` (53-56) | 0.5000 | 仅一个目标 group 被召回，另一个未召回 |
| G2Q10 | `ignore/src/types.rs:248-289`；`:378-390` | `group_15623_emb_4` (255-308)；`group_15637_emb_3` (378-390) | 0.0000 | 目标 group 未被召回 |
| G2Q12 | `printer/src/standard.rs:1176-1200`；`:1681-1695` | `group_2479_emb_10/11` (1172-1239)；`group_2596_emb_4/5` (1679-1702) | 0.0000 | 目标 group 未被召回 |
| G2Q15 | `searcher/src/searcher/mod.rs:244-266`；`regex/src/error.rs:7-41` | `group_17383_emb_1` (217-262)；`group_885_emb_0` (1-37) | 0.0000 | 目标 group 未被召回 |
| G2Q16 | `regex/src/literal.rs:40-80` | `group_591_emb_0` (11-42)；`group_593_emb_0` (44-123) | 0.0000 | 目标 group 未被召回 |
| G2Q19 | `searcher/src/sink.rs:340-362` | `group_16868_emb_0/1` (345-362) | 0.0000 | 目标 group 未被召回 |

判定依据：对应 chunk 在 `outputs/scenarios/rust/chunks/ripgrep/emb/...` 中存在且文本包含方法名/功能描述，说明 chunk 内容本身不是问题；未被召回的根因是 chunk 总数从 1645 膨胀到 2579，目标 group 在排序中被稀释。对应代码位置：`crates/parser/cce-parser/src/ast_to_nl/chunker/merge/merge_strategy.rs`（合并只按 min 触发）、`crates/parser/cce-parser/src/ast_to_nl/chunker/splitter.rs`（分片深度与 ceiling 填充）、`crates/parser/cce-parser/src/ast_to_nl/chunker/chunk_builder.rs`（fragment 组装）。

## 8. 改进方向

### 8.1 优先修复语义信号

semantic 类别在 top-5/top-50 上均大幅下降，且 full-pipeline 比 direct 退化更严重。应对比 `6db71d7` 与当前的 NL 生成与混合文本组装逻辑，定位削弱语义信号的具体变更，并恢复。

### 8.2 合并按 ceiling 填充

合并判据从“任一侧低于 min”改为“合并后仍不超过 ceiling 且存在未达 target 的一侧”，让 chunk 尺寸向 ceiling 收敛。同时保持“不跨语义单元合并”的约束。

### 8.3 以实体为不可分单元

分片深度止于实体：一个方法在未超限时不得拆分；超限时按内部结构（分支、语句块）拆，且拆出的每片保留准确的行范围。类只在超限时按成员拆，不额外产生句子/Token 级 fragment。

### 8.4 降低 BM25 侧的分裂激进度

BM25 路径的 chunk 数应与 emb 路径同量级。需要检查 BM25 侧的 term 计量与 min 阈值，避免 term 计量导致的过度分裂。

### 8.5 身份头只在必要处注入一次

续片（`index > 0`）只带最小锚点（`Class.method` 一行），不重复控制流和完整 `Calls:` 列表，降低装饰占比，让正文预算更多用于实际代码。

### 8.6 评测保留去重语义，补充冗余诊断

`P_s`/`R_s`/`R_a` 的定义不应改动。应在报告中增加“同 group fragment 数”和“top-k 被同一 group 占据的名额数”列，使 `P_s` 下降可以直接归因到 chunk 形态。

### 8.7 验证方式

- 单测：同一方法的多个 fragment 必须声明互不相同的子区间；合并后尺寸落在 `[min, ceiling]` 且明显高于 min；chunk 数不因句子级下探而膨胀。
- 集成：重新生成 ripgrep 三类基线，要求 semantic 类别的 `R_s`（top-5）恢复到 0.3 以上，`P_s` 恢复到与 `R_s` 同一量级。
- 指标：top-50 semantic `R_s` 恢复到 0.6 以上；chunk 数回归到与旧基线同量级。

## 9. 待确认

1. `ref/ripgrep` 快照对应的代码提交与 e2e 子模块版本，确认 `6db71d7` 为唯一基线。
2. 工作区未提交的 splitter/config 改动是否继续保留；当前指标已包含这些改动，若回退需重新生成基线。
3. BM25 侧 `Calls:` 行与身份头是否属于生成器必要输出，还是可在续片中省略。
4. semantic 查询退化的具体代码变更位置，需通过 `git diff 6db71d7..HEAD` 在 `ast_to_nl` / `chunker` / `chunk_builder` 中定位。
