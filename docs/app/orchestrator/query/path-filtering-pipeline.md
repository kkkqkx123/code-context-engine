# 文件路径过滤流水线

## 1. 概述

本文档描述 `code-context-engine` 查询功能中的文件路径过滤机制。路径过滤分布在两个阶段：**检索阶段**（向量存储层过滤）和 **后处理阶段**（结果集层面的应用层过滤），形成双层过滤流水线。

### 核心目标

- 允许用户按目录和预定义内容类型（测试/生成/第三方）过滤搜索结果
- 支持灵活的 Glob 模式自定义包含/排除规则
- 对不同检索策略（向量、BM25、混合）提供一致的过滤行为

---

## 2. 请求路径总览

```
SearchRequest (HTTP API)
    │
    ▼
handle_search()                         ← src/api/handlers/search.rs
    │  • 验证 Glob 模式合法性 (validate_glob_patterns)
    │  • 解析 query_type → SearchSources
    │  • 映射 FilterOptions → QueryOptions
    ▼
Searcher::search()
    │
    ▼
execute_search_flow()
    │
    ├─ Step 1: Retrieval (检索) ─────────────────── 存储层过滤
    │   ├─ HybridFusion  → Qdrant 过滤器 (filter.must / must_not)
    │   ├─ VectorOnly    → Qdrant 过滤器
    │   └─ Bm25Only      → SQLite 回填 file_path
    │
    ├─ Step 2: BM25 Consensus Fusion
    ├─ Step 3: Summary Score Boost
    ├─ Step 4: Relation Score Boost
    │
    └─ Step 5: Post-processing ───────────────────── 应用层过滤
        ├─ 5.1 Reranking      (LLM reranker)
        ├─ 5.2 Score Sorting  (ScoreSorter)
        ├─ 5.3 Threshold      (ThresholdFilter)
        └─ 5.4 Glob Filter    (GlobFilter) ← include_patterns / exclude_patterns
```

---

## 3. 过滤字段总览

| 字段 | API 类型 | 作用范围 | 实现层 | 过滤方式 |
|------|---------|---------|--------|---------|
| `directory_prefix` | `Option<String>` | 向量搜索 | Qdrant `must[].wildcard` | 目录边界通配匹配（`{prefix}/*`） |
| `exclude_content_types` | `Vec<ExcludableContentType>` | 向量搜索 | Qdrant `must_not[].wildcard` | Wildcard 排除 |
| `include_patterns` | `Vec<String>` | 全部策略 | GlobFilter (globset) | Post-retrieval |
| `exclude_patterns` | `Vec<String>` | 全部策略 | GlobFilter (globset) | Post-retrieval |

### 3.1 字段间交互关系

```
                      QueryOptions
                     ┌───────────┐
                     │directory_prefix│───→ Qdrant must match (向量层)
                     │exclude_content │───→ Qdrant must_not   (向量层)
                     │include_patterns│───→ GlobFilter include (应用层)
                     │exclude_patterns│───→ GlobFilter exclude (应用层)
                     └───────────┘

 向量层过滤条件 = must(directory wildcard)
                 AND must_not(content_type_exclude)

 应用层过滤条件 = (include_patterns 为空 OR 匹配至少一个)
                 AND (exclude_patterns 为空 OR 不匹配任何模式)
```

---

## 4. 数据流分层详解

### 4.1 请求层 → 查询选项映射

**文件**: `src/api/handlers/search.rs` (L48-241)

`handle_search()` 将 `SearchRequest` 转换为 `QueryOptions`：

```rust
// 关键映射：
request.directory_prefix  → query_opts.with_directory_prefix(prefix)
request.exclude_patterns  → query_opts.with_exclude_patterns(patterns)
request.include_patterns  → query_opts.with_include_patterns(patterns)
request.exclude_content_types → query_opts.add_exclude_content_type(...)
```

输入验证（搜索前执行）：
- `validate_glob_patterns()` — 校验 Glob 语法

### 4.2 检索阶段过滤

#### 4.2.1 Hybrid 混合检索

**文件**: `src/orchestrator/query/retrieval/strategies/hybrid.rs`

混合检索构建完整的 Qdrant 过滤器，同时用于 Dense + Sparse 搜索：

