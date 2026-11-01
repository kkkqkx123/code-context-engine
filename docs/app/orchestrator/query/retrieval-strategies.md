# 检索策略层设计

> 模块路径：`crates/app/cce-orchestrator/src/query/retrieval/`
> 本文描述查询链路中"召回"阶段的策略抽象与两个具体实现。

## 1. 定位与总体结构

检索策略层位于查询流水线的最前端，负责根据查询意图从索引后端召回原始候选集（`SearchResult`）。策略层只产出"裸结果"——内容、行号、实体映射等富化（enrichment）在后续流水线中完成（`strategies/dense.rs:43-45` 的文档注释明确了这一约定）。

模块组成（`retrieval.rs` / `strategies.rs`）：

- `strategies/`：策略枚举与具体策略（`dense.rs`、`bm25.rs`、`strategy_enum.rs`）。
- `core/`：底层无状态检索实现，直接对接存储后端（`core.rs:1-5`）。`core/dense.rs` 提供 `DenseRetrieval`，`core/vector.rs` 提供 `FilterOptions`。
- `post_processing/fusion/`：融合层，负责将多条召回路径的结果合并为最终候选。

## 2. 策略枚举与执行入口

`strategies/strategy_enum.rs` 定义了两个核心类型：

- `RecallAlgorithm` 枚举：`Dense`（纯向量语义检索）与 `Bm25`（纯关键词检索），见 `strategy_enum.rs:16-22`。
- `RetrievalStrategy` 枚举：对两个具体策略的静态分发包装（`strategy_enum.rs:49-52`），避免 trait object 的动态分发开销。

执行入口与工厂：

- `RecallAlgorithm::create_strategy(&Searcher)` 按枚举值构造对应策略（`strategy_enum.rs:26-37`）。BM25 分支通过 `Searcher::extract_fulltext_store` 从 searcher 中取出全文检索存储句柄。
- `RetrievalStrategy::retrieve(&QueryOptions, &QueryFilter)` 是统一执行入口，返回 `Result<Vec<SearchResult>>`（`strategy_enum.rs:56-65`）。
- `RetrievalStrategy::name()` 返回策略名（"dense"/"bm25"），用于日志与监控（`strategy_enum.rs:68-73`）。

## 3. Dense 策略（向量检索）

`strategies/dense.rs` 中的 `DenseStrategy` 编排"嵌入计算 + 向量检索"两步（`dense.rs:46-85`）：

1. 调用 `searcher.embedder.embed_one` 计算查询向量（`dense.rs:52-57`）。
2. 通过 `build_search_filter` 构造 `SearchFilter`（`dense.rs:88-117`）：填充 `group_id`（来自 `scope.project_group_id()`）、`point_type = PointKind::Chunk`、`directory_prefix`、内容类型排除标记（test/generated/dependency）、分类包含/排除列表；再经 `QueryFilter::apply_to_search_filter` 叠加 epoch 与排除文件过滤（`dense.rs:60-61`）。
3. 计算取回数量并调用核心层 `DenseRetrieval::search`，传入 `similarity_threshold` 与 `hnsw_ef`（`dense.rs:74-83`）。

### GLOB_OVER_FETCH_FACTOR 超取回填

存储后端不解析 glob 模式，因此当查询带有 include/exclude glob 模式时，策略按 `GLOB_OVER_FETCH_FACTOR = 3` 超取（至少 `top_k + 20`），本地过滤后再截断回 `top_k`（`dense.rs:63-73`，常量定义于 `dense.rs:119-123`）。内容类型排除是已建索引的标记，由存储侧下推执行，不需要超取（`dense.rs:64-66`）。

本地过滤由 `apply_strategy_filters` 完成（`dense.rs:130-148`）：复用 `post_processing::GlobFilter` 做 glob 过滤，保持原有排序并截断到 limit。

## 4. BM25 策略（关键词检索）

`strategies/bm25.rs` 中的 `Bm25Strategy` 持有 `FulltextStore` 句柄（`bm25.rs:33-35`），通过 fulltext 契约调用后端，本地与远程分支共享同一读路径（`bm25.rs:30-31`）。

