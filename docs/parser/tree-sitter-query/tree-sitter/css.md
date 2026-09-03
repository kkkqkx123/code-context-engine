基于你提供的 `NODE_TYPES` 常量数据，以下是整理后的 **tree-sitter-css** 支持的节点类型列表。

已过滤掉纯符号节点（如 `"#"`, `"("`, `","`, `"@media"` 等）和标记为 `named: false` 的节点，仅保留具有实际语义的命名节点（`named: true`）。

### 📋 CSS 语法树节点分类清单

#### 1. 核心结构与规则

| 节点类型      | 说明                                                 |
| :------------ | :--------------------------------------------------- |
| `stylesheet`  | 根节点，代表整个样式表                               |
| `block`       | 代码块（由 `{}` 包裹的内容）                         |
| `rule_set`    | 规则集（选择器 + 代码块）                            |
| `at_rule`     | At 规则通用节点（如 `@media`, `@supports` 等的容器） |
| `declaration` | 声明（属性名 + 值）                                  |

#### 2. 选择器 (Selectors)

这些节点用于定义 CSS 选择器的不同部分：

- **基础选择器**:
  - `tag_name` (标签名)
  - `class_selector` (类选择器)
  - `id_selector` (ID 选择器)
  - `universal_selector` (通配符 `*`)
  - `namespace_selector` (命名空间前缀)
  - `nesting_selector` (嵌套选择器 `&`)
  - `attribute_selector` (属性选择器 `[...]`)

- **组合关系选择器**:
  - `descendant_selector` (后代选择器，空格分隔)
  - `child_selector` (子元素选择器 `>`)
  - `sibling_selector` (兄弟选择器 `~`)
  - `adjacent_sibling_selector` (相邻兄弟选择器 `+`)

- **伪类与伪元素**:
  - `pseudo_class_selector` (伪类，如 `:hover`, `:nth-child()`)
  - `pseudo_element_selector` (伪元素，如 `::before`, `::after`)

- **查询与逻辑**:
  - `selector_query` (在 `@supports` 或 `@media` 中使用的选择器查询)
  - `binary_expression` (二元表达式，常用于值计算或查询逻辑)

#### 3. 值与数据类型 (Values & Types)

- **数值类型**:
  - `integer_value` (整数值)
  - `float_value` (浮点数值)
  - `color_value` (颜色值)
  - `grid_value` (Grid 布局值)
  - `plain_value` (普通文本值)
  - `string_value` (字符串值)

- **其他值组件**:
  - `unit` (单位，如 `px`, `em`, `%`) - _注：虽然 `float_value` 包含 unit，但这里单独列出作为叶子节点_
  - `important` (`!important` 标记)
  - `parenthesized_value` (括号内的值)
  - `call_expression` (函数调用，如 `rgb()`, `calc()`)

#### 4. At 规则特定语句

| 节点类型              | 对应 CSS 规则        |
| :-------------------- | :------------------- |
| `charset_statement`   | `@charset`           |
| `import_statement`    | `@import`            |
| `keyframes_statement` | `@keyframes`         |
| `media_statement`     | `@media`             |
| `namespace_statement` | `@namespace`         |
| `supports_statement`  | `@supports`          |
| `scope_statement`     | `@scope`             |
| `postcss_statement`   | PostCSS 特定扩展语句 |

#### 5. 查询条件 (Queries)

主要用于 `@media`, `@supports`, `@import` 中的条件判断：

- `keyword_query` (关键字查询，如 `not screen`)
- `feature_query` (特性查询，如 `(min-width: 100px)`)
- `binary_query` (二元逻辑查询，如 `and`, `or`)
- `unary_query` (一元逻辑查询，如 `not`)
- `parenthesized_query` (括号包裹的查询)

#### 6. 动画关键帧 (Keyframes)

- `keyframe_block_list` (关键帧块列表)
- `keyframe_block` (单个关键帧块，如 `from { ... }` 或 `50% { ... }`)
- `from` / `to` (关键帧百分比关键字)

#### 7. 辅助/原子节点

- `arguments` (函数参数列表)
- `property_name` (属性名称)
- `function_name` (函数名称)
- `feature_name` (特性名称，如 `grid`)
- `keyframes_name` (关键帧名称)
- `namespace_name` (命名空间名称)
- `id_name` (ID 名称内容)
- `class_name` (类名内容)
- `at_keyword` (At 关键字，如 `@media` 中的单词部分)
- `identifier` (标识符)
- `escape_sequence` (转义序列)
- `string_content` (字符串内容)

### 💡 补充说明

- **层级关系**: 大多数复杂的结构（如 `rule_set`, `at_rule`, `block`）都通过 `children` 字段引用上述原子节点。
- **命名规范**: 所有列出的节点均带有 `named: true`，这意味着它们在解析树中是可见的、有名字的节点，适合被提取器（Extractor）或查询器（Query）使用。
- **忽略项**: 像 `"@"`, `"# "`, `"."`, `"["`, `"]"`, `"{"`, `"}"`, `";"` 以及 `@media`, `@import` 等具体关键词本身（如果它们没有独立的 `named: true` 节点定义，而是作为 `at_keyword` 的一部分），在此列表中已被视为纯符号或辅助词处理，未单独列出。
