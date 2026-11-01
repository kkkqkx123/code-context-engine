# cce-orchestrator 设计文档目录

本目录收录编排层（`crates/app/cce-orchestrator`）各子系统的设计文档。均以简洁自然语言描述当前实现，类型名与路径以代码为准。

## query/ —— 查询管线

| 文档 | 内容 |
|---|---|
| [retrieval-strategies.md](query/retrieval-strategies.md) | 检索策略层：dense/bm25 策略、over-fetch、SearchFilter 构造 |
| [hybrid-fusion-design.md](query/hybrid-fusion-design.md) | 混合融合：加权算法族、意图权重、召回后扩展 |
| [path-filtering-pipeline.md](query/path-filtering-pipeline.md) | 多层路径过滤管线：epoch 下推、策略内 Glob、融合后 enrichment |
| [score-normalization-analysis.md](query/score-normalization-analysis.md) | 分数归一化分析与 RRF/BordaCount 现状 |
| [ranking.md](query/ranking.md) | 排序与重排：LLM/插件 reranker、确定性排序、阈值过滤 |
| [annotation.md](query/annotation.md) | 关系注解子系统：抽取/聚合/拼接与扩展预算语义 |
| [graph.md](query/graph.md) | 独立图检索路径：ego 邻域、最短路径、连通分量 |
| [caching.md](query/caching.md) | 查询缓存与容错：结果缓存、CachedEmbedder、能力协商、重试队列 |
| [tantivy-path-filter-capability.md](query/tantivy-path-filter-capability.md) | BM25 存储层的路径过滤能力现状（原缺口分析的重写版） |

## tools/ —— 查询工具

| 文档 | 内容 |
|---|---|
| [module-design.md](tools/module-design.md) | 符号查找工具当前架构：LayeredSnapshotIndex + RelationStore |
| [symbol-lookup-design.md](tools/symbol-lookup-design.md) | findReferences / getSymbols / gotoDefinition 三工具设计 |
| [stateless-vs-stateful-design.md](tools/stateless-vs-stateful-design.md) | 工具无状态/有状态分类与数据源边界 |
| [ast-diagnosis.md](tools/ast-diagnosis.md) | AST 诊断：错误收集、最内层过滤、各语言细化策略 |
| [compression.md](tools/compression.md) | 代码压缩工具：请求/响应契约与处理流程 |

## index/ —— 索引流水线

| 文档 | 内容 |
|---|---|
| [pipeline.md](index/pipeline.md) | 全量/增量索引编排：批处理、checkpoint、dead_letter、存储协调、关系发布 |

## hot_update/ —— 热更新

| 文档 | 内容 |
|---|---|
| [architecture.md](hot_update/architecture.md) | 事件驱动热更新：watcher/debounce/变更检测/处理器/OperationRuntime |

## 与其他文档目录的边界

- **operation 与 index_state 模块**：`docs/architecture/state/OPERATION_AND_INDEX_STATE_DESIGN.md`
- **数据流总览**（实体/关系从扫描到入库）：`docs/architecture/data-stream/`
- **存储后端选型与契约**：`docs/storage/`
- **导出（export）子系统**：`docs/export/`
- **对外 API 行为**：`docs/app/api/`
- **热更新恢复演练**：`docs/tests/hot_update_recovery_workflow.md`
