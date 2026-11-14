# 文件路径过滤流水线

## 1. 概述

本文档描述 code-context-engine 查询链路中的文件路径过滤机制。过滤已重构为**多层下推**结构：epoch 视图与内容类型排除下推到存储层（向量后端与 BM25 索引），Glob 过滤前移到检索策略内部并配合超量召回回填（over-fetch refill），SQLite enrichment 在融合之后对存活候选集一次性执行。

### 核心目标

- 允许按目录前缀、预定义内容类型（测试/生成/第三方）与文件类别（category）过滤结果
- 支持用户自定义 Glob 包含/排除规则，且在融合与 enrichment **之前**生效，避免无效候选进入后续阶段
- 保证所有查询只能看到已发布 generation 的 epoch 视图（版本隔离）

---

## 2. 请求路径总览

```
SearchRequest (API)
    │
    ▼
Searcher::search()
    │  每请求一次：load_active_query_filter(_from_store)
    │  → QueryFilter { own_epoch, parent_epoch, excluded_files }
    ▼
execute_search_flow()
    │
    ├─ Step 1: Retrieval（检索策略内部过滤）────────────
    │   ├─ DenseStrategy   → SearchFilter 下推（epoch/排除文件/目录/类别）
    │   └─ Bm25Strategy    → FulltextSearchOptions 下推 + 策略内 GlobFilter
    │
    ├─ Step 2: Hybrid 融合（dense + bm25 在融合层组合，无独立 hybrid 策略）
    │   └─ expand_multi_entity_results → fuse_hybrid_results
    │
    ├─ Step 3: 可选分数归一化（config.score.enable）
    │
    ├─ Step 4: SQLite enrichment（对存活候选集一次性回填）
    │
    └─ Step 5: 后处理（rerank → sort → threshold filter → 标注）
```

过滤位置与旧版的关键差异：Glob 过滤不再位于后处理流水线末端，而是在**检索策略内部**、在融合与 enrichment 之前执行。

---

## 3. QueryFilter：epoch 视图下推

**文件**：`crates/app/cce-orchestrator/src/query/filter.rs`

`QueryFilter` 携带活动发布的完整可见视图：

- `own_epoch`：活动 generation 自身写入的数据 epoch
- `parent_epoch`：继承的父 generation（其行保持可见，除非被覆盖）
- `excluded_files`：在 `generation_overrides` 中登记的文件，其父代行不可见

可见数据 = 自有行 ∪ 父代行 − 被覆盖文件。视图每次请求从 SQLite 活动清单（或经 `RelationStore` 的后端中立契约）重新推导，不做进程内缓存，保证发布切换（adopt/rollback）对下一次查询立即可见。

两个下推入口：

- `to_search_filter()`：构造携带完整 epoch 视图的 `cce_storage_common::SearchFilter`（`epochs` + `excluded_files`），用于向量检索。
- `apply_to_search_filter(filter)`：把 epoch 视图合并进已存在的 `SearchFilter`（覆盖原有 epoch 字段）。

BM25 侧不经过 `SearchFilter`，而是由 `Bm25Strategy` 把 `query_filter.epochs()` 与 `excluded_files()` 直接写入 `FulltextSearchOptions`。

---

## 4. 检索策略与过滤下推

检索策略位于 `crates/app/cce-orchestrator/src/query/retrieval/strategies/`，只有两个实现：`dense.rs` 与 `bm25.rs`，由 `strategy_enum.rs` 的 `RecallAlgorithm::{Dense, Bm25}` 静态分发。**不存在 hybrid.rs**：混合检索由融合层把 dense 与 bm25 两条召回组合，策略层只负责单路径召回。

### 4.1 Dense 策略

**文件**：`strategies/dense.rs`

策略内部 `build_search_filter()` 直接构造 `cce_storage_common::SearchFilter`：

| 字段 | 来源 |
|---|---|
| `group_id` | 项目作用域 `project_group_id()` |
| `point_type` | 固定 `PointKind::Chunk` |
| `directory_prefix` | `options.directory_prefix` |
| `exclude_test` / `exclude_generated` / `exclude_dependency` | `exclude_content_types` 是否含对应 `ExcludableContentType` |
| `include_categories` / `exclude_categories` | `QueryOptions.include_categories` / `exclude_categories` |
| `epochs` / `excluded_files` | 经 `query_filter.apply_to_search_filter()` 注入 |
| `raw_filter` | 恒为 `None`（见 4.3） |

随后以超量召回（over-fetch）弥补 Glob 过滤的候选损失：仅当存在 include/exclude Glob 模式时按 `GLOB_OVER_FETCH_FACTOR = 3`（且至少 `top_k + 20`）超取，策略内 `apply_strategy_filters()` 用 `GlobFilter` 本地过滤后截断回 `top_k`。内容类型排除是已索引的标记，直接下推到存储层，无需超取。

底层检索在 `retrieval/core/dense.rs` 的 `DenseRetrieval::search()`，把 `SearchFilter` 附着到 `DenseSearchQuery` 交给向量后端（后端翻译为原生过滤谓词或 Qdrant filter JSON）。

### 4.2 BM25 策略

**文件**：`strategies/bm25.rs`