```rust
// 1. 从 QueryOptions 构建 FilterOptions（统一了路径和内容类型过滤）
let filter_options = options.to_filter_options();

// 2. 将 FilterOptions 编译为 Qdrant JSON filter
let qdrant_filter = VectorRetrieval::build_search_filter_from_options(&filter_options);

// 3. 通过 raw_filter 传递给 QdrantRetrieval
let filter = qdrant_filter.map(|raw| SearchFilter {
    directory_prefix: options.directory_prefix.clone(),
    raw_filter: Some(raw),
});

// 4. Dense + Sparse 并行搜索使用同一过滤器
let (dense_results, sparse_results) = tokio::join!(
    qdrant.search_dense(DenseSearchQuery { vector, limit, filter: filter.clone() }),
    qdrant.search_sparse(SparseSearchQuery { sparse_vector, limit, filter }),
);
```

生成的 Qdrant 过滤器 JSON 结构示例（`directory_prefix` 经规范化后按目录边界匹配）：

```json
{
    "must": [
        {"key": "file_path", "wildcard": "src/main/*"}
    ],
    "must_not": [
        {"key": "file_path", "wildcard": "*test*"},
        {"key": "file_path", "wildcard": "*vendor/*"}
    ]
}
```

#### 4.2.2 纯向量检索 (VectorOnly)

通过 `Searcher::build_filter_options()` 构建 `FilterOptions`，然后调用 `VectorRetrieval::search()` → `build_search_filter_from_options()` 生成同样的 Qdrant 过滤器。

**注意**: 纯向量检索通过传统的 `VectorRetrieval` API（HTTP 直接调用 Qdrant），而 Hybrid 通过 `QdrantRetrieval`（trait 实现）。两者最终使用相同的 `build_search_filter_from_options()` 构建过滤器。

#### 4.2.3 BM25 纯文本检索

**文件**: `src/orchestrator/query/retrieval/strategies/bm25_only.rs`

BM25 索引将 `file_path` 视为遗留字段（不索引），路径数据来自 SQLite `ChunkRecord`：

```rust
// 1. 从 BM25 获取 chunk_ids
let chunk_ids: Vec<String> = bm25_results.iter().map(|r| r.document_id.clone()).collect();

// 2. EntityMapper 从 SQLite 批量查询
let chunk_records = EntityMapper::get_chunk_records(&conn, &chunk_ids, pid)?;

// 3. 构建 SearchResult 时，优先从 SQLite 获取 file_path
let file_path = chunk_records
    .get(&r.document_id)
    .map(|chunk| chunk.file_path.clone())
    .unwrap_or_else(|| r.fields.get("file_path").cloned().unwrap_or_default());
```

**关键**: BM25 检索**不由存储层过滤路径** — 所有过滤依赖于后处理阶段的 `GlobFilter`。

### 4.3 SQLite 回填文件路径

**文件**: `src/orchestrator/query/retrieval/strategies/entity_mapper.rs`

`EntityMapper::enrich_from_chunk()` 确保所有检索策略的结果最终都有正确的 `file_path`：

```rust
pub fn enrich_from_chunk(result: &mut SearchResult, chunk_records: &HashMap<String, ChunkRecord>) {
    if let Some(chunk) = chunk_records.get(&result.id) {
        result.snippet = Some(chunk.raw_code.clone());
        result.content = chunk.raw_code.clone();
        result.file_path = chunk.file_path.clone();   // ← 回填 file_path
        result.start_line = chunk.start_line as u32;
        result.end_line = chunk.end_line as u32;
        result.kind = chunk.chunk_type.clone();
        // ...
    }
}
```

该函数在以下检索策略的末尾被调用：
- **HybridRetrieval** — 对融合后结果逐条回填
- **Bm25OnlyRetrieval** — 在 `SearchResult` 构造时直接使用，并再次通过 enrichment 确保正确
- **VectorOnlyRetrieval** — 同样通过 enrichment 回填

### 4.4 后处理阶段 Glob 过滤

**文件**: `src/orchestrator/query/ranking/glob_filter.rs`

在后处理流水线的最后一步（Step 5.4），对所有检索策略的结果统一应用 Glob 过滤：

```rust
// Searcher::execute_search_flow() 中:
let glob_filtered_results = self
    .glob_filter
    .apply(final_results, &options.include_patterns, &options.exclude_patterns)?;
```

**GlobFilter 行为**：

| 条件 | 行为 |
|------|------|
| `include_patterns` 为空, `exclude_patterns` 为空 | 全部结果通过 |
| `include_patterns` 非空, `exclude_patterns` 为空 | 只保留匹配至少一个 include 模式的结果 |
| `include_patterns` 为空, `exclude_patterns` 非空 | 移除匹配任意 exclude 模式的结果 |
| 两者均非空 | include 筛选后移除 exclude 匹配项 |

**实现**: 使用 `globset::GlobSet` 编译所有模式为 DFA，对所有结果的 `file_path` 字段执行 `is_match()`。

