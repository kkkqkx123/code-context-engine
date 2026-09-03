# 文档注释处理架构设计

## 概述

本文档描述代码库中文档注释（doc comment）的处理架构设计方案，解决当前文档注释无法正确提取和关联的问题。

## 当前问题分析

### 1. 核心问题

- **doc_comment 字段始终为空**：Entity extractor 尝试查找包含 "doc" 或 "comment" 的 capture，但 entity query 未定义这些 capture
- **覆盖率计算失效**：`calculate_documentation_coverage()` 永远返回 0%，无法正确反映文档注释覆盖情况
- **决策逻辑受影响**：基于文档覆盖率的生成策略决策无法正常工作

### 2. Tree-sitter 注释捕获限制

**重要约束**：Tree-sitter 的 query 语法**不区分注释类别**（文档注释 vs 普通注释）：

```
Rust:
- (line_comment)     匹配 //
- (doc_comment)      匹配 /// 和 //! （Rust 特有）

其他语言（Java/C/Go等）:
- (line_comment)     匹配 //
- (block_comment)    匹配 /* */（包含文档注释和非文档注释）
```

**限制说明**：
- 仅 Rust 有原生的 `doc_comment` 节点类型
- 其他语言（Java、C、Go、Python 等）的文档注释和普通块注释使用相同的 AST 节点
- 必须通过**行跨度分析**来区分单行注释和块级注释

**重要优化**：Tree-sitter 捕获已经包含行列位置信息（`start_point` 和 `end_point`），可以直接通过比较行号来判断注释类型，**无需使用正则表达式**。

## 架构设计方案

### 1. 处理阶段定位

**过滤必须在 Parser 阶段实现**，原因：
- Grouper 阶段接收的是已提取的 Entity 列表，原始注释信息已丢失
- 注释与代码的关联需要源码位置信息（span）
- 早期过滤可减少后续处理的数据量

```
处理流程：
源码 → Tree-sitter Parse → 提取所有注释 → Parser 过滤 → 关联到 Entity → Grouper → Summary
                            ↑
                     在此阶段区分注释类型
```

### 2. 注释分类策略

#### 2.1 基于行跨度的过滤（推荐）

利用 Tree-sitter 捕获的行列位置信息直接判断注释类型：

```rust
/// 判断是否单行注释
fn is_single_line_comment(capture: &Capture) -> bool {
    capture.start_point.0 == capture.end_point.0
    // start_point.0 是起始行号，end_point.0 是结束行号
    // 同行 = 单行注释，跨行 = 块级注释
}
```

**过滤规则**：
- **过滤**：单行注释（start_row == end_row）
- **保留**：块级注释（end_row > start_row）

**特殊处理**：
- **Rust**：`doc_comment` 节点（`///` 和 `//!`）天然保留，不受行跨度限制
- **Python**：docstring 虽然 tree-sitter 识别为 string，但跨多行，通过行跨度判断保留

#### 2.2 块级注释处理

**策略**：不过滤块级注释，统一处理为"文档注释"

理由：
1. 通过行跨度判断简单可靠，无需解析注释内容
2. 块级注释即使不是标准文档格式，通常也包含语义信息
3. 避免过度过滤，保留潜在的文档价值

### 3. 模块设计

#### 3.1 新增模块：`parser::comment_processor`

```rust
/// 注释处理器
/// 
/// 职责：
/// 1. 执行 comment query 提取所有注释
/// 2. 过滤掉单行非文档注释
/// 3. 将有效注释与 Entity 关联
pub struct CommentProcessor;

impl CommentProcessor {
    /// 处理文件注释
    /// 
    /// 步骤：
    /// 1. 执行 comment query 获取所有注释捕获
    /// 2. 应用语言特定的过滤规则
    /// 3. 按位置排序
    /// 4. 关联到最近的后续实体
    pub fn process(
        &self,
        parsed_file: &ParsedFile,
        entities: &[Entity],
    ) -> HashMap<EntityId, String> {
        // 实现逻辑
    }
}
```

#### 3.2 基于行跨度的过滤实现

```rust
/// 注释过滤策略（按语言）
pub struct CommentFilterStrategy {
    /// 是否过滤单行注释
    pub filter_single_line: bool,
    /// 是否特殊处理 doc_comment 节点（仅 Rust）
    pub has_doc_comment_node: bool,
}

impl CommentFilterStrategy {
    pub fn for_language(lang: Language) -> Self {
        match lang {
            Language::Rust => Self {
                // Rust: 有过滤 doc_comment 节点，单行 line_comment 过滤
                filter_single_line: true,
                has_doc_comment_node: true,
            },
            Language::Python => Self {
                // Python: 过滤单行 # 注释，保留多行 docstring
                filter_single_line: true,
                has_doc_comment_node: false,
            },
            Language::Java | Language::C | Language::Cpp | Language::Go | Language::JavaScript | Language::TypeScript => Self {
                // 这些语言：过滤单行 // 注释，保留块级 /* */
                filter_single_line: true,
                has_doc_comment_node: false,
            },
            // ... 其他语言
        }
    }
}

/// 核心过滤函数 - 基于行跨度，无需正则
fn should_keep_comment(capture: &Capture, strategy: &CommentFilterStrategy) -> bool {
    // Rust 特殊处理：doc_comment 节点直接保留
    if strategy.has_doc_comment_node && capture.name.contains("doc") {
        return true;
    }
    
    // 基于行跨度判断
    let start_row = capture.start_point.0;
    let end_row = capture.end_point.0;
    
    if strategy.filter_single_line {
        // 保留跨多行的注释（块级注释）
        // 过滤单行的注释（//, # 等）
        end_row > start_row
    } else {
        // 不过滤，保留所有
        true
    }
}
```

