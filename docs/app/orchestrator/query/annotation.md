# 关系注解子系统设计

## 概述

注解子系统把调用图邻居（被调用者 / 调用者）以保持结构的方式附加到搜索结果上，作为文本注解增强语义检索结果。实现位于 `crates/app/cce-orchestrator/src/query/annotation/`，配置为 `cce-config` search 模块中的 `RelationAnnotationConfig`。

## 架构与数据流

```
RelationAnnotator（协调者，annotator.rs）
    ├── SemanticUnitExtractor（extractor.rs）
    │       └── 按行区间从调用方提供的内容切片出完整代码单元
    ├── SegmentAggregator（aggregator.rs）
    │       └── 聚合同一文件的相邻切片段
    └── StructureConcatenator（concatenator.rs）
            └── 加文件标记、关系标记，去重后拼接
```

## 职责边界：扩展在调用方

`annotate_single` 接收**预先解析好的**前向（callee）与后向（caller）`ExpandedUnit` 列表，只做结构化拼接、去重与预算封顶；图遍历本身在调用方完成（`query/expansion.rs` 的 NeighbourResolver，见 hybrid-fusion-design.md 的召回后扩展章节）。

调用方拥有路径/可见性策略：扩展单元必须先通过查询的排除规则（路径过滤、epoch 视图）再进入 `annotate_single`，保证被排除的邻居不占用扩展预算。注解器内部再做一层防御性过滤（stdlib、external、非调用边剔除），并按分数排序——前向单元优先填充共享配额；配置了 workspace root 时，已消失的文件降级为引用。

## 预算与配额语义

- 每条结果受**单结果配额**封顶。
- 召回条数只由 `annotation_top_n` 与分数阈值决定，**不存在批级 token 上限**——旧的 batch token cap 行为已被 `batch_shared_dedup`（跨结果共享去重）+ 扩展预算机制取代。

## 关键配置分组（RelationAnnotationConfig）

| 分组 | 字段（节选） | 作用 |
|---|---|---|
| 基础开关 | `enable_annotation`、`omit_primary_body`、`omit_self_evident_edges` | 是否注解、是否省略主实体正文、省略显而易见边 |
| 结构边 | `filter_stdlib`、`filter_external`、`allow_structural_edges` | 防御性过滤与结构边白名单 |
| 段聚合 | `enable_segment_merge`、`segment_merge_gap` | 相邻段合并开关与行距阈值 |
| 扩展 | `expansion_enabled`、`forward/backward_max_expanded_units`、`expansion_include_callers`、`expansion_domains`、`expansion_scoring`、`expansion_query_scoring` | 扩展方向、方向配额、领域白名单与评分 |
| 共享预算 | `batch_shared_dedup`、fan-in centrality | 跨结果去重与按扇入中心性分配预算 |

## 集成点

`enable_annotation` 开启时，搜索器在排序与阈值过滤之后对最终 top-N 结果注解（`query/searcher/post_processing.rs`），不改变排序本身。