**性能**: GlobMatch 复杂度为 O(n × m)，其中 n 为结果数，m 为模式数。GlobSet 内部将多模式编译为单个 DFA，单次匹配复杂度接近 O(len(path))。

---

## 5. Qdrant 过滤器构建

**文件**: `src/orchestrator/query/retrieval/vector.rs` — `build_search_filter_from_options()` (L269-428)

构建逻辑：

```
Algorithm build_qdrant_filter(options):
    must = []
    must_not = []

    // directory_prefix → must wildcard（目录边界匹配）
    // 前缀先经 normalize_project_path 规范化（折叠 .、解析 ..），
    // 再生成 {prefix}/* 通配符；空前缀退化为 *
    if options.directory_prefix:
        normalized = normalize(directory_prefix)
        must.push({key: "file_path", wildcard: "{normalized}/*"})

    // exclude_content_types → must_not wildcard
    for each exclude_type in options.exclude_content_types:
        match exclude_type:
            Test → add wildcard patterns for test files
            Generated → add wildcard patterns for generated files
            Vendor → add wildcard patterns for vendor files

    return {must, must_not} if non-empty
```

### `SearchFilter` 传递链：

```rust
// 存储层接口 (src/storage/vector_retrieval.rs)
pub struct SearchFilter {
    pub directory_prefix: Option<String>,
    pub raw_filter: Option<serde_json::Value>,  // 预构建的完整 Qdrant 过滤器 JSON
}

// QdrantRetrieval 使用 (src/storage/qdrant/retrieval.rs)
fn build_filter(filter: Option<&SearchFilter>) -> Option<serde_json::Value> {
    filter.and_then(|f| {
        if let Some(raw) = &f.raw_filter {
            return Some(raw.clone());  // 优先使用 raw_filter（包含完整过滤条件）
        }
        // 降级到 directory_prefix 的 wildcard 目录边界过滤
        f.directory_prefix.as_ref().map(|prefix| {
            json!({"must": [{"key": "file_path", "wildcard": "{normalized}/*"}]})
        })
    })
}
```

---

## 6. 检索策略覆盖矩阵

| 过滤字段 | VectorOnly | Hybrid | Bm25Only |
|---------|-----------|--------|----------|
| `directory_prefix` | ✅ Qdrant wildcard | ✅ Qdrant wildcard | 仅 Glob 过滤 |
| `exclude_content_types` | ✅ Qdrant must_not | ✅ Qdrant must_not | 仅 Glob 过滤 |
| `include_patterns` | ✅ GlobFilter | ✅ GlobFilter | ✅ GlobFilter |
| `exclude_patterns` | ✅ GlobFilter | ✅ GlobFilter | ✅ GlobFilter |

- ✅ **向量检索**（VectorOnly + Hybrid）在 Qdrant 层完成精确过滤，结果准确且高效
- ⚠️ **BM25Only** 的路径过滤完全依赖后处理的 GlobFilter，在数据量大时结果数可能远多于预期

---

## 7. 验证与错误处理

### 7.1 输入验证

| 验证 | 位置 | 检查内容 |
|------|------|---------|
| Glob 语法校验 | `search.rs` → `validate_glob_patterns()` | `*?[]{}!` 等特殊字符的合法性 |
| 查询非空 | `search.rs` | query 不可为空或空白 |
| Limit 上限 | `search.rs` | 最大 100 |

### 7.2 运行时错误处理

| 错误场景 | 处理方式 |
|---------|---------|
| Glob 编译失败 | 返回 `QueryError::invalid()`，包含具体模式错误信息 |
| SQLite 连接失败 | 记录 warning，回退到 BM25 fields 中的 file_path |
| Qdrant 查询失败 | 返回 `QueryError::Vector` 或 `QueryError::Bm25` |

---

## 8. 潜在问题与注意事项

### 8.1 双重路径过滤（向量检索）

对于向量检索，路径可能被过滤两次：
1. **Qdrant 层**：`directory_prefix` 和 `exclude_content_types` 通过 match/wildcard 过滤
2. **GlobFilter 层**：`include_patterns`/`exclude_patterns` 再次过滤

这可能导致 `include_patterns` 与 `directory_prefix` 组合使用时，`include_patterns` 的作用域被 `directory_prefix` 预先缩小。属于**预期的叠加效果**而非 Bug。

### 8.2 BM25 无存储层过滤

BM25 检索不应用任何 Qdrant 风格的过滤器。这意味着：
- 如果有大量文档的 BM25 分数超过 `bm25_min_score`，`GlobFilter` 可能丢弃大部分结果
- `directory_prefix` 对 BM25 检索仅为软约束（通过 GlobFilter）

