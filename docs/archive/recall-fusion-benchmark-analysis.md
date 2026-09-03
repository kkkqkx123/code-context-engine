# 召回聚合基准测试分析（对齐修复后复测）

> **状态：已复测。** 根因（full_pipeline 双解析导致两路径 EntityId 不相交）已修复
> （commit `5816511a`），三 fixture 均已用修复后代码重新生成数据并评测：
> ripgrep 对齐 95.0%、flask 98.5%、once_cell full_pipeline 96.8%、直接分块 ~100%。
> **cross_lang 查询类型仅存在于 once_cell（4 条，中-英对照演示用途），不参与任何结论，直接忽略。**

## 一、数据概况

- 方法：`emb` / `bm25` / `minmax-α`（按实体键 min-max 归一化加权融合）/ `rrf-k`。
- 基线：`direct_chunking` / `full_pipeline` / `full_pipeline_raw_source`。
- 查询类型：`qualified` / `fuzzy` / `semantic`（`cross_lang` 忽略）。
- 指标：recall@k（R_s）为主、F1 为辅（weight_sensitivity 为 F1@5）。

## 二、复测观察

| 方法 | 优势场景 | 劣势 / 不可用场景 |
|---|---|---|
| emb | 召回锚点：flask full_pipeline R=98.5%、ripgrep 85.5%、once_cell full 91.5%；fuzzy/qualified 常 100% @30 | 精准度低（F1 0.09~0.18，once_cell direct 除外 0.42） |
| bm25 | flask 召回高（94.4%）；raw_source 下 P/F1 反而最高（flask 0.27、once_cell 0.53 vs 各自 emb） | ripgrep 弱（67.2%）；ripgrep semantic 50% 上下 |
| minmax-α | full_pipeline 下 F1 显著增益：ripgrep 0.32 vs emb 0.10（3.4 倍）、flask 0.185 vs 0.106（1.7 倍）、once_cell 0.38 vs 0.17（2.3 倍）；flask α=0.6/0.7 召回与 emb 平齐、once_cell 融合召回反超 emb | 召回一般不高于 emb 单路径（ripgrep 0.81 < 0.85 为反例）；α=0.9 尾端损失（ripgrep fuzzy 召回 80%、once_cell F1@5 明显回落）；flask/once_cell 直接分块下融合 F1 低于 emb |
| rrf | 与 minmax 相当；once_cell full_pipeline 下 rrf-100 召回最高（94.9%）；k 大多不敏感 | 无独立短板 |

### 权重敏感性（F1@5，按查询类型）

| fixture/baseline | qualified | fuzzy | semantic |
|---|---|---|---|
| ripgrep full_pipeline | 峰值 α=0.7（0.5063）；0.1 时 0.4032 | 峰值 α=0.6（0.4690）；0.9 回落到 0.3841 | 随 α 上升：0.1→0.0875，0.9→0.2070；仅 α=0.9 融合增益为正（+0.077） |
| flask full_pipeline | 单调随 α 上升：0.1→0.1905，0.9→0.4921 | 单调：0.1→0.2444，0.9→0.5317 | 不敏感：全 α ≈ 0.31（std 0.0065），融合增益全为负 |
| once_cell full_pipeline | 峰值 α=0.3~0.4（0.5055）；0.9 回落到 0.3095 | 峰值 α=0.6（0.4865）；0.9 回落到 0.3095 | 峰值 α=0.3（0.5355）；融合增益为正（F1@30 +0.07~+0.23） |
| once_cell direct_chunking | 峰值 α=0.1（0.6771） | 峰值 α=0.1（0.5759） | 峰值 α=0.9（0.5600），增益大正（+0.56 左右） |

补充：once_cell full_pipeline 的 minmax 在 k=30 下最优 F1 反而在 α=0.9（0.3823），与 F1@5 峰值（0.3~0.6）不同，说明最优 α 随 k 变化。

## 三、结论（取代旧文档初步观察）

1. **旧数据中的异常基本为测量伪影，复测后消失**：α≤0.3"全面崩溃/大量零命中"、qualified 权重
   非单调（0.4→70%、0.5→100%）、once_cell"偏好低 α"均未复现或已修正；
   复测后 qualified 在 ripgrep α≥0.4 召回稳定 0.9，低 α 仅 F1 温和下降。
2. **真实存在的行为**：α=0.9 尾端损失（ripgrep fuzzy 召回 0.8 vs α=0.6 的 1.0；once_cell
   fuzzy/qualified 的 F1@5 自峰值 0.49~0.51 回落至 0.31）。
3. **semantic 融合收益依语料分歧**：ripgrep（仅 α=0.9 微正 +0.077）与 flask（全 α 为负）
   不支持融合；once_cell full_pipeline/direct_chunking 为显著正增益（F1@30 +0.07~+0.66）。
   "semantic 一律不融合"不可行，需 per-project 判定。
4. **emb 是召回底座，融合价值集中在 F1**（full_pipeline 下 1.7~3.4 倍）；融合召回与单路径的
   关系依语料：ripgrep 低于 emb（0.81 < 0.85）、flask 平齐（0.985）、once_cell 反超
   （minmax-0.6 0.935 vs 0.915，rrf-100 0.949）。
5. **最优 α 依语料与基线分歧**：flask 单调偏好高 α（0.9）、ripgrep 偏好 0.6~0.7、
   once_cell full_pipeline 偏好 0.3~0.6、once_cell direct_chunking 偏好低 α（0.1，BM25 主导）。
   单一全局权重无法兼顾，权重需按语料标定。
6. **直接分块基线下融合收益小甚至为负**（flask/once_cell direct 融合 F1 < emb F1），
   融合收益集中在 AST-to-NL 的 full_pipeline 路径；once_cell direct 的 semantic 为例外（正增益）。
7. **rrf 与 minmax 结论一致**；once_cell full_pipeline 下 rrf-100 召回最高，k 值敏感度低。

## 四、遗留事项

- 网格搜索需按 k 分别选参：once_cell 显示 F1@5 与 F1@30 的最优 α 不一致（0.3~0.6 vs 0.9）。
- 权重设计落地方案见 `docs/plan/per-query-type-minmax-weights-design.md`（已按本复测修订）。
