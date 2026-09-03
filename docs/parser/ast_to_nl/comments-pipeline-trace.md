# 注释处理流水线追踪

本文档追踪注释从源代码到最终导出输出的完整流动路径，记录每个阶段的筛选、转换和丢失逻辑。

## 概述

注释在系统中经历五阶段流水线：

```
源文件 → [Parser] 提取与关联 → [Grouper] 保留传递 → [AST→NL] 模板生成 → [Chunker] 分块 → [Exporter] 格式化输出
```

每个阶段对注释的处理策略不同，最终输出内容取决于各阶段累积作用的结果。

---

## 阶段一：Parser — 提取与关联

### 1.1 tree-sitter 查询捕获

**文件**: `crates/cce_parser/src/tree_sitter_query/scheme/rust.rs`（及其他语言）

每种语言定义注释捕获规则，以 Rust 为例：

```scheme
(line_comment) @comment.line    ; // 普通注释 → 标记为 comment.line
(doc_comment) @comment.doc      ; /// 或 //! → 标记为 comment.doc
```

| 语言           | 捕获的 comment.line        | 捕获的 comment.doc            |
| -------------- | -------------------------- | ----------------------------- |
| Rust           | `//`                       | `///`, `//!`                  |
| C/C++/Java     | `//`                       | `/* */`, `/** */`             |
| Python         | 无（无 line_comment 节点） | `"""..."""`（string_content） |
| JavaScript/TSX | `(comment)`（统一的）      | `(html_comment)`（JSX）       |

### 1.2 should_keep_comment 过滤

**文件**: `crates/cce_parser/src/parser/comment_processor.rs:239-268`

```rust
fn should_keep_comment(capture: &Capture) -> bool {
    let name = &capture.name;
    if name == "comment.line" { return false; }           // ← 普通 // 注释被丢弃
    if name.contains(".doc")   { return true; }           // ← 文档注释保留
    if name.contains(".block") { return true; }           // ← 块注释保留
    if name == "comment"       { return !text.starts_with("//"); } // ← 通用注释按内容判断
    false
}
```

**过滤结果**：

| 注释类型       | 示例                | tree-sitter 标记 | 是否保留 |
| -------------- | ------------------- | ---------------- | -------- |
| 普通行注释     | `// some note`      | `comment.line`   | ❌ 丢弃  |
| 文档注释（外） | `/// documentation` | `comment.doc`    | ✅ 保留  |
| 文档注释（内） | `//! module doc`    | `comment.doc`    | ✅ 保留  |
| 块注释         | `/* block */`       | `comment.block`  | ✅ 保留  |
| Javadoc/KDoc   | `/** doc */`        | `comment.doc`    | ✅ 保留  |

### 1.3 associate_comments 关联

**文件**: `crates/cce_parser/src/parser/comment_processor.rs:105-148`

通过字节偏移将保留的注释关联到实体：

```
算法：
  对于每个注释（按 start_byte 排序）：
    如果是内联文档（//!, #!）：
      ← 反向查找包含该注释的容器实体
    否则（///, /** */, /* */）：
      → 正向扫描，关联到下一个紧随的实体
```

**注意**：实体必须有正确的 `span.start_byte` 才能被正确关联。修饰符（如 `unsafe`）会影响 tree-sitter 节点边界，可能导致：

- 文档注释偏移 → 关联到前一个实体（漂移）
- 实体节点未包含正确的起始字节 → 关联失败

### 1.4 文件级文档注释提取

在关联实体注释之前，先提取文件级文档注释：

```rust
let file_doc = comments.iter()
    .find(|c| first_entity_start.map(|s| c.span.end_byte <= s).unwrap_or(true))
    .map(|c| clean_doc_comment(&c.text));
```

文件级注释（在第一个实体之前结束的文档注释）被提取后，从关联列表中移除。

### 1.5 clean_doc_comment

**文件**: `crates/cce_parser/src/parser/comment_processor.rs:267-320`

去除注释标记（`///`, `//!`, `/**`, `*/`, `#` 等），返回纯文本。

---

## 阶段二：Grouper — 保留传递

### 2.1 Entity → GroupedEntity

**文件**: `crates/cce_core/src/types/entity/grouped.rs:71-102`

```rust
pub fn from_entity(entity: &Entity) -> Self {
    Self {
        // ...
        doc_comment: entity.doc_comment.clone(),  // ← 直接克隆，完整保留
        // ...
    }
}
```

`GroupedEntity` 保留 `doc_comment: Option<String>`，不做任何处理。

### 2.2 EntityGroup 结构

```rust
pub struct EntityGroup {
    pub header: Option<GroupedEntity>,    // 组标题（如类定义），携带 doc_comment
    pub members: SmallVec<[GroupedEntity; 4]>,  // 组成员（如方法），各自携带 doc_comment
    // ...
}
```

---

## 阶段三：AST→NL 模板生成（关键分岔点）

### 3.1 分派机制

