# Rerank 基准分析报告（once_cell fixture）

> 数据自包含：本文所有表格均来自 2026-09-17 的一次完整重测快照，后续基准数据变化不影响本文引用的有效性。生成工具与复现命令见文末。
>
> 关联设计文档：`docs/benchmark/rerank-benchmark-design.md`（基准设计）；`docs/analysis/pipeline-rerank-degradation.md`（结论修正记录）。

## 1. 测量设置

| 项 | 值 |
|---|---|
| fixture | `once_cell`（Rust review fixture，`fixtures/rust/review/once_cell`） |
| 真值 | 39 条判断中的 CoreRetrieval scope 35 条（qualified 8 / fuzzy 15 / semantic 12；cross_lang 4 条诊断查询已排除） |
| 基线（baseline） | `direct_chunking`（源码直接切块）、`full_pipeline`（AST→NL 摘要双路）、`full_pipeline_raw_source`（NL 摘要替换为原始源码文本的 pipeline 对照） |
| 召回路 | 仅 emb 路（NL 摘要文本 + bge-m3 向量召回）。bm25 路与 minmax 融合路已移出 rerank 矩阵（理由见 §2） |
| rerank 模型 | `bge-reranker` → `BAAI/bge-reranker-v2-m3`，cross_encoder 模式，SiliconFlow `/rerank` 端点 |
| 候选深度 | 每查询取召回序前 50，请求侧 500 字符截断（镜像生产配置） |
| 融合变体 | `rerank_only`（纯 cross-encoder 序）、`linear_weighted(alpha=0.7)` + 初始分逐查询 min-max 归一化（linear+norm） |
| 候选文本源 | `emb-text`：实体对应的 NL 摘要文本（embedder 消费的文本）；`raw-code`：实体按 chunk 行号映射回的 fixture 原始源码 |
| 指标 | top-5 任意档召回率 R_any（主指标）、F1_any、强相关档 R_strong、首命中位次 1st_hit |
| 对照语义 | control 与 reranked 共享同一候选列表，唯一差异是重排序；运行 0 失败样本 |

## 2. 基准矩阵的前置修正（相对初版设计）

初版矩阵（bm25 / minmax-0.5 / emb 三路 × rerank）暴露出两类测试设计问题，本轮重构：

1. **bm25 与 minmax-0.5 路径移出 rerank 矩阵**。rerank 是语义重打分步骤，只应定义在语义召回路上。初版数据显示 cross-encoder 对 BM25 混合增强文本打分严格劣于召回序——该文本为词面匹配设计（token 展开重复流），不是给语言模型使用的。融合基线由 `retrieval_method` 基准独立覆盖，minmax-0.5 行与之重复，一并移除。
2. **cross_lang 查询移出评估**。G4 系中文查询为诊断用途；在部分 baseline 下 BM25 初始分恒为 0、候选集由空结果兜底，混入均值只产生误导波动。
3. **候选文本源参数化**（`RerankTextSource`），对 `emb-text` 与 `raw-code` 各生成一套 sidecar 并对比。BM25 文本被明确排除。

## 3. 主结果（top-5 R_any / R_strong）

四组离线评估（2 文本源 × 2 融合变体），control 行在四组间相同：

### 3.1 控制组（不开 rerank，召回序）

| baseline | R_any | F1_any | R_strong | 1st_hit | emb chunks | bm25 chunks |
|---|---|---|---|---|---|---|
| direct_chunking | 0.852 | 0.582 | 0.857 | 1.8 | 170 | 104 |
| full_pipeline | 0.667 | 0.286 | 0.676 | 2.4 | 121 | 93 |
| full_pipeline_raw_source | 0.457 | 0.224 | 0.469 | 2.5 | 121 | 93 |

### 3.2 emb-text 文本源（NL 摘要文本）

| baseline | rerank_only R_any | linear+norm R_any | rerank_only R_strong | linear+norm R_strong |
|---|---|---|---|---|
| direct_chunking | 0.786 | 0.817 | 0.790 | 0.824 |
| full_pipeline | 0.582 | 0.609 | 0.583 | 0.612 |
| full_pipeline_raw_source | 0.460 | 0.450 | 0.467 | 0.462 |

