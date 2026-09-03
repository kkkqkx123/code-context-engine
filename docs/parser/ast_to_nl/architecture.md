# AST到自然语言转换架构设计

## 概述

本文档描述 AST to Natural Language（AstToNl）转换模块的整体架构。该模块负责将代码解析器产生的抽象语法树（AST）实体转换为自然语言描述，支持 BM25（关键词搜索）和 Embedding（语义搜索）双路输出。

## 模块位置

源代码位于 `crates/cce_parser/src/ast_to_nl/`，类型定义位于 `crates/cce_core/src/types/ast_to_nl/`。

## 核心设计原则

### 1. 实体组导向（Group-Oriented）

转换以 **实体组（EntityGroup）** 为基本单位，而非单个实体。Grouper 模块将相关实体（如类及其方法）聚合成组，AstToNl 对组整体生成描述。

- 组的头部实体（如类）生成整体描述
- 组的关键成员（如核心方法）生成独立描述
- 样板代码成员被压缩进组描述中

### 2. 双路输出模式

每个转换结果同时支持两种输出模式：

| 模式 | 用途 | 特点 |
|------|------|------|
| **BM25** | 关键词搜索/混合检索 | 保留原始名称、类型名等代码符号 |
| **Embedding** | 语义搜索/向量检索 | 纯语义描述，移除代码符号 |

### 3. 样板代码压缩（Boilerplate Compression）

对 Getter/Setter、DTO、Repository 等样板模式的实体组，压缩成员描述数量：
- 旧方案：1 个类描述 + N 个成员描述
- 新方案：1 个组描述 + M 个核心成员描述（M << N）

### 4. 标准库分组压缩（Stdlib Grouping）

对于标准库实体组（如集合类、I/O 类），使用统一模板生成精简描述。

### 5. 插件扩展机制

通过 `PluginRegistry` 支持外部插件注册自定义模板生成器，优先于内置模板执行。

## 模块架构

```
crates/cce_parser/src/ast_to_nl/
├── mod.rs                    # 模块入口，公共类型重导出
├── options.rs                # ConversionOptions, ConversionRequest
├── error.rs                  # 错误类型
│
├── common/                   # 共享工具
│   ├── mod.rs
│   ├── normalizer.rs         # 名称规范化器
│   ├── utils.rs              # 工具函数
│   └── templates/
│       ├── mod.rs
│       ├── group_trait_base.rs  # GroupTemplateBase trait
│       └── helpers.rs           # 模板辅助函数
│
├── converter/                # 转换器
│   ├── mod.rs
│   ├── entity_converter.rs   # 单实体转换
│   ├── group_converter.rs    # 实体组转换（主入口）
│   ├── helpers.rs            # 转换辅助函数
│   └── patterns/             # 模式转换
│       ├── mod.rs
│       ├── creational.rs     # 创建型模式
│       ├── structural.rs     # 结构型模式
│       ├── behavioral.rs     # 行为型模式
│       ├── data_transfer.rs  # 数据传输模式
│       ├── config_validation.rs # 配置验证模式
│       ├── event_handling.rs # 事件处理模式
│       └── architectural.rs  # 架构模式
│
├── chunker/                  # 分块器
│   ├── mod.rs
│   ├── chunker.rs            # GroupChunker 主类
│   ├── boundary.rs           # 分块边界
│   ├── config.rs             # 分块配置
│   ├── splitter.rs           # 分割策略
│   ├── overlap.rs            # 重叠管理
│   ├── result.rs             # 分块结果
│   └── tracker.rs            # 组跟踪器
│
├── bm25/                     # BM25 路径
│   ├── mod.rs
│   ├── generator.rs          # Bm25Generator
│   ├── keyword_extractor.rs  # 关键词提取
│   ├── mixed_tokenizer.rs    # 混合分词器
│   ├── text_cleaner.rs       # 文本清理
│   ├── type_annotation_cleaner.rs # 类型注解清理
│   └── templates/
│       ├── mod.rs
│       ├── group_trait.rs        # BM25 GroupTemplate trait
│       ├── dispatcher.rs         # BM25 模板分发器
│       ├── design_patterns.rs    # 设计模式模板
│       ├── boilerplate_patterns.rs # 样板模式模板
│       ├── regular.rs            # 常规实体模板
│       └── stdlib.rs             # 标准库模板
│
└── embedding/                # Embedding 路径
    ├── mod.rs
    ├── generator.rs          # EmbeddingGenerator
    └── templates/
        ├── mod.rs
        ├── group_trait.rs        # Embedding GroupTemplate trait
        ├── dispatcher.rs         # Embedding 模板分发器
        ├── design_patterns.rs    # 设计模式模板
        ├── boilerplate_patterns.rs # 样板模式模板
        ├── regular.rs            # 常规实体模板
        └── stdlib.rs             # 标准库模板
```

