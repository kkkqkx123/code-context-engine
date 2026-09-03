## 命名节点类型（按字母顺序）

| 节点类型                 | 说明             |
| ------------------------ | ---------------- |
| `as`                     |                  |
| `attribute`              | 属性节点         |
| `attribute_name`         | 属性名           |
| `attribute_value`        | 属性值           |
| `await_end_expr`         | await 结束表达式 |
| `await_start_expr`       | await 开始表达式 |
| `await_statement`        | await 语句       |
| `catch_expr`             | catch 表达式     |
| `catch_statement`        | catch 语句       |
| `comment`                | 注释             |
| `const_expr`             | const 表达式     |
| `document`               | 文档根节点       |
| `each_end_expr`          | each 结束表达式  |
| `each_start_expr`        | each 开始表达式  |
| `each_statement`         | each 语句        |
| `element`                | HTML 元素        |
| `else_each_statement`    | else each 语句   |
| `else_expr`              | else 表达式      |
| `else_if_expr`           | else if 表达式   |
| `else_if_statement`      | else if 语句     |
| `else_statement`         | else 语句        |
| `end_tag`                | 结束标签         |
| `erroneous_end_tag_name` | 错误的结束标签名 |
| `expr_attribute_value`   | 表达式属性值     |
| `expression`             | 表达式           |
| `html_expr`              | HTML 表达式      |
| `if_end_expr`            | if 结束表达式    |
| `if_start_expr`          | if 开始表达式    |
| `if_statement`           | if 语句          |
| `key_end_expr`           | key 结束表达式   |
| `key_start_expr`         | key 开始表达式   |
| `key_statement`          | key 语句         |
| `quoted_attribute_value` | 带引号的属性值   |
| `raw_text`               | 原始文本         |
| `raw_text_await`         | await 原始文本   |
| `raw_text_each`          | each 原始文本    |
| `raw_text_expr`          | 表达式原始文本   |
| `script_element`         | script 元素      |
| `self_closing_tag`       | 自闭合标签       |
| `special_block_keyword`  | 特殊块关键字     |
| `start_tag`              | 开始标签         |
| `style_element`          | style 元素       |
| `tag_name`               | 标签名           |
| `text`                   | 文本节点         |
| `then`                   |                  |
| `then_expr`              | then 表达式      |
| `then_statement`         | then 语句        |

---

**节点关系总结：**

- **顶级节点**：`document`
- **HTML 结构**：`element`, `start_tag`, `end_tag`, `self_closing_tag`, `tag_name`, `attribute`, `attribute_name`, `attribute_value`, `expr_attribute_value`, `quoted_attribute_value`
- **特殊元素**：`script_element`, `style_element`
- **控制流语句**：`if_statement`, `each_statement`, `await_statement`, `key_statement`, `then_statement`, `catch_statement`, `else_statement`, `else_if_statement`, `else_each_statement`
- **表达式节点**：`expression`, `html_expr`, `const_expr`
- **原始文本节点**：`raw_text`, `raw_text_expr`, `raw_text_await`, `raw_text_each`
- **其他**：`comment`, `special_block_keyword`, `as`, `then`
