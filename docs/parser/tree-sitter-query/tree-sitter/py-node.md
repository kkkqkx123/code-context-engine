这段文本定义了 Python AST（抽象语法树）的节点类型列表。它包含了结构化的语句、表达式、模式、关键字以及特殊字符等。

以下是提取出的所有 **`type`** 字段的值（即节点类型）：

### 1. 核心语句与块 (Statements & Blocks)

- `_compound_statement` (复合语句父类)
  - `class_definition`
  - `decorated_definition`
  - `for_statement`
  - `function_definition`
  - `if_statement`
  - `match_statement`
  - `try_statement`
  - `while_statement`
  - `with_statement`
- `_simple_statement` (简单语句父类)
  - `assert_statement`
  - `break_statement`
  - `continue_statement`
  - `delete_statement`
  - `exec_statement`
  - `expression_statement`
  - `future_import_statement`
  - `global_statement`
  - `import_from_statement`
  - `import_statement`
  - `nonlocal_statement`
  - `pass_statement`
  - `print_statement`
  - `raise_statement`
  - `return_statement`
  - `type_alias_statement`
- `block`
- `class_definition`
- `decorated_definition`
- `for_in_clause`
- `for_statement`
- `elif_clause`
- `else_clause`
- `except_clause`
- `finally_clause`
- `if_clause`
- `if_statement`
- `match_statement`
- `parameters`
- `relative_import`
- `return_statement`
- `try_statement`
- `typed_default_parameter`
- `typed_parameter`
- `type_alias_statement`
- `type_parameter`
- `while_statement`
- `with_clause`
- `with_item`
- `with_statement`
- `yield`

### 2. 表达式 (Expressions)

- `expression` (基类)
- `as_pattern`
- `boolean_operator` (`and`, `or`)
- `comparison_operator`
- `conditional_expression`
- `lambda`
- `named_expression`
- `not_operator`
- `primary_expression` (基类，包含各类原子表达式)
- `alias` (注意：这是导入别名，有时被归类为 `aliased_import` 的一部分，此处单独列出 `aliased_import`)
- `argument_list`
- `await`
- `binary_operator`
- `call`
- `concatenated_string`
- `dictionary`
- `dictionary_comprehension`
- `dotted_name`
- `ellipsis`
- `float`
- `generator_expression`
- `generic_type`
- `integer`
- `identifier` (出现在多种上下文中)
- `interpolation`
- `keyword_argument`
- `list`
- `list_comprehension`
- `list_splat`
- `parenthesized_expression`
- `parenthesized_list_splat`
- `set`
- `set_comprehension`
- `slice`
- `string`
- `subscript`
- `tuple`
- `unary_operator`
- `type_conversion`
- `union_type`
- `format_expression`
- `escape_interpolation`
- `escape_sequence`

### 3. 参数与模式 (Parameters & Patterns)

- `parameter`
- `default_parameter`
- `dictionary_splat_pattern`
- `dictionary_splat`
- `identifier` (再次确认)
- `keyword_separator`
- `list_splat_pattern`
- `positional_separator`
- `tuple_pattern`
- `pattern` (基类)
- `as_pattern` (重复，见上文)
- `attribute` (属性访问)
- `class_pattern`
- `complex_pattern`
- `case_pattern`
- `concatenated_string` (重复，见上文)
- `dict_pattern`
- `dotted_name` (重复，见上文)
- `false` (布尔值字面量)
- `float` (重复，见上文)
- `integer` (重复，见上文)
- `key` (在模式上下文中，如 `dict_pattern` 的 key) -> 实际上文本中是作为 pattern type 列表的一部分出现的，但具体字段名是 pattern type。
- `none` (NoneType 字面量)
- `splat_pattern`
- `string` (重复，见上文)
- `true` (True 字面量)
- `subscript` (重复，见上文)
- `union_pattern`
- `list_pattern`

### 4. 装饰器与其他元数据 (Decorators & Metadata)

- `decorator`
- `expression` (在 decorator 中)
- `statement` (某些语句作为 children 出现)
- `comment` (注释)
- `line_continuation` (换行继续)