BM25 召回侧同样应用过滤：构造 `cce_storage_common::FulltextSearchOptions`，携带 `project_id`、`epochs`、`excluded_files`、`exclude_test/generated/dependency`、`include_categories/exclude_categories`、`field_weights`、`term_operator`，经 `FulltextStorage` 契约执行搜索（本地 Tantivy 与远程 search-service 分支共享同一读路径）。epoch 过滤与被覆盖文件排除在 BM25 索引内部以 `BooleanQuery` 组合完成（见 tantivy 文档）。

召回侧超取：存在 Glob 模式或 `directory_prefix` 时按 3 倍（至少 `+20`，上限 200）超取。策略内 `apply_bm25_strategy_filters()` 依次执行：

1. `GlobFilter`（include/exclude 模式）
2. `term_weight_threshold` 召回侧成本阀（原始词权重域，默认 0.0 关闭）
3. `directory_prefix_allows`：BM25 索引无前缀字段，目录前缀在查询时按规范化路径做斜杠边界前缀检查
4. 截断回 `base_limit`

### 4.3 raw_filter：调试逃生口

`SearchFilter.raw_filter` 是面向后端调试的原始过滤器逃生口。业务查询路径被显式拒绝：`DenseRetrieval::search()`（`retrieval/core/dense.rs`）开头调用 `filter.is_business_query()`，携带 `raw_filter` 的请求返回 `QueryError::InvalidQuery`。业务代码一律通过结构化字段构造过滤器。

---

## 5. Glob 过滤的位置与实现

**文件**：`crates/app/cce-orchestrator/src/query/retrieval/primitives/glob_filter.rs`

`GlobFilter` 使用 `globset::GlobSet` 把所有模式编译为单个 DFA，对结果 `file_path` 执行匹配：

| 条件 | 行为 |
|------|------|
| include 与 exclude 均为空 | 全部通过（快速路径） |
| include 非空 | 只保留匹配至少一个 include 模式的结果 |
| exclude 非空 | 移除匹配任意 exclude 模式的结果 |
| 两者均非空 | include 筛选后再排除 |

**管线位置**：Glob 过滤在检索策略内部执行（dense 的 `apply_strategy_filters`、bm25 的 `apply_bm25_strategy_filters`），配合超取回填被过滤掉的名额，发生在融合与 enrichment **之前**；`GlobFilter::apply` 的直接调用仅保留给绕过策略的调用方作为兜底。

---

## 6. SQLite enrichment

**文件**：`crates/app/cce-orchestrator/src/query/searcher/searcher_core.rs`

enrichment（内容、snippet、行号、实体映射回填）是融合之后、后处理之前的最后一个数据塑形步骤。融合与过滤只需索引携带字段（分数、entity/segment id、file_path），因此昂贵的 SQLite 内容查找只对存活候选集执行一次。hybrid 流程中：多实体展开 → 融合 → 归一化 → enrichment → 后处理（rerank、排序、阈值过滤、标注）。

---

## 7. 类别与内容类型

**文件**：`crates/app/cce-orchestrator/src/query/types/query_options.rs`

- `ExcludableContentType` 枚举为 `{ Test, Generated, Dependency }`，`matches_path()` 对规范化项目相对路径做模式匹配。
- `QueryOptions` 新增 `include_categories: Vec<FileCategory>` 与 `exclude_categories: Vec<FileCategory>`，与内容类型排除一起下推到向量 `SearchFilter` 与 BM25 `FulltextSearchOptions`。

### 覆盖矩阵（当前）

| 过滤字段 | Dense | BM25 | 生效位置 |
|---------|-------|------|---------|
| epoch 视图 / excluded_files | ✅ 下推 | ✅ 下推（索引内 BooleanQuery） | 存储层 |
| `directory_prefix` | ✅ 下推 | ✅ 查询时前缀检查（配合超取） | 策略/存储层 |
| `exclude_test/generated/dependency` | ✅ 下推 | ✅ 下推 | 存储层 |
| `include/exclude_categories` | ✅ 下推 | ✅ 下推 | 存储层 |
| include/exclude Glob 模式 | ✅ 策略内 GlobFilter + 超取 | ✅ 策略内 GlobFilter + 超取 | 策略层（融合前） |

---

## 8. 验证与错误处理

| 场景 | 处理 |
|------|------|
| Glob 编译失败 | `GlobFilter::apply` 返回 `QueryError::invalid()`；策略侧对错误模式 `unwrap_or_default()` 降级为空结果 |
| 业务路径携带 raw_filter | `DenseRetrieval::search` 直接拒绝（`QueryError::InvalidQuery`） |
| BM25 运行时失败（I/O、损坏 segment 等） | `classify_bm25_error` 归类为可重试 `QueryError::Retryable`；配置类错误（如 BM25 未启用）为 `Config` |
| SQLite 连接失败 | enrichment 记录告警，保留索引携带字段 |

---

## 9. 注意事项

1. **内容类型排除与 Glob 排除语义可叠加**：前者是索引标记的下推过滤，后者是策略层模式过滤，最终效果为所有排除规则的并集。
2. **include_patterns 与 directory_prefix 组合**：目录前缀先缩小候选域，Glob 在其结果上过滤，属预期的叠加。
3. **超取上限**：BM25 侧超取有 200 的硬上限，极端过滤条件下可能回填不足。
4. **路径大小写敏感性**：沿用文件系统语义（Linux 大小写敏感）；GlobSet 匹配对大小写敏感。

---

*相关文档：[hybrid-fusion-design.md](hybrid-fusion-design.md)、[tantivy-path-filter-capability.md](tantivy-path-filter-capability.md)*