## 核心组件职责

### 1. AstToNlConverter（主转换器）

**位置**：`converter/mod.rs`

**职责**：接收 `EntityGroup` 列表，转换为 `GroupConversions`（包含组描述 + 成员描述）。

**处理流程**：
1. 检查是否有匹配的插件，优先使用插件批量处理
2. 根据 `GroupType` 分发到具体转换逻辑
3. 模式匹配分发（通过 `pattern_dispatch!` 宏）
4. 生成 `ConversionResult`（含 BM25 文本 + Embedding 文本）

### 2. GroupTemplate（实体组模板）

**定义**：
- BM25: `GroupTemplate` trait → `fn generate(&self, group: &EntityGroup) -> String`
- Embedding: `GroupTemplate` trait → `fn generate(&self, group: &EntityGroup) -> Vec<String>`

**基类**: `GroupTemplateBase` 提供公共方法（成员角色检查、重要成员过滤）

### 3. GroupTemplateDispatcher（模板分发器）

**位置**：
- BM25: `bm25/templates/dispatcher.rs`
- Embedding: `embedding/templates/dispatcher.rs`

**逻辑**：
1. `PatternInfo` 非空 → 使用对应模式模板
2. `is_stdlib_group` → 使用标准库模板
3. 其他 → 使用常规模板

### 4. EmbeddingGenerator / Bm25Generator

**EmbeddingGenerator**：
- 接收 `EntityGroup`，调用 `GroupTemplateDispatcher.dispatch()`
- 对每个描述进行字数截断（基于 `max_summary_words` 配置）

**Bm25Generator**：
- 接收 `EntityGroup`，调用 `GroupTemplateDispatcher.dispatch()`
- 返回包含原始名称、规范化名称、关键词的混合文本

### 5. GroupChunker（分块器）

**位置**：`chunker/chunker.rs`

**职责**：将 `GroupConversions` 切分为适合索引的小块。

**分块策略**：
- ByMembers：按实体边界分割（类、函数）
- BySentences：按句子边界分割（独立实体）
- ByParagraphs：按段落边界分割（模块）
- ByNestedGroups：按嵌套类/结构体边界
- ByTokens：强制按 token 分割（回退）

## 数据流

```
EntityGroup[] 
    → AstToNlConverter.convert_entity_groups()
        → GroupConversions[] (header + member conversions)
    → GroupChunker.chunk_groups()
        → ChunkedResult[] (chunks with metadata)
    → Embedder (向量化)
    → Store (Qdrant + BM25 + SQLite)
```

## 转换结果（ConversionResult）

每个转换结果包含：
- `bm25_text`: BM25 混合文本（保留符号）
- `embedding_text`: Embedding 语义摘要（纯文本）
- `source_entity_ids`: 来源实体 ID
- `source_span`: 源代码位置
- `raw_code`: 原始代码片段
- `entity_metadata`: 实体元数据

## 输出模式

| 模式 | `bm25_text` | `embedding_text` |
|------|------------|-----------------|
| `OutputMode::Bm25` | ✅ 生成 | ❌ 空 |
| `OutputMode::Embedding` | ❌ 空 | ✅ 生成 |
| `OutputMode::Both` | ✅ 生成 | ✅ 生成 |

## 配置项

详见 `ConversionOptions`（`options.rs`）：

```rust
pub struct ConversionOptions {
    pub mode: OutputMode,            // Bm25 / Embedding / Both
    pub include_context: bool,       // 包含文件路径、模块名
    pub include_original_names: bool, // 包含原始名称
    pub include_types: bool,         // 包含类型信息
    pub include_keywords: bool,      // 包含关键词
    pub max_summary_words: usize,    // Embedding 最大字数
    pub include_docstring: bool,     // 包含文档注释
    pub normalize_types: bool,       // 规范化类型名
    pub include_signature: bool,     // 包含函数签名
}
```
