0.23.2

## HTML 节点类型分类

### 1. 根节点类型

| 类型       | 说明                           | 是否命名节点 |
| ---------- | ------------------------------ | ------------ |
| `document` | 文档根节点，包含整个 HTML 文档 | ✓            |

### 2. 元素相关节点

| 类型                     | 说明                     | 是否命名节点 |
| ------------------------ | ------------------------ | ------------ |
| `element`                | 通用元素节点             | ✓            |
| `start_tag`              | 开始标签，如 `<div>`     | ✓            |
| `end_tag`                | 结束标签，如 `</div>`    | ✓            |
| `self_closing_tag`       | 自闭合标签，如 `<img />` | ✓            |
| `erroneous_end_tag`      | 错误的结束标签           | ✓            |
| `erroneous_end_tag_name` | 错误的结束标签名称       | ✓            |
| `tag_name`               | 标签名称                 | ✓            |

### 3. 属性相关节点

| 类型                     | 说明                       | 是否命名节点 |
| ------------------------ | -------------------------- | ------------ |
| `attribute`              | 属性节点，如 `class="foo"` | ✓            |
| `attribute_name`         | 属性名称                   | ✓            |
| `attribute_value`        | 属性值（未引号）           | ✓            |
| `quoted_attribute_value` | 引号包裹的属性值           | ✓            |

### 4. 特殊元素节点

| 类型             | 说明            | 是否命名节点 |
| ---------------- | --------------- | ------------ |
| `script_element` | `<script>` 元素 | ✓            |
| `style_element`  | `<style>` 元素  | ✓            |

### 5. 内容节点

| 类型       | 说明                             | 是否命名节点 |
| ---------- | -------------------------------- | ------------ |
| `text`     | 普通文本内容                     | ✓            |
| `raw_text` | 原始文本（如 script/style 内部） | ✓            |
| `comment`  | HTML 注释                        | ✓            |
| `entity`   | HTML 实体，如 `&amp;`            | ✓            |
| `doctype`  | 文档类型声明 `<!DOCTYPE>`        | ✓            |

### 6. 语法符号节点（匿名节点）

这些是语法中的符号，不属于命名节点：

| 符号 | 说明                |
| ---- | ------------------- |
| `<`  | 标签开始            |
| `</` | 结束标签开始        |
| `/>` | 自闭合标签结束      |
| `>`  | 标签结束            |
| `<!` | 注释或 doctype 开始 |
| `=`  | 属性赋值            |
| `"`  | 双引号              |
| `'`  | 单引号              |

### 节点层级关系

```
document
├── doctype
├── element
│   ├── start_tag
│   │   ├── tag_name
│   │   └── attribute
│   │       ├── attribute_name
│   │       └── attribute_value / quoted_attribute_value
│   ├── text
│   ├── element (嵌套)
│   └── end_tag
│       └── tag_name
├── script_element
│   ├── start_tag
│   ├── raw_text
│   └── end_tag
├── style_element
│   ├── start_tag
│   ├── raw_text
│   └── end_tag
├── self_closing_tag
│   ├── tag_name
│   └── attribute
└── comment
```

这些节点类型定义了 HTML 语法树的完整结构，用于解析和操作 HTML 文档。