### 4. 数据流设计

#### 4.1 当前数据流（问题）

```
ParsedFile
├── entities: Vec<Entity> (无 doc_comment)
├── raw_relations: Vec<RawRelationData>
└── local_calls: Vec<LocalCall>
```

#### 4.2 改进后数据流

```
ParsedFile
├── entities: Vec<Entity>
│   └── doc_comment: Option<String> （已填充）
├── raw_relations: Vec<RawRelationData>
├── local_calls: Vec<LocalCall>
└── file_doc_comment: Option<String> （新增：文件级文档）
```

### 5. 实现步骤

#### 步骤 1：修改 Entity 结构（最小改动）

文件：`src/types/entity.rs`

```rust
pub struct Entity {
    // ... 现有字段 ...
    
    /// 关联的文档注释（已清理的文本）
    pub doc_comment: Option<String>,
}

pub struct ParsedFile {
    // ... 现有字段 ...
    
    /// 文件级文档注释（模块/包文档）
    pub file_doc_comment: Option<String>,
}
```

#### 步骤 2：创建 CommentProcessor

文件：`src/parser/comment_processor.rs`

```rust
use crate::tree_sitter_query::executor::{QueryExecutor, Capture};
use crate::types::{Entity, EntityId, Language, ParsedFile, Position, Span};

/// 注释条目
pub struct Comment {
    pub text: String,
    pub span: Span,
}

pub struct CommentProcessor {
    query_executor: Arc<QueryExecutor>,
}

impl CommentProcessor {
    pub fn new() -> Self {
        Self {
            query_executor: Arc::new(QueryExecutor::new()),
        }
    }

    /// 提取并过滤注释
    /// 
    /// 核心逻辑：基于行跨度过滤，无需正则表达式
    pub fn extract_comments(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<Comment>, QueryError> {
        let matches = self.query_executor.execute_comment_query(tree, source, language)?;
        let strategy = CommentFilterStrategy::for_language(*language);
        let mut comments = Vec::new();
        
        for mat in matches {
            for capture in &mat.captures {
                // 使用行跨度过滤，无需解析文本内容
                if should_keep_comment(capture, &strategy) {
                    comments.push(Comment {
                        text: capture.text.clone(),
                        span: Span {
                            start_byte: capture.start_byte,
                            end_byte: capture.end_byte,
                            start_position: Position {
                                row: capture.start_point.0,
                                column: capture.start_point.1,
                            },
                            end_position: Position {
                                row: capture.end_point.0,
                                column: capture.end_point.1,
                            },
                        },
                    });
                }
            }
        }
        
        // 按起始位置排序
        comments.sort_by_key(|c| c.span.start_byte);
        Ok(comments)
    }
    
    /// 关联注释到实体
    pub fn associate_comments(
        &self,
        comments: Vec<Comment>,
        entities: &mut [Entity],
    ) {
        for comment in comments {
            if let Some(entity) = find_target_entity(&comment, entities) {
                let cleaned = clean_doc_comment(&comment.text);
                entity.doc_comment = Some(cleaned);
            }
        }
    }
}

/// 基于行跨度的过滤判断
/// 
/// 利用 tree-sitter 捕获的 start_point/end_point 信息：
/// - start_point.0 = 起始行号
/// - end_point.0 = 结束行号
fn should_keep_comment(capture: &Capture, strategy: &CommentFilterStrategy) -> bool {
    // Rust 特殊处理：doc_comment 节点直接保留
    if strategy.has_doc_comment_node && capture.name.contains("doc") {
        return true;
    }
    
    if !strategy.filter_single_line {
        return true;
    }
    
    // 核心逻辑：基于行号判断
    let start_row = capture.start_point.0;
    let end_row = capture.end_point.0;
    
    // 保留跨多行的注释（块级注释）
    // 过滤单行的注释（//, # 等）
    end_row > start_row
}

/// 查找目标实体（最近的后续实体）
fn find_target_entity<'a>(comment: &Comment, entities: &'a mut [Entity]) -> Option<&'a mut Entity> {
    let comment_end = comment.span.end_byte;
    
    entities
        .iter_mut()
        .filter(|e| e.span.start_byte >= comment_end)
        .min_by_key(|e| e.span.start_byte)
}
```