### 8.3 双重排除机制

有两个途径可以实现排除：
- `exclude_content_types` — 预定义的硬编码 wildcard 模式（测试/生成/第三方）
- `exclude_patterns` — 用户自定义 Glob 模式

两者语义重叠但可独立共存，最终效果为所有排除规则的并集。

### 8.4 路径大小写敏感性

当前实现假定文件路径大小写敏感（Linux 默认行为）。在 Windows 平台，Qdrant 的 `match` 和 `wildcard` 操作默认大小写敏感，可能导致遗漏匹配。

### 8.5 directory_prefix 语义与限制

`directory_prefix` 在 Qdrant 中使用 wildcard 进行**目录边界匹配**（`{prefix}/*`）：

- 前缀会先经 `normalize_project_path` 规范化（`\`→`/`、折叠 `.`、解析 `..`）。
- 目录边界语义确保 `docs` 不会误匹配 `docs2/` 下的文件。
- **通配符限制**：Qdrant wildcard 不支持转义，前缀本身含 `*`/`?`/`[` 时会被当作通配符解释（如目录名含 `*` 会破坏过滤语义），属用户输入约束。
- **文件级匹配**：若需对单个文件或更复杂的路径模式过滤，请使用 `include_patterns`/`exclude_patterns`（GlobFilter 层）。
- 对极深路径（>5 层），需注意前缀范围可能过窄，从而导致结果为空。

---

## 9. 测试覆盖

### 单元测试

| 测试文件 | 测试项 | 数量 |
|---------|--------|------|
| `ranking/glob_filter.rs` | include/exclude 组合、空结果、无匹配 | 6 |
| `retrieval/strategies/mod.rs` | 过滤器构建 | 2+ |
| `qdrant/retrieval.rs` | SearchFilter 构建 | 2 |
| `types/query_options.rs` | 序列化、构建器 | 8+ |

### 关键测试场景

```
include_patterns=["src/**"] → 只保留 src/ 下的结果
exclude_patterns=["tests/**"] → 排除 tests/ 下的结果
include=["**/*.rs"] + exclude=["**/lib.rs"] → 所有 .rs 文件排除 lib.rs
include 为空 + exclude 为空 → 全部通过
include=["*.py"] + 结果都是 .rs → 空结果
空结果列表 → 空结果
```

---

## 10. 架构图

```
┌───────────────┐
│  SearchRequest │  directory_prefix, exclude_content_types,
│  (API)         │  include_patterns, exclude_patterns
└───────┬───────┘
        │
        ▼
┌───────────────────────────────────────┐
│  handle_search()                      │
│  • 验证 Glob 语法                     │
│  • 映射到 QueryOptions                │
└───────────────┬───────────────────────┘
                │
                ▼
┌──────────────────────────────────────────────────────────────┐
│  execute_search_flow()                                       │
│                                                              │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐  │
│  │  HybridFusion   │  │  VectorOnly     │  │  Bm25Only   │  │
│  │  ↓              │  │  ↓              │  │  ↓          │  │
│  │ Qdrant filter   │  │ Qdrant filter   │  │ SQLite      │  │
│  │ (must/must_not) │  │ (must/must_not) │  │ file_path   │  │
│  └────────┬────────┘  └────────┬────────┘  └──────┬──────┘  │
│           │                    │                   │         │
│           └────────┬───────────┴───────────────────┘         │
│                    │                                          │
│                    ▼                                          │
│           ┌──────────────────┐                               │
│           │  EntityMapper    │ ← SQLite ChunkRecord 回填     │
│           │  enrich_from_    │    (snippet, content,         │
│           │  chunk()         │     file_path, line numbers)  │
│           └────────┬─────────┘                               │
│                    │                                          │
│                    ▼                                          │
│           ┌──────────────────┐                               │
│           │  Post-processing │                               │
│           │  5.1 Rerank      │                               │
│           │  5.2 Sort        │                               │
│           │  5.3 Threshold   │                               │
│           │  5.4 GlobFilter  │ ← include/exclude patterns    │
│           └────────┬─────────┘                               │
│                    │                                          │
│                    ▼                                          │
│           ┌──────────────────┐                               │
│           │  SearchResult    │                               │
│           └──────────────────┘                               │
└──────────────────────────────────────────────────────────────┘
```

---

*文档日期: 2025-07*  
*最后更新: 2025-07*  
*相关分析: [docs/analysis/file-path-filtering-analysis.md](../../analysis/file-path-filtering-analysis.md)*