### 3.3 raw-code 文本源（原始源码）

| baseline | rerank_only R_any | linear+norm R_any | rerank_only R_strong | linear+norm R_strong |
|---|---|---|---|---|
| direct_chunking | 0.176 | 0.522 | 0.176 | 0.521 |
| full_pipeline | 0.172 | 0.197 | 0.167 | 0.193 |
| full_pipeline_raw_source | 0.177 | 0.215 | 0.176 | 0.217 |

### 3.4 与 control 的差值汇总（dR_any，top-5）

| baseline | rerank_only(emb-text) | linear+norm(emb-text) | rerank_only(raw-code) | linear+norm(raw-code) |
|---|---|---|---|---|
| direct_chunking | -0.066 | -0.035 | -0.676 | -0.330 |
| full_pipeline | -0.085 | -0.058 | -0.495 | -0.470 |
| full_pipeline_raw_source | +0.003 | -0.007 | -0.280 | -0.242 |

## 4. 按查询类型的增益明细（dR_any，reranked − control，top-5）

### 4.1 emb-text × rerank_only

| baseline | qualified (8) | fuzzy (15) | semantic (12) |
|---|---|---|---|
| direct_chunking | 0.000 (1-5-2) | -0.067 (0-6-9) | -0.112 (2-2-8) |
| full_pipeline | -0.125 (0-7-1) | -0.133 (1-11-3) | +0.002 (3-6-3) |
| full_pipeline_raw_source | 0.000 (2-4-2) | +0.067 (4-8-3) | -0.074 (2-5-5) |

### 4.2 emb-text × linear+norm

| baseline | qualified (8) | fuzzy (15) | semantic (12) |
|---|---|---|---|
| direct_chunking | 0.000 (2-3-3) | 0.000 (2-6-7) | -0.102 (1-6-5) |
| full_pipeline | -0.125 (0-7-1) | -0.067 (1-12-2) | -0.004 (3-6-3) |
| full_pipeline_raw_source | 0.000 (2-4-2) | 0.000 (3-9-3) | -0.021 (2-8-2) |

### 4.3 raw-code × rerank_only

| baseline | qualified (8) | fuzzy (15) | semantic (12) |
|---|---|---|---|
| direct_chunking | -0.875 (0-1-7) | -0.867 (0-2-13) | -0.305 (3-1-8) |
| full_pipeline | -0.875 (0-1-7) | -0.600 (0-6-9) | -0.111 (4-2-6) |
| full_pipeline_raw_source | -0.375 (0-5-3) | -0.333 (1-8-6) | -0.151 (1-6-5) |

### 4.4 raw-code × linear+norm

| baseline | qualified (8) | fuzzy (15) | semantic (12) |
|---|---|---|---|
| direct_chunking | -0.250 (0-2-6) | -0.467 (0-6-9) | -0.213 (2-4-6) |
| full_pipeline | -0.875 (0-1-7) | -0.533 (0-7-8) | -0.122 (1-7-4) |
| full_pipeline_raw_source | -0.250 (0-6-2) | -0.333 (1-8-6) | -0.124 (0-7-5) |

（括号内为 wins-ties-losses 逐查询胜负计数。）

## 5. 延迟与成本（emb-text 侧，两组融合变体相同调用）

| baseline | calls | 失败率 | avg_ms | p50_ms | max_ms | 平均候选数 |
|---|---|---|---|---|---|---|
| full_pipeline | 35 | 0 | 642.8 | 611 | 1016 | 50.0 |
| full_pipeline_raw_source | 35 | 0 | 705.8 | 620 | 1543 | 50.0 |
| direct_chunking | 35 | 0 | 663.4 | 615 | 1367 | 50.0 |

每查询固定 1 次 `/rerank` 调用、50 候选，均值 ~650ms。在收益为负的前提下，这是纯成本。
