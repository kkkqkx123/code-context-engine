## ✅ 1. `#eq?` —— 字符串相等比较

**作用**
检查某个节点或字段的**文本内容是否等于指定字符串**。

**语法**
```lisp
(#eq? <field-or-node> "string-value")
```

## ✅ 2. `#match?` —— 正则表达式匹配

**作用**
检查某个节点或字段的**文本内容是否匹配给定的正则表达式**。

**语法**
```lisp
(#match? <field-or-node> "regex-pattern")
```

### 示例 1：匹配所有以 `test_` 开头的函数名（适用于测试框架）
```scm
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @function.test))
(#match? @function.test "^test_")
```

这会匹配：
```c
void test_init();
void test_parse_json();
```

但不会匹配：
```c
void helper_function();
```

### 示例 2：匹配变量类型为指针的声明
```scm
(declaration
  type: _ @type)
; 捕获类型并判断是否包含 "*"
(#match? @type "\\*")
```

或者更精确地匹配指针类型：
```scm
(pointer_type) @type.pointer
```
（不过有时仍需用 `#match?` 辅助处理特殊情况）

---

## 更复杂的联合使用示例

你想高亮所有名为 `setup`、`teardown` 或任何以 `test_` 开头的函数：

```scm
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @test.function))

[
  (#eq? @test.function "setup")
  (#eq? @test.function "teardown")
  (#match? @test.function "^test_")
]
```

这里使用了 `[ ... ]` 表示“任意一个成立即可”(交替查询)。

---

## 📌 常见用途总结

| 场景 | 推荐谓词 | 示例 |
|------|----------|------|
| 精确匹配标识符名 | `#eq?` | `(#eq? @name "main")` |
| 匹配命名模式（如 `test_*`） | `#match?` | `(#match? @name "^test_")` |
| 过滤特定关键字 | `#eq?` | `(#eq? @token "NULL")` |
| 判断类型是否含指针 | `#match?` | `(#match? @type "\\*")` |

---

## 注意事项

1. **大小写敏感**：`#eq?` 和 `#match?` 默认都区分大小写。
   - 若需忽略大小写，可在正则中使用 `(?i)` 标志：
    ```scm
    (#match? @name "(?i)^test") ; 匹配 Test, TEST, test
    ```

2. **性能影响**：过度使用正则可能降低查询性能，尽量优先使用结构化模式匹配。

3. **必须与捕获绑定一起用**：谓词只能作用于已命名的捕获（如 `@xxx`）。

4. **不在核心语法中**：这些是 Tree-sitter **query engine 支持的谓词**，不是文法生成部分的内容。

---

## ✅ 总结对比表

| 谓词 | 全称 | 作用 | 是否支持正则 | 典型用途 |
|------|------|------|----------------|-----------|
| `#eq?` | equal | 文本完全相等 | ❌ 否 | 匹配固定名称（如 `main`, `NULL`） |
| `#match?` | match regex | 正则表达式匹配 | ✅ 是 | 匹配模式（如 `^test_`, `.*_t$` 类型别名） |
