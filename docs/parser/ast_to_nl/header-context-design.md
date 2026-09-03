# Header 两级上下文机制

## 概述

当一个实体组（如包含多个方法的 `InherentImpl`）被切分为多个 chunk 时，每个 chunk 需要保留组级别上下文信息，以便嵌入查询时能理解 chunk 的来源和语义范围。

Header 机制提供两级上下文：

| 级别 | 用途 | 内容 | 示例 |
|------|------|------|------|
| **Full Header** | 首个 chunk | 组描述 + 全部成员签名、返回值、安全说明 | `once cell inherent_impl. Implements methods for OnceCell of T. public constant new function. Returns OnceCell of T. public constant with_value function...` |
| **Brief Header** | 后续 chunk | 组名称/类型 + 成员名称列表 | `once cell inherent_impl. Implements methods for OnceCell of T. Methods: new, with_value, is_initialized, initialize.` |

## 数据流

```
EntityGroup
    │
    ▼
convert_group_with_generators()           [converter/group_converter.rs]
    │
    ├─ header_conversion (ConversionResult)
    │    ├─ bm25_text: 完整 BM25 header
    │    ├─ embedding_text: 完整 Embedding header
    │    ├─ bm25_brief_header: 简要 BM25 header
    │    └─ embedding_brief_header: 简要 Embedding header
    │
    └─ member_conversions (Vec<ConversionResult>)
         ├─ bm25_text: 成员 BM25 描述
         └─ embedding_text: 成员 Embedding 描述
    │
    ▼
smart_chunk_with_header()                 [chunker/chunker.rs]
    │
    ├─ idx == 0: 使用 full header
    └─ idx > 0:  使用 brief header
    │
    ▼
create_chunk_with_header()                [chunker/chunker.rs]
    │
    └─ chunk_text = header_text + "\n\n" + member_texts
```

## 数据结构

### ConversionResult

```rust
// cce_core/src/types/ast_to_nl/result.rs
pub struct ConversionResult {
    // ... 其他字段 ...

    /// 完整 BM25 header（组描述 + 全部成员详情）
    pub bm25_text: Option<String>,

    /// 完整 Embedding header（组描述 + 全部成员详情）
    pub embedding_text: Option<String>,

    /// 简要 BM25 header（组名称 + 成员名称列表）
    pub bm25_brief_header: Option<String>,

    /// 简要 Embedding header（组名称 + 成员名称列表）
    pub embedding_brief_header: Option<String>,
}
```

### GroupConversions

```rust
// cce_parser/src/ast_to_nl/converter/group_converter.rs
pub struct GroupConversions {
    pub group: EntityGroup,
    pub header_conversion: Option<ConversionResult>,  // 包含 full + brief header
    pub member_conversions: Vec<ConversionResult>,     // 成员描述
}
```

## Header 生成

### Full Header

Full header 由模板调度器生成，包含完整的组信息和成员签名：

```
EmbeddingGenerator::generate_for_group()        [embedding/generator.rs]
    └─ GroupTemplateDispatcher::dispatch()       [embedding/templates/dispatcher.rs]
         └─ RegularGroupTemplate::generate()    [embedding/templates/regular.rs]
              ├─ generate_group_description()    // 组级别描述
              └─ generate_member_description()   // 每个成员的完整描述
```

输出示例：
```
once cell inherent_impl. Implements methods for OnceCell of T. public constant new function. Returns OnceCell of T. public constant with_value function. Returns OnceCell of T. public is_initialized function. Returns bool...
```

### Brief Header

Brief header 复用模板调度器的组描述，仅附加成员名称列表：

```rust
// embedding/generator.rs
pub fn generate_brief_for_group(&self, group: &EntityGroup) -> String {
    let descriptions = self.template_dispatcher.dispatch(group);
    let group_desc = descriptions.first()...;
    let member_names = group.members.iter().map(|m| m.name.clone());

    format!("{}. {}: {}.", group_desc, self.member_label(group), member_names.join(", "))
}
```

输出示例：
```
once cell inherent_impl. Implements methods for OnceCell of T. Methods: new, with_value, is_initialized, initialize, wait, get_unchecked, get_mut, into_inner.
```

### 成员标签

根据组类型自动选择标签：

| 组类型 | 标签 |
|--------|------|
| `InherentImpl`, `TraitImpl` | "Methods" |
| `Class`, `Struct` | "Members" |
| 其他 | "Items" |

## Chunk 切分逻辑

### smart_chunk_with_header()

```rust
// chunker/chunker.rs
fn smart_chunk_with_header(&mut self, group, header_conv, member_convs, file_path) {
    // 提取 full header
    let header_bm25 = header_conv.bm25_text...;
    let header_embedding = header_conv.embedding_text...;

    // 提取 brief header
    let brief_bm25 = header_conv.bm25_brief_header...;
    let brief_embedding = header_conv.embedding_brief_header...;

    // 按预算分组成员
    let bm25_groups = group_members_by_budget(member_convs, bm25_budget, Bm25);
    let emb_groups = group_members_by_budget(member_convs, emb_budget, Embedding);

    // 构建 chunk：首个 chunk 用 full header，后续用 brief header
    for (idx, members) in bm25_groups {
        let header_text = if idx == 0 { &header_bm25 } else { &brief_bm25 };
        // ...
    }
}
```

### 预算计算

成员预算需要预留 header 空间：

```rust
let bm25_member_budget = max_bm25_words - header_bm25.word_count();
let emb_member_budget = max_tokens - header_embedding.token_count();
```

由于 brief header 远短于 full header，后续 chunk 实际可用空间会更大，但预算仍按 full header 计算，确保首个 chunk 不会超限。

## 输出示例

### 切分前（单 chunk）

```
=== CHUNK 1/1 ===
[Full Header]
[Member A description]
[Member B description]
...
```

### 切分后（多 chunk）

```
=== CHUNK 1/4 ===
[Full Header: 组描述 + 全部成员签名]
[Member A description]
[Member B description]

=== CHUNK 2/4 ===
[Brief Header: 组名称 + 成员名称列表]
[Member C description]
[Member D description]

=== CHUNK 3/4 ===
[Brief Header: 组名称 + 成员名称列表]
[Member E description]
[Member F description]

=== CHUNK 4/4 ===
[Brief Header: 组名称 + 成员名称列表]
[Member G description]
```

## 涉及文件

| 文件 | 职责 |
|------|------|
| `cce_core/src/types/ast_to_nl/result.rs` | `ConversionResult` 结构定义，包含 `bm25_brief_header` 和 `embedding_brief_header` |
| `cce_parser/src/ast_to_nl/embedding/generator.rs` | `EmbeddingGenerator::generate_brief_for_group()` - 生成简要 Embedding header |
| `cce_parser/src/ast_to_nl/bm25/generator.rs` | `Bm25Generator::generate_brief_for_group()` - 生成简要 BM25 header |
| `cce_parser/src/ast_to_nl/converter/group_converter.rs` | `convert_group_with_generators()` - 填充 brief header 字段 |
| `cce_parser/src/ast_to_nl/chunker/chunker.rs` | `smart_chunk_with_header()` - 根据 chunk 索引选择 full/brief header |
| `cce_parser/src/ast_to_nl/embedding/templates/regular.rs` | `RegularGroupTemplate` - 生成 full header 的模板逻辑 |
| `cce_parser/src/ast_to_nl/bm25/templates/regular.rs` | BM25 模板 - 生成 full header 的模板逻辑 |