#### 步骤 3：集成到 Parser

文件：`src/parser/mod.rs` 或相关入口

```rust
/// 解析文件主流程
pub fn parse_file(
    &self,
    source: &str,
    path: &str,
    language: Language,
) -> Result<ParsedFile, ParseError> {
    // 1. 解析 AST
    let tree = self.ast_parser.parse(source, &language)?;
    
    // 2. 提取实体（当前逻辑）
    let mut entities = self.entity_extractor.extract(&tree, source, &language)?;
    
    // 3. 【新增】处理注释
    let comment_processor = CommentProcessor::new();
    let comments = comment_processor.extract_comments(&tree, source, &language)?;
    comment_processor.associate_comments(comments, &mut entities);
    
    // 4. 提取关系（当前逻辑）
    let relations = self.relation_extractor.extract(&tree, source, &language)?;
    
    // 5. 构建 ParsedFile
    Ok(ParsedFile {
        language,
        path: path.to_string(),
        source: source.into(),
        entities,
        raw_relations: relations,
        // ... 其他字段 ...
    })
}
```

### 6. 多语言支持矩阵

| 语言 | 单行注释 | 块级/文档注释 | 过滤策略 |
|------|----------|---------------|----------|
| Rust | `//` (单行) | `///`, `//!` (doc_comment 节点) | 保留 doc_comment 节点，过滤单行 line_comment |
| Python | `#` (单行) | `"""..."""` (多行 string) | 过滤单行，保留多行（行跨度 > 1）|
| Java | `//` (单行) | `/** ... */`, `/* */` (多行) | 过滤单行，保留多行（行跨度 > 1）|
| C/C++ | `//` (单行) | `/** ... */`, `/* */` (多行) | 过滤单行，保留多行（行跨度 > 1）|
| Go | `//` (单行) | `/* */` (多行) | 过滤单行，保留多行（行跨度 > 1）|
| JavaScript | `//` (单行) | `/** ... */`, `/* */` (多行) | 过滤单行，保留多行（行跨度 > 1）|
| TypeScript | `//` (单行) | `/** ... */`, `/* */` (多行) | 过滤单行，保留多行（行跨度 > 1）|

**说明**：
- 单行注释：start_row == end_row（同一行开始和结束）
- 块级注释：end_row > start_row（跨越多行）
- 无需解析注释内容，仅通过行列位置判断

### 7. 与下游模块的集成

#### 7.1 Grouper 层

无需修改，`SemanticEntity::from_entity()` 会自动复制 `doc_comment` 字段。

#### 7.2 Summary 层

文件：`src/summary/strategy/decision.rs`

```rust
/// 计算文档覆盖率（修复后逻辑）
fn calculate_documentation_coverage(context: &DecisionContext) -> f32 {
    let public_entities: Vec<_> = context
        .parsed_file
        .entities
        .iter()
        .filter(|e| is_entity_public(e))
        .collect();

    if public_entities.is_empty() {
        return 0.0;
    }

    // 【修复】现在 doc_comment 字段已有值
    let documented_count = public_entities
        .iter()
        .filter(|e| {
            e.doc_comment.as_ref()
                .map(|d| !d.trim().is_empty())
                .unwrap_or(false)
        })
        .count();

    documented_count as f32 / public_entities.len() as f32
}
```

#### 7.3 AST-to-NL 层

文件：`src/ast_to_nl/common/docstring_cleaner.rs`

保持现有逻辑，但输入现在包含已过滤的注释文本。

## 备选方案

### 方案 B：在 Entity Query 中添加注释捕获

**优点**：
- 无需单独的 comment query 执行
- 注释与实体在同一 pattern 中捕获，位置关系明确

**缺点**：
- 需要修改所有语言的 entity query
- tree-sitter query 语法复杂，容易出错
- 对于多行文档注释处理不便

**适用场景**：如果大多数语言的文档注释格式统一，可考虑此方案。

### 方案 C：延迟到 Grouper 阶段处理

**思路**：在 grouper 中根据 `combined_source` 提取文档注释。

**缺点**：
- 需要重新解析源码片段
- 失去精确的位置信息
- 复杂度过高，不推荐

## 结论

推荐采用**方案 A（Parser 阶段处理）**：

1. **创建 `CommentProcessor`** 专门处理注释提取和过滤
2. **基于行跨度过滤**：单行注释（start_row == end_row）过滤，块级注释（end_row > start_row）保留
3. **无需正则表达式**：直接利用 tree-sitter 捕获的行列位置信息
4. **基于位置关联**注释到最近的后续实体
5. **填充 Entity.doc_comment** 字段，修复覆盖率计算

此方案优势：
- **高效**：基于整数比较，无需字符串解析
- **可靠**：不受注释内容格式影响
- **简单**：代码逻辑清晰易维护
- **通用**：适用于所有语言（仅 Rust 需特殊处理 doc_comment 节点）
- **符合现有架构**：与 tree-sitter 集成紧密