执行流程（`bm25.rs:56-159`）：

1. 构造 `FulltextSearchOptions`（`bm25.rs:74-97`）：limit、`field_weights`（来自 `config.bm25.field_weights`）、原生 `project_id` 隔离（多租户隔离在索引内完成，不依赖外部 SQLite 查询，见 `bm25.rs:11-15`）、epoch 过滤、内容类型排除标记、分类过滤、`term_operator`。
2. 调用 `self.fulltext.search(&options.query, &retrieval_options)`（`bm25.rs:101-105`）。
3. 将 BM25 结果映射为统一的 `SearchResult`：从索引字段提取 `chunk_id`、`file_path`、`title`，通过共享编解码器 `ChunkEntityRefs::parse_bm25_csv` 解析实体列表（`bm25.rs:118-123`）；`content` 为仅索引不存储的字段，留空，由后处理从 SQLite 富化（`bm25.rs:124-126`）。

### 召回侧过滤与超取

`apply_bm25_strategy_filters`（`bm25.rs:162-192`）在召回侧做三件事：

- `GlobFilter` 本地 glob 过滤；
- `term_weight_threshold` 分数下限：低于阈值的候选直接淘汰，不进入融合层。这是原始词权重域的"成本阀门"，默认 0.0 时禁用（`bm25.rs:168-171`，阈值来自 `config.bm25.term_weight_threshold`）；
- `directory_prefix_allows` 目录前缀检查（`bm25.rs:199-210`）：BM25 索引没有前缀字段，所以是查询时检查，语义与向量存储的前缀语义一致（精确匹配或以斜杠为边界的路径前缀，基于 `normalize_project_path` 归一化）。

带 glob 或 directory_prefix 时同样按 3 倍超取（至少 `base_limit + 20`，上限 200），以回填被过滤掉的名额（`bm25.rs:66-73`）。

### 错误分类

`classify_bm25_error`（`bm25.rs:217-224`）区分永久性配置错误（`QueryError::Config`）与瞬态运行错误（`QueryError::Retryable`，service 标记为 "bm25"），后者可由查询重试机制重新处理。

## 5. 与融合层（fusion）的关系

**hybrid 不是独立的第三种策略**。策略层只提供 dense 与 bm25 两条独立召回路径；"hybrid" 是融合层对这两路结果的对齐合并（见 `post_processing/fusion.rs:1-11` 的模块文档）：

- 融合在对齐键（alignment key）级别进行：优先 entity id，其次 segment id，再次 chunk id。
- 融合算法由配置 `[search.fusion]` 中的 `RecallFusionAlgorithm` 决定：归一化加权求和（默认）、RRF（倒数排名融合）或权重感知 Borda 计数。
- 每种查询意图的权重档位与插件权重覆盖只调换权重对，不会隐式改变算法。

因此架构是"两条召回策略 + 一个融合器"，`RecallAlgorithm` 枚举中刻意没有 Hybrid 变体。

## 6. raw_filter 业务路径拒绝

`SearchFilter` 上的 `raw_filter` 字段是后端调试用的逃生通道。核心层 `DenseRetrieval::search` 在入口处检查 `filter.is_business_query()`，若过滤器携带 `raw_filter` 则返回 `QueryError::InvalidQuery`，拒绝其在业务查询路径上使用（`core/dense.rs:50-55`）。策略层的 `build_search_filter` 始终将 `raw_filter` 置为 `None`（`dense.rs:115`），从源头保证业务路径不带原始过滤表达式。相关回归测试见 `core/dense.rs:128-158`。

## 7. 依赖关系小结

```
QueryOptions / QueryFilter
        │
RetrievalStrategy (enum, static dispatch)
   ├── DenseStrategy ──> embedder.embed_one
   │                 └─> DenseRetrieval (core) ──> VectorStore.search_dense
   └── Bm25Strategy  ───────────────────────────> FulltextStore.search
        │
   post_processing::GlobFilter（本地 glob / 阈值 / 前缀过滤）
        │
   fusion::fuse_hybrid_results（hybrid 模式下合并两路）
```
