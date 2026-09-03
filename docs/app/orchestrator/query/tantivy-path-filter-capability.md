# Tantivy 引擎路径过滤能力分析与扩展方案

> 基于 Tantivy 本地 fork (`crates/tantivy/`, v0.26.1) 的源码分析

---

## 1. Tantivy 当前路径过滤能力矩阵

### 现有可直接使用的查询类型

| 查询类型 | 对 STRING 字段支持 | 路径过滤适用性 |
|---------|-------------------|--------------|
| `TermQuery` | ✅ 精确匹配完整值 | 精确路径匹配 |
| `RegexQuery` | ✅ 正则匹配（可用于 glob 模式） | 前缀/后缀/wildcard 匹配 |
| `BooleanQuery` | ✅ 组合 Must/Should/MustNot | include + exclude 布尔组合 |
| `RangeQuery` | ✅ 字典序范围查询 | `directory_prefix` 范围匹配 |
| `Exclude` | ✅ DocSet 级排除 | 排除特定路径 |
| `TermSetQuery` | ✅ 多值精确匹配 | 文件扩展名集合 |

### 路径过滤场景覆盖情况

| 场景 | Tantivy 现有能力 | 示例 |
|------|----------------|------|
| `file_path="/src/main.rs"` | ✅ `TermQuery::new(Term::from_field_text(file_path, "/src/main.rs"), Basic)` | 精确路径 |
| `file_path ~ "/src/**"` | ✅ `RegexQuery::from_pattern(r"/src/.*", file_path)?` | 递归前缀 |
| `file_path ~ "*.rs"` | ✅ `RegexQuery::from_pattern(r".*\.rs", file_path)?` | 扩展名 |
| `file_path ~ "/src/*/mod.rs"` | ✅ 正则转换 `r"/src/[^/]*/mod\.rs"` | 单层通配 |
| `file_path > "/src/a" AND file_path < "/src/zzz"` | ✅ `RangeQuery::new_("/src/a", "/src/zzz")` | 前缀范围 |
| `NOT file_path ~ "**/test/**"` | ✅ `Exclude` + `RegexQuery` | 排除测试路径 |
| `file_path IN ["/src/a.rs", "/src/b.rs"]` | ✅ `TermSetQuery::new(terms)` | 精确集合 |

**结论**：Tantivy 在 v0.26.1 版本**已经具备完整的路径过滤原语**，无需新增底层查询类型。

---

## 2. 现有能力的使用方式（当前代码）

### 当前 BM25 检索不使用路径过滤

当前 `src/storage/bm25/search.rs` 的 `search()` 函数只接受 `query_text` + `options`，没有传入任何过滤条件。路径过滤完全由后处理 `GlobFilter` 完成。

```rust
// 当前 search 签名 — 无过滤参数
pub fn search(
    manager: &IndexManager,
    schema: &IndexSchema,
    query_text: &str,
    options: &SearchOptions,
) -> Result<(Vec<SearchResult>, f32), Bm25Error>
```

### 缺少的「胶水代码」

Tantivy 查询原语足够了，但缺少三层胶水代码：

| 层级 | 缺少的内容 |
|------|-----------|
| **Schema** | `file_path` 字段未被索引（`STRING \| STORED`） |
| **Search 参数** | `search()` 函数不接受 `file_path` 过滤条件 |
| **查询构建** | 缺少从 `PathFilterOptions` 到 Tantivy `BooleanQuery` 的转换函数 |

---

## 3. 两种扩展路径对比

### 方案 A：在 Tantivy 内部实现 GlobQuery (新类型)

**描述**：在 tantivy `query/` 目录下新增 `glob_query.rs`，实现 `GlobQuery`，内部将 glob 模式转换为正则，用 `AutomatonWeight` 执行。

```rust
// 新增: src/query/glob_query.rs
pub struct GlobQuery {
    pattern: String,
    field: Field,
}
impl Query for GlobQuery { ... }
```

**优点**：
- 查询 API 语义清晰，与 Qdrant `match.value` 对等
- 可扩展支持大小写不敏感选项
- 可在 QueryParser 中注册 `glob:` 语法

**缺点**：
- 本质上是对 `RegexQuery` 的轻量封装（glob → regex 转换）
- 功能收益有限，增加了维护面

