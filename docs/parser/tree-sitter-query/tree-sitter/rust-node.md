以下是提取出的所有节点类型，按出现顺序整理并去重后的列表：

1. `_declaration_statement`
2. `_expression`
3. `_literal`
4. `_literal_pattern`
5. `_pattern`
6. `_type`
7. `abstract_type`
8. `array_expression`
9. `assignment_expression`
10. `async_block`
11. `await_expression`
12. `binary_expression`
13. `block`
14. `break_expression`
15. `call_expression`
16. `closure_expression`
17. `closure_parameters`
18. `compound_assignment_expr`
19. `const_block`
20. `const_item`
21. `const_parameter`
22. `continue_expression`
23. `declaration_list`
24. `dynamic_type`
25. `else_clause`
26. `empty_statement`
27. `enum_item`
28. `enum_variant`
29. `enum_variant_list`
30. `expression_statement`
31. `extern_crate_declaration`
32. `extern_modifier`
33. `field_declaration`
34. `field_declaration_list`
35. `field_expression`
36. `field_initializer`
37. `field_initializer_list`
38. `field_pattern`
39. `for_expression`
40. `for_lifetimes`
41. `foreign_mod_item`
42. `fragment_specifier`
43. `function_item`
44. `function_modifiers`
45. `function_signature_item`
46. `function_type`
47. `gen_block`
48. `generic_function`
49. `generic_pattern`
50. `generic_type`
51. `generic_type_with_turbofish`
52. `higher_ranked_trait_bound`
53. `if_expression`
54. `impl_item`
55. `index_expression`
56. `inner_attribute_item`
57. `inner_doc_comment_marker`
58. `label`
59. `let_chain`
60. `let_condition`
61. `let_declaration`
62. `lifetime`
63. `lifetime_parameter`
64. `line_comment`
65. `match_arm`
66. `match_block`
67. `match_expression`
68. `match_pattern`
69. `mod_item`
70. `mut_pattern`
71. `negative_literal`
72. `never_type`
73. `or_pattern`
74. `ordered_field_declaration_list`
75. `outer_doc_comment_marker`
76. `parameter`
77. `parameters`
78. `parenthesized_expression`
79. `pointer_type`
80. `qualified_type`
81. `range_expression`
82. `range_pattern`
83. `raw_string_literal`
84. `ref_pattern`
85. `reference_expression`
86. `reference_pattern`
87. `reference_type`
88. `remaining_field_pattern`
89. `removed_trait_bound`
90. `return_expression`
91. `scoped_identifier`
92. `scoped_type_identifier`
93. `scoped_use_list`
94. `self_parameter`
95. `shorthand_field_initializer`
96. `shorthand_field_identifier`
97. `slice_pattern`
98. `source_file`
99. `struct_expression`
100. `struct_item`
101. `struct_pattern`
102. `token_binding_pattern`
103. `token_repetition`
104. `token_repetition_pattern`
105. `token_tree`
106. `token_tree_pattern`
107. `type_arguments`
108. `type_binding`
109. `type_cast_expression`
110. `type_item`
111. `type_parameter`
112. `type_parameters`
113. `unary_expression`
114. `union_item`
115. `unit_expression`
116. `unit_type`
117. `unsafe_block`
118. `use_as_clause`
119. `use_declaration`
120. `use_list`
121. `use_wildcard`
122. `visibility_modifier`
123. `where_clause`
124. `where_predicate`
125. `yield_expression`

**符号类型**（`named`为`false`）：  
126. `!`  
127. `!=`  
128. `\"`  
129. `#`  
130. `$`  
131. `%`  
132. `%=`  
133. `&`  
134. `&&`  
135. `&=`  
136. ``` 
137.`_` 
138.`_=` 
139.`+` 
140.`+=` 
141.`,` 
142.`-` 
143.`-=` 
144.`->` 
145.`.` 
146.`..` 
147.`...` 
148.`..=` 
149.`/` 
150.`/\*` 
151.`//` 
152.`/=` 
153.`:` 
154.`::` 
155.`;` 
156.`<` 
157.`<<` 
158.`<<=` 
159.`<=` 
160.`=` 
161.`==` 
162.`=>` 
163.`>` 
164.`>=` 
165.`>>` 
166.`>>=` 
167.`?` 
168.`@` 
169.`[`
170. `]` 
171.`^` 
172.`^=` 
173.`\_` 
174.`as` 
175.`async` 
176.`await` 
177.`block` 
178.`break` 
179.`char_literal` 
180.`const` 
181.`continue` 
182.`crate` 
183.`default` 
184.`doc_comment` 
185.`dyn` 
186.`else` 
187.`enum` 
188.`escape_sequence` 
189.`expr` 
190.`expr_2021` 
191.`extern` 
192.`false` 
193.`field_identifier` 
194.`float_literal` 
195.`fn` 
196.`for` 
197.`gen` 
198.`ident` 
199.`identifier` 
200.`if` 
201.`impl` 
202.`in` 
203.`integer_literal` 
204.`item` 
205.`let` 
206.`lifetime` 
207.`literal` 
208.`loop` 
209.`macro_rules!` 
210.`match` 
211.`meta` 
212.`metavariable` 
213.`mod` 
214.`move` 
215.`mutable_specifier` 
216.`pat` 
217.`pat_param` 
218.`path` 
219.`primitive_type` 
220.`pub` 
221.`raw` 
222.`ref` 
223.`return` 
224.`self` 
225.`shebang` 
226.`shorthand_field_identifier` 
227.`static` 
228.`stmt` 
229.`string_content` 
230.`struct` 
231.`super` 
232.`trait` 
233.`true` 
234.`try` 
235.`tt` 
236.`ty` 
237.`type` 
238.`type_identifier` 
239.`union` 
240.`unsafe` 
241.`use` 
242.`vis` 
243.`where` 
244.`while` 
245.`yield` 
246.`{` 
247.`|` 
248.`|=` 
249.`||` 
250.`}`

