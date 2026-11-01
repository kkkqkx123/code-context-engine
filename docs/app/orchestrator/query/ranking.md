# 排序与重排设计

## 概述

排序层位于检索融合之后、结果返回之前，负责对候选结果做可选的重排、确定性排序与最终截断。实现位于 `crates/app/cce-orchestrator/src/query/ranking/`，均为纯算法组件，不感知存储层。

## 组件与流水线

搜索器（`query/searcher/searcher_core.rs`）在后处理阶段按以下顺序调用排序层：

```
融合候选 ──► Reranker（LLM 或插件，可选） ──► ScoreSorter（确定性排序） ──► ThresholdFilter（阈值 + limit）
```

### LlmReranker（llm_reranker.rs）

- 基于 `cce-llm-client` 的 `ProductionRerankHandler`，由配置层在 `[rerank] enabled = true` 时构建 handler；每请求可通过 `QueryOptions::enable_rerank` 强制开/关。
- 候选数量不是固定的：`select_candidate_count` 依据分数分布（`max_candidates` / `min_candidates` / `score_drop_threshold` / `min_score` / `drop_detection_start`）动态选择，只把选出的候选发给模型。
- 容错语义：handler 缺失、候选为空、熔断器打开（provider 不健康）或请求失败时，一律保持原始顺序返回，不抛错中断查询。

### PluginReranker（plugin_reranker.rs）

- 由具备 `Rerank` 能力的插件（Lua / native，见 `cce-plugin` 的 `CodePlugin::rerank`）执行重排；首个返回非空结果的插件生效。
- 插件调用是同步阻塞的，经 `tokio::task::spawn_blocking` 移出异步运行时，Lua/native 适配器自带硬超时。
- 候选选择与合并复用 `common.rs` 的同一套函数，失败同样回退原始顺序。

### ScoreSorter（score_sorter.rs）

按分数降序排序，平分时以稳定内容坐标（文件路径、行区间、kind、名称）决胜，绝不使用索引分配的标识符，保证重建索引后相同输入得到相同顺序。

### ThresholdFilter（threshold_filter.rs）

最终门槛：按 `SearchConfig.result.min_score` 保留，再截断到 `result.limit`。

### 分数归一化

排序层之前，`query/boost.rs` 提供候选分数归一化（`NormalizationStrategy` / `normalize_scores`），使不同召回模式（向量 / BM25 / 混合）产出的分数落在同一量纲上，阈值过滤才有统一语义。归一化与融合层的分数分析另见同目录 `score-normalization-analysis.md`。

## 配置要点

- `[rerank]`（`cce-config` search 模块）：enabled、max/min_candidates、score_drop_threshold、min_score、temperature、return_reasoning、score_fusion_strategy、timeout_ms。
- `[search.result]`：min_score、limit —— 由 ThresholdFilter 消费。
- 开关优先级：配置层决定 handler 是否存在，`QueryOptions::enable_rerank` 可按请求覆盖。