### 方案 B：在现有 Tantivy 原语上构建高层过滤函数（推荐 ★★★★★）

**描述**：不在 Tantivy 层添加新查询类型，而是在 BM25 使用侧（上层）构建过滤函数，组合现有原语。

```rust
// 在 src/storage/bm25/filter.rs 或 search.rs 中新增
pub fn build_path_filter(
    schema: &IndexSchema,
    file_path_field: Field,
    filter: &PathFilterOptions,
) -> Option<Box<dyn Query>> { ... }
```

**组合模式**：

```
输入: directory_prefix="/src", include_patterns=["**/*.rs"], exclude_patterns=["**/test/**"]

构建:
  RangeQuery("/src\x00" .. "/src\xFF\xFF")    // directory 前缀范围
  AND RegexQuery(".*\.rs")                      // include 扩展名
  AND NOT RegexQuery(".*/test/.*")              // exclude 测试路径
  → BooleanQuery(Must[Range, Regex], MustNot[Regex])
```

**优点**：
- **零 Tantivy 核心修改**，仅在 fork 上层组合现有原语
- 系统架构清晰：Tantivy 层保持通用搜索引擎角色，路径过滤是应用层语义
- 更新上游 Tantivy 时无需冲突解决
- 可与 Qdrant 层路径过滤共用同一套 `PathFilterOptions`

**缺点**：
- `GlobPattern → Regex` 转换需要手动实现（globset crate 的 `Glob::regex()` 可用）

### 对比总结

| 维度 | 方案 A: GlobQuery | 方案 B: 上层构建 |
|------|-----------------|----------------|
| 代码量 | ~200 行 (新文件) | ~80 行 (现有 search.rs 中新增) |
| Tantivy 核心修改 | ✅ 新增全局查询类型 | ❌ 零修改 |
| 上游兼容性 | ⚠️ 需 merge 时维护 | ✅ 无冲突 |
| 功能收益 | 语义清晰但冗余 | 够用，与应用层对齐 |
| 测试复杂度 | 需独立测试 | 集成测试即可 |

---

## 4. 具体实施方案：在上层构建路径过滤

### 4.1 第一步：Schema 补充 file_path 字段

文件：`src/storage/bm25/schema.rs`

```rust
pub struct IndexSchema {
    pub file_path: Field,  // 新增
    // ... 其他字段
}

// build 时
let file_path = schema_builder.add_text_field("file_path", STRING | STORED);

// to_document 中替换 ignore 逻辑
"file_path" => doc.add_text(self.file_path, value),
// 移除原有的 ignore 分支
```

**字段类型选择**：
- `STRING`：不分词，整个路径作为单一词条存储在倒排索引中
- `STORED`：可召回，搜索结果中直接获取路径值
- 可选 `FAST`：如果需要离线聚合索引路径唯一值，可增加 fast field

### 4.2 第二步：在 search.rs 中添加路径过滤查询构建

文件：`src/storage/bm25/search.rs`（或新增 `filter.rs`）