---

### 说明：

- **声明类型**（`_declaration_statement`的子类型）：  
  `associated_type`, `attribute_item`, `const_item`, `enum_item`, `extern_crate_declaration`, `foreign_mod_item`, `function_item`, `function_signature_item`, `impl_item`, `let_declaration`, `macro_invocation`, `mod_item`, `static_item`, `struct_item`, `trait_item`, `type_item`, `union_item`, `use_declaration`。
- **表达式类型**（`_expression`的子类型）：  
  `array_expression`, `assignment_expression`, `async_block`, `await_expression`, `binary_expression`, `block`, `break_expression`, `call_expression`, `closure_expression`, `compound_assignment_expr`, `const_block`, `continue_expression`, `field_expression`, `for_expression`, `gen_block`, `generic_function`, `if_expression`, `index_expression`, `loop_expression`, `macro_invocation`, `match_expression`, `parenthesized_expression`, `reference_expression`, `return_expression`, `scoped_identifier`, `struct_expression`, `try_block`, `try_expression`, `tuple_expression`, `type_cast_expression`, `unary_expression`, `unit_expression`, `unsafe_block`, `while_expression`, `yield_expression`。

- **字面量类型**（`_literal`的子类型）：  
  `boolean_literal`, `char_literal`, `float_literal`, `integer_literal`, `raw_string_literal`, `string_literal`。

- **模式类型**（`_pattern`的子类型）：  
  `_literal_pattern`, `captured_pattern`, `const_block`, `generic_pattern`, `identifier`, `macro_invocation`, `mut_pattern`, `or_pattern`, `range_pattern`, `reference_pattern`, `shorthand_field_identifier`, `shorthand_field_identifier`, `tuple_pattern`。

- **类型类型**（`_type`的子类型）：  
  `abstract_type`, `array_type`, `bounded_type`, `dynamic_type`, `function_type`, `generic_type`, `macro_invocation`, `metavariable`, `never_type`, `pointer_type`, `primitive_type`, `reference_type`, `scoped_type_identifier`, `tuple_type`, `type_identifier`, `unit_type`。

- **符号类型**（`named`为`false`的类型）：  
  如`!`, `!=`, `\"`, `#`, `$`, `%`, `%=`, `&`, `&&`, `&=`, `\'`, `*`, `*/`, `*=`, `+`, `+=`, `-`, `-=`, `->`, `.`, `..`, `...`, `..=`, `/`, `/*`, `//`, `/=`, `:`, `::`, `;`, `<`, `<<`, `<<=`, `<=`, `=`, `==`, `=>`, `>`, `>=`, `>>`, `>>=`, `?`, `@`, `[`, `]`, `^`, `^=`, `_`, `as`, `async`, `await`, `block`, `break`, `char_literal`, `const`, `continue`, `crate`, `default`, `doc_comment`, `dyn`, `else`, `enum`, `escape_sequence`, `expr`, `expr_2021`, `extern`, `false`, `field_identifier`, `float_literal`, `fn`, `for`, ``, `ident`, `identifier`, `if`, `impl`, `in`, `integer_literal`, `item`, `let`, `lifetime`, `literal`, `loop`, `macro_rules!`, `match`, `meta`, `metavariable`, `mod`, `move`, `mutable_specifier`, `pat`, `pat_param`, `path`, `primitive_type`, `pub`, `raw`, `ref`, `return`, `self`, `shebang`, `shorthand_field_identifier`, `static`, `stmt`, `string_content`, `struct`, `super`, `trait`, `true`, `try`, `tt`, `ty`, `type`, `type_identifier`, `union`, `unsafe`, `use`, `vis`, `where`, `while`, `yield`, `{`, `|`, `|=`, `||`, `}`。

这些类型涵盖了所有在JSON结构中出现的节点类型，包括嵌套的子类型和符号类型。