```
EntityGroup → GroupTemplateDispatcher.dispatch()
    ├── PatternInfo 匹配 → 模式模板（Builder/Factory/DTO 等）
    ├── StdlibCategory 匹配 → 标准库模板
    └── 默认 → RegularGroupTemplate（常规模板）
```

### 3.2 Embedding 路径处理

**模板头部描述** — `crates/cce_parser/src/ast_to_nl/embedding/templates/regular.rs:62-64`：

```rust
fn generate_group_description(&self, group: &EntityGroup) -> String {
    if let Some(header) = &group.header {
        if let Some(doc) = Self::clean_doc_comment(header.doc_comment.as_deref()) {
            return Self::append_hints(doc, group);  // ← 完整返回文档注释
        }
    }
    // ... 回退：用规范化名称生成语义描述
}
```

**成员描述** — `regular.rs:155-157`：

```rust
fn generate_member_description(&self, member: &GroupedEntity, group: &EntityGroup) -> String {
    if let Some(doc) = Self::clean_doc_comment(member.doc_comment.as_deref()) {
        return doc;  // ← 完整返回文档注释
    }
    // ... 回退：用规范化名称 + 参数 + 返回类型生成语义描述
}
```

**行为**：文档注释完整保留，作为描述的优先选择。

### 3.3 BM25 路径处理

**常规模板** — `crates/cce_parser/src/ast_to_nl/bm25/templates/regular.rs:214-225`：

```rust
fn push_entity_features(all_parts: &mut Vec<String>, entity: &GroupedEntity) {
    // ... 实体名、参数、返回类型 ...
    if let Some(ref doc) = entity.doc_comment {
        let clean_doc = Self::clean_doc_comment(doc);
        if !clean_doc.is_empty() {
            // 仅提取关键词，不包含完整文本！
            all_parts.extend(helpers::extract_keywords(&clean_doc));
        }
    }
    // ...
}
```

**行为**：文档注释被 `helpers::extract_keywords()` 拆分为独立关键词，融入整体文本。完整句子结构和语义上下文丢失。

**其他模式模板**（如 Builder/Factory/DTO）：行为与常规模板一致，均通过 `push_entity_features` 或等效函数，仅提取关键词。

### 3.4 clean_doc_comment（模板层）

**文件**：

- `crates/cce_parser/src/ast_to_nl/bm25/templates/regular.rs` — BM25 模板
- `crates/cce_parser/src/ast_to_nl/embedding/templates/regular.rs` — Embedding 模板

两者实现相同逻辑：

```rust
fn clean_doc_comment(doc: &str) -> String {
    doc.lines()
        .map(|line| {
            line.trim()
                .trim_start_matches("///").trim_start_matches("//!")
                .trim_start_matches("//").trim_start_matches("/**")
                .trim_start_matches("/*").trim_start_matches('*')
                .trim_end_matches("*/").trim()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
```

**注意**：这是模板层的第二阶段清理，与 Parser 层的 `clean_doc_comment` 功能重复。实体到达模板时 `doc_comment` 已经过 Parser 层的清理。

### 3.5 两种路径对比

| 维度                   | Embedding 路径                                   | BM25 路径                                                                 |
| ---------------------- | ------------------------------------------------ | ------------------------------------------------------------------------- |
| 最终用途               | 语义搜索、文档显示                               | 关键词检索索引                                                            |
| 文档注释处理           | 完整保留作为描述                                 | 仅提取关键词                                                              |
| 代码符号               | 移除（自然语言化）                               | 保留（实体名、参数、类型）                                                |
| 输出示例               | `"initialized atomic bool"`                      | `"initialized atomic bool"`                                               |
| 输出示例（带文档注释） | `"Get the reference to the underlying value..."` | `"get_unchecked get reference underlying value without checking if cell"` |

---

## 阶段四：Chunker — 分块传递

**文件**: `crates/cce_parser/src/ast_to_nl/chunker/`

Chunker 将模板生成的文本按令牌预算分块，不修改文本内容。`ChunkedResult` 包含：

```rust
pub struct ChunkedResult {
    pub text: String,               // 模板生成的文本（BM25 或 Embedding）
    pub path: ChunkPath,            // 标识 BM25 或 Embedding
    pub metadata: ChunkMetadata,    // 包含 source_span, file_path, code_metadata 等
    pub bm25_title: Option<String>, // BM25 标题
    pub source_group_id: String,    // 源分组 ID
    pub group_type: GroupType,
    pub token_count: usize,
}
```

注释内容已在模板阶段定型，Chunker 不涉及注释的进一步处理。

---

## 阶段五：Exporter — 格式化输出

### 5.1 Embedding/NL 导出

**聚合器** — `crates/cce_orchestrator/src/export/aggregator.rs:290-306`：

```rust
fn build_entity_doc(&self, _group_id: String, chunks: Vec<&ChunkedResult>) -> EntityNlDocument {
    let nl_description = chunks.iter()
        .filter(|c| c.path == ChunkPath::Embedding)  // ← 仅选取 Embedding 路径文本
        .map(|c| c.text.as_str())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    // ...
}
```