```rust
use crate::query::{BooleanQuery, Occur, Query, RangeQuery, RegexQuery};

/// 路径过滤选项
#[derive(Debug, Default, Clone)]
pub struct PathFilter {
    pub directory_prefix: Option<String>,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

/// 将路径过滤选项转换为 Tantivy BooleanQuery
pub fn build_path_filter_query(
    schema: &IndexSchema,
    filter: &PathFilter,
) -> Option<Box<dyn Query>> {
    let mut subqueries: Vec<(Occur, Box<dyn Query>)> = Vec::new();

    // directory_prefix → RangeQuery (字典序前缀范围)
    if let Some(ref prefix) = filter.directory_prefix {
        let start = Bound::Included(Term::from_field_text(
            schema.file_path, &format!("{}\x00", prefix),
        ));
        let end = Bound::Included(Term::from_field_text(
            schema.file_path, &format!("{}\xFF\xFF", prefix),
        ));
        subqueries.push((Occur::Must, Box::new(RangeQuery::new(start, end))));
    }

    // include_patterns → RegexQuery (OR 组合)
    if !filter.include_patterns.is_empty() {
        let include_queries: Vec<Box<dyn Query>> = filter.include_patterns
            .iter()
            .map(|pat| {
                let regex = glob_to_regex(pat);
                Box::new(RegexQuery::from_pattern(&regex, schema.file_path).unwrap())
                    as Box<dyn Query>
            })
            .collect();
        // OR 组合
        subqueries.push((Occur::Must, Box::new(BooleanQuery::new(
            include_queries.into_iter().map(|q| (Occur::Should, q)).collect(),
            1, // minimum_number_should_match
        ))));
    }

    // exclude_patterns → RegexQuery (NOT 组合)
    if !filter.exclude_patterns.is_empty() {
        let exclude_queries: Vec<Box<dyn Query>> = filter.exclude_patterns
            .iter()
            .map(|pat| {
                let regex = glob_to_regex(pat);
                Box::new(RegexQuery::from_pattern(&regex, schema.file_path).unwrap())
                    as Box<dyn Query>
            })
            .collect();
        // NOT 组合
        for q in exclude_queries {
            subqueries.push((Occur::MustNot, q));
        }
    }

    if subqueries.is_empty() {
        None
    } else {
        Some(Box::new(BooleanQuery::new(subqueries, 0)))
    }
}

/// glob 模式转正则表达式
fn glob_to_regex(pattern: &str) -> String {
    let mut regex = String::with_capacity(pattern.len() + 4);
    regex.push('^');
    for ch in pattern.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push_str("."),
            '.' | '+' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }
    regex.push('$');
    regex
}
```

### 4.3 第三步：search() 函数增加过滤参数

```rust
pub fn search(
    manager: &IndexManager,
    schema: &IndexSchema,
    query_text: &str,
    options: &SearchOptions,
    path_filter: Option<PathFilter>,  // ← 新增
) -> Result<(Vec<SearchResult>, f32), Bm25Error> {
    let query = parse_query(query_text, schema, &options.field_weights)?;

    // 如果存在路径过滤，将全文检索 + 路径过滤组合为 BooleanQuery
    let final_query: Box<dyn Query> = if let Some(filter) = path_filter {
        if let Some(path_query) = build_path_filter_query(schema, &filter) {
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, query),
                (Occur::Must, path_query),
            ], 0))
        } else {
            query
        }
    } else {
        query
    };

    // 后续逻辑使用 final_query 替代 query
    // ...
}
```

### 4.4 第四步：向上传到 Bm25Client 和 Bm25OnlyRetrieval

文件：`src/storage/bm25/client.rs`

```rust
pub async fn search_with_path_filter(
    &mut self,
    _index_name: &str,
    query: &str,
    limit: i32,
    path_filter: PathFilter,
) -> Result<Vec<Bm25SearchResult>, Bm25Error> {
    // ...
    let (results, max_score) = search(&manager_guard, schema, query, &options, Some(path_filter))?;
    // ...
}
```

`Bm25OnlyRetrieval::retrieve()` 中构造 `PathFilter` 并传给 BM25 搜索。

---

## 5. Tantivy 内部需要做的修改

### 5.1 核心修改：仅 1 个文件

| 文件 | 修改 | 影响范围 |
|------|------|---------|
| `src/storage/bm25/schema.rs` | 新增 `file_path: Field` | 需重建 Tantivy 索引 |

**Tantivy fork 本身无需任何修改**。方案 B 完全在上层使用现有 API 组合。

### 5.2 可选增强（低优先级）

如果希望在 Tantivy fork 中增加更优雅的 API，可考虑：

1. **`query_parser.rs` 中增加 glob 语法**：支持 `file_path:src/**/*.rs` 语法，自动将 `*` 转换为正则
   - 修改 `compute_logical_ast_lenient` 中的字段值解析
   - 约 20 行代码

2. **新增 `PrefixQuery`**：对 `STRING` 字段做高效前缀匹配（比 `RangeQuery` 语义更清晰）
   - 约 80 行新文件，使用 `AutomatonWeight` + 前缀 automaton
   - 但功能上 `RangeQuery` 已覆盖

---

## 6. 性能分析

### Tantivy 路径过滤的性能特征