### 5. 符号与关键字 (Keywords & Symbols - `named: false`)

- `%`
- `%= `
- `&`
- `&=`
- `(`
- `)`
- `*`
- `**`
- `**=`
- `*=`
- `+`
- `+=`
- `,`
- `-`
- `-=`
- `->`
- `.`
- `/`
- `//`
- `//= `
- `/= `
- `:`
- `:=`
- `;`
- `<`
- `<<`
- `<<=`
- `<=`
- `<>`
- `=`
- `==`
- `>`
- `>=`
- `>>`
- `>= `
- `@`
- `@=`
- `[`
- `\\`
- `]`
- `^`
- `^=`
- `_`
- `__future__`
- `and`
- `as`
- `assert`
- `async`
- `await`
- `break`
- `case`
- `class`
- `continue`
- `def`
- `del`
- `elif`
- `else`
- `except`
- `exec`
- `finally`
- `for`
- `from`
- `global`
- `if`
- `import`
- `in`
- `is`
- `lambda`
- `match`
- `nonlocal`
- `not`
- `or`
- `pass`
- `print`
- `raise`
- `return`
- `string_start` (虽然字符串内内容 named:true, 但这里指字符串开始标记可能被视为 non-named token 或 context marker? 注：文本中 `string_content` 下的子项有 named:false, 如 string_start/end/named:false? **修正**: 查看原文，`string_end` 和 `string_start` 定义为 `named: true` 但 `extra: true` 或其他？不，看最后部分：
  - `"type": "string_end", "named": true`
  - `"type": "string_start", "named": true`
  - `"type": "{", "named": false` -> 这里的标点符号大多名为 false。

### 6. 其他结构元素

- `pair`
- `member_type`
- `module` (根模块)
- `type` (类型构造/字面量)
- `constrained_type`
- `union_type`
- `splat_type`
- `keyword_pattern`
- `chevron` (用于 print statement)

---

### 汇总列表 (仅包含唯一且重要的 Node Types)

为了方便使用，去重后的完整节点类型列表如下：

```json
[
  "_compound_statement",
  "_simple_statement",
  "expression",
  "parameter",
  "pattern",
  "aliased_import",
  "argument_list",
  "as_pattern",
  "assert_statement",
  "assignment",
  "attribute",
  "augmented_assignment",
  "await",
  "binary_operator",
  "block",
  "boolean_operator",
  "break_statement",
  "call",
  "case_clause",
  "case_pattern",
  "chevron",
  "class_definition",
  "class_pattern",
  "comparison_operator",
  "complex_pattern",
  "concatenated_string",
  "conditional_expression",
  "constrained_type",
  "continue_statement",
  "decorated_definition",
  "decorator",
  "default_parameter",
  "delete_statement",
  "dict_pattern",
  "dictionary",
  "dictionary_comprehension",
  "dictionary_splat",
  "dictionary_splat_pattern",
  "dotted_name",
  "elif_clause",
  "else_clause",
  "except_clause",
  "exec_statement",
  "expression_list",
  "expression_statement",
  "finally_clause",
  "for_in_clause",
  "for_statement",
  "format_expression",
  "format_specifier",
  "function_definition",
  "future_import_statement",
  "generator_expression",
  "generic_type",
  "global_statement",
  "if_clause",
  "if_statement",
  "import_from_statement",
  "import_prefix",
  "import_statement",
  "identifier",
  "if_clause",
  "interpolation",
  "is_not",
  "keyword_argument",
  "keyword_pattern",
  "keyword_separator",
  "lambda",
  "lambda_parameters",
  "list",
  "list_comprehension",
  "list_pattern",
  "list_splat",
  "list_splat_pattern",
  "match_statement",
  "member_type",
  "module",
  "named_expression",
  "nonlocal_statement",
  "not_in",
  "not_operator",
  "pair",
  "parenthesized_expression",
  "parenthesized_list_splat",
  "pass_statement",
  "pattern_list",
  "positional_separator",
  "print_statement",
  "raise_statement",
  "relative_import",
  "return_statement",
  "set",
  "set_comprehension",
  "slice",
  "splat_pattern",
  "splat_type",
  "string",
  "string_content",
  "subscript",
  "try_statement",
  "tuple",
  "tuple_pattern",
  "type",
  "type_alias_statement",
  "type_parameter",
  "typed_default_parameter",
  "typed_parameter",
  "unary_operator",
  "union_pattern",
  "union_type",
  "while_statement",
  "wildcard_import",
  "with_clause",
  "with_item",
  "with_statement",
  "yield",
  "false",
  "float",
  "integer",
  "none",
  "true",
  "!",
  "%",
  "&",
  "*",
  "+",
  ",",
  "-",
  "/",
  "//",
  "<<",
  "=",
  ">=",
  "->",
  ".",
  "(",
  "[",
  "\\",
  "]",
  "^",
  "a",
  "e",
  "i",
  "j",
  "k",
  "l",
  "m",
  "n",
  "o",
  "p",
  "q",
  "r",
  "s",
  "t",
  "u",
  "v",
  "w",
  "x",
  "y",
  "z",
  "{",
  "|",
  "}",
  "~",
  ";",
  "<",
  ">"
]
```

_(注：上面的 JSON 列表中为了区分符号混排，实际使用时请严格按照原始文本中的 `type` 字段严格提取。上述纯文本列表主要展示了名称，下面的代码块提供了精准的解析结果)_

**最准确的节点类型集合 (Clean List):**

1.  `_compound_statement`
2.  `_simple_statement`
3.  `class_definition`
4.  `decorated_definition`
5.  `for_statement`
6.  `function_definition`
7.  `if_statement`
8.  `match_statement`
9.  `try_statement`
10. `while_statement`
11. `with_statement`
12. `assert_statement`
13. `break_statement`
14. `continue_statement`
15. `delete_statement`
16. `exec_statement`
17. `expression_statement`
18. `future_import_statement`
19. `global_statement`
20. `import_from_statement`
21. `import_statement`
22. `nonlocal_statement`
23. `pass_statement`
24. `print_statement`
25. `raise_statement`
26. `return_statement`
27. `type_alias_statement`
28. `block`
29. `argument_list`
30. `keyword_argument`
31. `elif_clause`
32. `else_clause`
33. `except_clause`
34. `finally_clause`
35. `if_clause`
36. `parameters`
37. `relative_import`
38. `typed_default_parameter`
39. `typed_parameter`
40. `for_in_clause`
41. `yield`
42. `alias` (Alias in import context) -> Note: The list has `aliased_import`.
43. `named_expression`
44. `await`
45. `lambda`
46. `primary_expression`
47. `binary_operator`
48. `call`
49. `conditional_expression`
50. `concatenated_string`
51. `dictionary`
52. `dictionary_comprehension`
53. `dotted_name`
54. `ellipsis`
55. `float`
56. `generator_expression`
57. `generic_type`
58. `generic_type` (duplicate check)
59. `identifier`
60. `interpolation`
61. `list`
62. `list_comprehension`
63. `list_splat`
64. `parenthesized_expression`
65. `parenthesized_list_splat`
66. `set`
67. `set_comprehension`
68. `slice`
69. `string`
70. `subscript`
71. `tuple`
72. `unary_operator`
73. `type_conversion`
74. `format_expression`
75. `escape_interpolation`
76. `escape_sequence`
77. `keyword_pattern`
78. `type`
79. `constrained_type`
80. `union_type`
81. `splat_type`
82. `member_type`
83. `dict_pattern`
84. `case_pattern`
85. `class_pattern`
86. `complex_pattern`
87. `list_pattern`
88. `tuple_pattern`
89. `attribute`
90. `keyword_pattern`
91. `union_pattern`
92. `as_pattern`
93. `dictionary_splat_pattern`
94. `splat_pattern`
95. `chained_comparison` (No, it's just comparison_operator)
96. `check_operator` (No)
97. `operator` tokens (Mostly literals like `!=`, `%`, etc.)
98. `comment`
99. `line_continuation`
100.  `chevron`

**(注意：大量符号如 `!=`, `%`, `*` 等也被定义为独立的 node types，它们的 `named` 字段通常为 `false`)**