**格式化器** — `crates/cce_orchestrator/src/export/formatter.rs:131-149`：

- 文件头：写入 `summary.file_doc_comment`
- 实体：写入 `entity.nl_description`（已包含文档注释或语义描述）
- 不支持独立的 `doc_comment` 字段

### 5.2 BM25 导出

**文件**: `crates/cce_e2e_tests/tests/e2e/verify/common/bm25_exporter.rs`

- 写入 `ChunkPath::Bm25` 数据块的 `chunk.text`
- 按实体分组，使用 ````text` 代码块格式展示
- 包含 `keywords` 元数据行

---

## 丢失场景汇总

### 场景 1：普通 `//` 注释

**位置**: `comment_processor.rs:246-248`

```
// some note → tree-sitter: (line_comment) → @comment.line → should_keep_comment returns false
```

**结果**：永远不会进入 `Entity.doc_comment`。

### 场景 2：文档注释关联失败

**位置**: `comment_processor.rs:105-148`

可能性：

- **字节偏移不匹配**：实体 `span.start_byte` 与注释 `span.end_byte` 偏差（如 `unsafe fn` 等修饰符改变节点偏移）
- **关联被抢占**：先被其他实体匹配，但 `if entity.doc_comment.is_some()` 跳过已有关联的实体，导致剩余实体无法获得文档注释
- **文件级注释误删除**：`first_entity_start` 判定错误，导致原本应关联到第一个实体的文档注释被当作文件级注释提取

### 场景 3：BM25 模板丢弃完整文本

**位置**: `regular.rs:214-225`

```
doc_comment "Get the reference..." → clean_doc_comment → extract_keywords → 仅保留关键词
```

有意识的设计选择，非 bug。

### 场景 4：导出仅使用 Embedding 文本

**位置**: `aggregator.rs:290-306`

```
BM25 路径文本即使包含文档注释内容，也不会进入 EntityNlDocument.nl_description
```

对于 NL/Embedding 导出，这是正确的设计。

---

## 数据流图

```
源代码
  │
  ▼
tree-sitter 解析
  │
  ├── (line_comment)  → @comment.line         → should_keep_comment → ❌ 丢弃
  │
  ├── (doc_comment)   → @comment.doc          → should_keep_comment → ✅ 保留
  │     │
  │     ▼
  │   clean_doc_comment（Parser 层）
  │     │
  │     ▼
  │   associate_comments → Entity.doc_comment
  │     │                              │
  │     │ 位于第一个实体之前?          │ 否则
  │     ▼                              ▼
  │   file_doc_comment               Entity.doc_comment
  │                                    │
  │                                    ▼
  │                                  GroupedEntity.doc_comment
  │                                    │
  │                    ┌───────────────┴───────────────┐
  │                    ▼                               ▼
  │             Embedding 模板                  BM25 模板
  │                    │                               │
  │                    ▼                               ▼
  │         完整保留为描述                   仅提取关键词混入文本
  │                    │                               │
  │                    ▼                               ▼
  │           ChunkedResult (Embedding)      ChunkedResult (BM25)
  │                    │                               │
  │                    ▼                               ▼
  │           FileAggregator                   Bm25Exporter
  │           (仅取 Embedding)                  (直接写入)
  │                    │                               │
  │                    ▼                               ▼
  │           .cce/nl_docs/*.md               .cce/bm25_docs/*-bm25.md
  │           含完整文档注释                   含关键词碎片
  │
  └── (block_comment) → @comment.block       → should_keep_comment → ✅ 保留（同 doc）
```

## 关键文件索引

| 功能                        | 文件                                                             | 行号（关键逻辑） |
| --------------------------- | ---------------------------------------------------------------- | ---------------- |
| tree-sitter 查询（Rust）    | `crates/cce_parser/src/tree_sitter_query/scheme/rust.rs`         | 255-265          |
| 注释提取与关联              | `crates/cce_parser/src/parser/comment_processor.rs`              | 52-148, 239-268  |
| clean_doc_comment（Parser） | `crates/cce_parser/src/parser/comment_processor.rs`              | 267-320          |
| Entity → GroupedEntity      | `crates/cce_core/src/types/entity/grouped.rs`                    | 71-102           |
| BM25 模板（文档→关键词）    | `crates/cce_parser/src/ast_to_nl/bm25/templates/regular.rs`      | 214-225          |
| Embedding 模板（完整保留）  | `crates/cce_parser/src/ast_to_nl/embedding/templates/regular.rs` | 62-64, 155-157   |
| Converter 分派              | `crates/cce_parser/src/ast_to_nl/converter/group_converter.rs`   | 191-280          |
| FileAggregator              | `crates/cce_orchestrator/src/export/aggregator.rs`               | 290-306          |
| MarkdownFormatter           | `crates/cce_orchestrator/src/export/formatter.rs`                | 131-149          |
| BM25 Exporter（测试用）     | `crates/cce_e2e_tests/tests/e2e/verify/common/bm25_exporter.rs`  | 全文件           |