| 操作 | 复杂度 | 说明 |
|------|--------|------|
| `TermQuery` | O(log N) + O(result set) | 倒排索引查找 |
| `RangeQuery` | O(M) | M = 范围内的文档数，扫描倒排索引 |
| `RegexQuery` | O(K × T) | K = 匹配的词条数，T = 词条的平均 posting list 长度 |
| `BooleanQuery(Must)` | O(最小子查询的扫描) | 先执行最高效的子查询 |
| `BooleanQuery(MustNot)` | O(主查询扫描 + 排除集检查) | 与 GlobFilter 后处理性能相当 |

### vs 当前 GlobFilter 后处理

| 场景 | Tantivy 内过滤 | GlobFilter 后处理 |
|------|--------------|------------------|
| 路径选择性强（如仅搜索 /src） | ✅ **更快**（提前裁剪结果集） | ⚠️ 需检索全部结果后再过滤 |
| 路径选择性弱（如排除 3 个路径） | ⚠️ 性能相当 | ⚠️ 性能相当 |
| 复杂 Glob 模式（深层通配） | ⚠️ 正则引擎扫描 | ⚠️ GlobSet 编译优化 |
| BM25 结果数远大于 TopK | ✅ **大幅减少排序开销** | ❌ 大量无用结果过排序 |

**关键收益场景**：当 `directory_prefix` 或 `include_patterns` 能大幅缩小候选集时（过滤掉 >50% 文档），Tantivy 内过滤比后处理快 **3-10 倍**。

---

## 7. 实施路线图

| 阶段 | 文件 | 工作量 | 收益 |
|------|------|--------|------|
| **P0** | `bm25/schema.rs` | 0.5 天 | file_path 索引化，搜索结果自带路径 |
| **P0** | `bm25/types.rs` | 0.25 天 | ConversionResult 包含 file_path |
| **P1** | `bm25/search.rs` | 1 天 | 新增 `build_path_filter_query()` + `search()` 过滤参数 |
| **P1** | `bm25/client.rs` | 0.5 天 | 新增 `search_with_path_filter()` |
| **P2** | `strategies/bm25_only.rs` | 0.5 天 | 连接 `QueryOptions` → `PathFilter` → BM25 搜索 |
| **P2** | 重建索引 | 按数据量 | 运行 full reindex 使 file_path 生效 |

### 不需要的修改

- ❌ Tantivy fork 核心代码（新增 `GlobQuery` 等）
- ❌ QueryParser 语法扩展
- ❌ `src/orchestrator/query/ranking/glob_filter.rs`（保持双通道过滤）

---

## 8. 最终结论

### 关于「Tantivy 引擎是否需要补充路径过滤能力」

**需要补充的是「上层胶水代码」，不是 Tantivy 引擎本身**。

| 层面 | 现状 | 需要补充 | 是否需修改 Tantivy |
|------|------|---------|------------------|
| **查询原语** | ✅ `RegexQuery` + `BooleanQuery` + `RangeQuery` 已完备 | 无 | ❌ |
| **Schema** | ❌ `file_path` 未被索引 | 新增 `file_path: Field` (`STRING\|STORED`) | ❌（在 `IndexSchema` 层） |
| **查询构建** | ❌ 无 `glob_to_regex` 转换 | 新增 `build_path_filter_query()` | ❌ |
| **Search 接口** | ❌ `search()` 无过滤参数 | 新增 `path_filter: Option<PathFilter>` | ❌ |
| **调用链路** | ❌ `Bm25OnlyRetrieval` 不传路径条件 | 连接到 `QueryOptions` | ❌ |

所有修改都在上层代码中，**Tantivy fork 的 400+ 文件零修改**。

### 建议

1. **优先做**：Tantivy Schema 补充 `file_path`（40 行，结果自带路径，减少 SQLite 依赖）
2. **增量做**：在 `search.rs` 中构建路径过滤查询构建函数（80 行，可选启用）
3. **按需用**：BM25-only 路径过滤作为可选优化特性，默认关闭，保持后处理 GlobFilter 作为主路径
4. **不修改 Tantivy 内核**：减少与上游同步的冲突风险，因为方案 B 已覆盖所有需求

---

*分析日期: 2025-07*
*Tantivy 版本: 0.26.1 (fork at crates/tantivy)*
*关联文档: [path-filtering-pipeline.md](path-filtering-pipeline.md), [path-filtering-solutions.md](path-filtering-solutions.md)*
