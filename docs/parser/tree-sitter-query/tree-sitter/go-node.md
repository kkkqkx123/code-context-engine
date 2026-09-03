从您提供的 JSON 数据中，我可以提取出所有节点类型（`type` 字段的值）。以下是提取结果：

## 命名节点（named: true）

### 语法节点

1. `_expression`
2. `binary_expression`
3. `call_expression`
4. `composite_literal`
5. `false`
6. `float_literal`
7. `func_literal`
8. `identifier`
9. `imaginary_literal`
10. `index_expression`
11. `int_literal`
12. `interpreted_string_literal`
13. `iota`
14. `nil`
15. `parenthesized_expression`
16. `raw_string_literal`
17. `rune_literal`
18. `selector_expression`
19. `slice_expression`
20. `true`
21. `type_assertion_expression`
22. `type_conversion_expression`
23. `type_instantiation_expression`
24. `unary_expression`
25. `_simple_statement`
26. `assignment_statement`
27. `dec_statement`
28. `expression_statement`
29. `inc_statement`
30. `send_statement`
31. `short_var_declaration`
32. `_simple_type`
33. `array_type`
34. `channel_type`
35. `function_type`
36. `generic_type`
37. `interface_type`
38. `map_type`
39. `negated_type`
40. `pointer_type`
41. `qualified_type`
42. `slice_type`
43. `struct_type`
44. `type_identifier`
45. `_statement`
46. `block`
47. `break_statement`
48. `const_declaration`
49. `continue_statement`
50. `defer_statement`
51. `empty_statement`
52. `expression_switch_statement`
53. `fallthrough_statement`
54. `for_statement`
55. `go_statement`
56. `goto_statement`
57. `if_statement`
58. `labeled_statement`
59. `return_statement`
60. `select_statement`
61. `type_declaration`
62. `type_switch_statement`
63. `var_declaration`
64. `_type`
65. `parenthesized_type`
66. `argument_list`
67. `expression_list`
68. `variadic_argument`
69. `for_clause`
70. `range_clause`
71. `function_declaration`
72. `method_declaration`
73. `parameter_list`
74. `parameter_declaration`
75. `variadic_parameter_declaration`
76. `type_parameter_list`
77. `type_parameter_declaration`
78. `type_constraint`
79. `type_arguments`
80. `type_elem`
81. `field_declaration_list`
82. `field_declaration`
83. `method_elem`
84. `import_declaration`
85. `import_spec`
86. `import_spec_list`
87. `package_clause`
88. `source_file`
89. `statement_list`
90. `type_alias`
91. `type_spec`
92. `var_spec`
93. `var_spec_list`
94. `const_spec`
95. `communication_case`
96. `default_case`
97. `expression_case`
98. `type_case`
99. `literal_value`
100. `literal_element`
101. `keyed_element`
102. `receive_statement`
103. `implicit_length_array_type`
104. `dot`
105. `label_name`
106. `field_identifier`
107. `package_identifier`
108. `blank_identifier`
109. `escape_sequence`
110. `interpreted_string_literal_content`
111. `raw_string_literal_content`
112. `comment` (extra: true)

## 非命名节点（named: false）

包括各种运算符、关键字和标点符号，如：

- 运算符: `!`, `!=`, `%`, `%=`, `&`, `&&`, `&=`, `|`, `||`, `|=`, `^`, `^=`, `*`, `*=`, `+`, `+=`, `-`, `-=`, `/`, `/=`, `<`, `<=`, `>`, `>=`, `<<`, `<<=`, `>>`, `>>=`, `&^`, `&^=`, `<-`, `=`, `==`, `:=`, `...`, `~`
- 标点: `(`, `)`, `[`, `]`, `{`, `}`, `,`, `;`, `:`, `.`, `"`
- 关键字: `break`, `case`, `chan`, `const`, `continue`, `default`, `defer`, `else`, `fallthrough`, `for`, `func`, `go`, `goto`, `if`, `import`, `interface`, `map`, `package`, `range`, `return`, `select`, `struct`, `switch`, `type`, `var`

这些节点类型定义了一个完整的 Go 语言语法树结构，用于解析和分析 Go 源代码。
