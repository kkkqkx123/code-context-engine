# Vue/Svelte 嵌入代码块解析设计方案

## 1. 问题分析

### 1.1 当前现状

Query模块对Vue/Svelte文件中的`script`和`style`块采用**浅层解析**策略：

- 仅识别块边界（start_tag, raw_text, end_tag）
- `raw_text`内容作为原始文本捕获，不做进一步解析
- 无法提取script中的函数、类、导入关系
- 无法提取style中的CSS选择器、属性

### 1.2 影响

- **语义缺失**：无法获取SFC文件完整的代码结构
- **关系断裂**：无法进行跨语言调用链分析（如template→script的函数调用）
- **索引不完整**：嵌入代码块中的实体无法被向量化和检索

## 2. 设计目标

1. **深度解析**：对script/style块进行与独立JS/CSS文件同等的解析
2. **代码复用**：复用现有的JS/TS/CSS query schemes，避免重复实现
3. **位置映射**：保持嵌入代码在原文件中的准确位置信息
4. **关系连通**：建立跨块关系（template→script→style）

## 3. 设计方案

### 3.1 架构设计

采用**"主从解析"（Primary-Sub Parsing）**架构：

```
Vue/Svelte File
    ├── Template (Vue/Svelte parser处理)
    ├── Script   (提取 → JS/TS parser二次解析)
    └── Style    (提取 → CSS parser二次解析)
```

### 3.2 核心组件

#### 3.2.1 EmbeddedBlock 类型

```rust
#[derive(Debug, Clone)]
pub struct EmbeddedBlock {
    pub block_type: BlockType,           // Script | Style | Template
    pub language: Language,              // JavaScript | TypeScript | Css | Scss
    pub content: String,                 // 提取的代码内容
    pub span: Span,                      // 在原文件中的位置
    pub attributes: HashMap<String, String>, // lang="ts", scoped, etc.
}
```

#### 3.2.2 处理流程

```
1. Vue/Svelte Parser (识别block边界和属性)
           │
           ▼
2. EmbeddedBlock Extractor (提取raw_text)
           │
      ┌────┴────┐
      ▼         ▼
3. JS/TS Parser   CSS Parser (复用现有query)
      │              │
      ▼              ▼
4. 实体/关系提取   实体/关系提取
      │              │
      └────┬─────────┘
           ▼
5. Span Offset调整 (映射回原文件位置)
           │
           ▼
6. 跨块关系提取 (template→script关联)
```

### 3.3 Query Scheme 调整

#### Vue/Svelte Query（简化）

仅负责识别block边界和属性：

```scheme
(script_element
    (start_tag
        (attribute
            (attribute_name) @attr.name
            (quoted_attribute_value (attribute_value) @attr.value)?
        )* @script.attributes
    ) @script.start_tag
    (raw_text) @script.content
) @embedded.script

(style_element
    (start_tag
        (attribute
            (attribute_name) @attr.name
            (quoted_attribute_value (attribute_value) @attr.value)?
        )* @style.attributes
    ) @style.start_tag
    (raw_text) @style.content
) @embedded.style
```

#### JS/TS/CSS Query（复用）

无需修改，直接使用现有的：
- `javascript.rs`
- `typescript.rs`
- `css.rs`

### 3.4 关键实现点

#### 3.4.1 语言识别

根据block的属性确定子语言：

```rust
fn detect_embedded_language(block_type: BlockType, attrs: &HashMap<String, String>) -> Language {
    match block_type {
        BlockType::Script => {
            match attrs.get("lang").map(|s| s.as_str()) {
                Some("ts") | Some("typescript") => Language::TypeScript,
                Some("js") | Some("javascript") | None => Language::JavaScript,
            }
        }
        BlockType::Style => {
            match attrs.get("lang").map(|s| s.as_str()) {
                Some("scss") => Language::Scss,
                Some("less") => Language::Less,
                Some("css") | None => Language::Css,
            }
        }
        _ => Language::Unknown,
    }
}
```

#### 3.4.2 Span偏移量调整

将子解析结果的位置映射回原文件：

```rust
fn adjust_span(span: &mut Span, offset: usize) {
    span.start_byte += offset;
    span.end_byte += offset;
    // 行列位置也需要调整...
}
```

#### 3.4.3 跨块关系提取

识别template与script之间的引用关系：

```rust
// 示例：template中的@click="handler"
// → 关联到script中的function handler() {}
pub fn extract_cross_block_relations(
    template_entities: &[Entity],
    script_entities: &[Entity],
) -> Vec<BlockRelation> {
    // 实现逻辑...
}
```

## 4. 实施步骤

### Phase 1: 基础类型定义

1. 在`src/types`中添加`embedded.rs`模块
2. 定义`EmbeddedBlock`、`BlockType`、`BlockRelation`类型
3. 扩展`ParsedFile`结构

### Phase 2: Block提取

1. 创建`src/parser/embedded_parser.rs`
2. 实现`extract_blocks()`方法
3. 在Vue/Svelte parser中集成block提取

### Phase 3: 子解析集成

1. 实现`parse_block()`方法
2. 调用现有JS/TS/CSS query进行解析
3. 实现span偏移量调整

### Phase 4: 跨块关系

1. 实现`extract_cross_block_relations()`
2. 识别常见的template→script模式（事件绑定、props等）

### Phase 5: Query Loader扩展

1. 在`loader.rs`中启用Vue/Svelte语言支持
2. 添加embedded block查询类型

## 5. 预期收益

| 指标 | 当前 | 预期 |
|------|------|------|
| Vue/Svelte实体覆盖率 | ~20% (仅template) | ~90% (template+script+style) |
| 跨文件关系 | 仅template | template↔script↔style |
| 代码复用度 | 低 | 高 (复用JS/TS/CSS query) |
| 维护成本 | 高 | 低 |

## 6. 兼容性考虑

1. **向后兼容**：独立JS/CSS文件解析不受影响
2. **渐进启用**：可通过配置开关控制是否启用embedded解析
3. **错误隔离**：子解析失败不影响主文件解析
