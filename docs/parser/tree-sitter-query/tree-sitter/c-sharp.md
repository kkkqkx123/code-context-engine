好的，这是从您提供的文本中整理出的C#节点类型列表。

为了清晰起见，我将它们按主要类别（如声明、表达式、语句等）进行了分组。

### C# 语法节点类型

#### 1. 根节点

- `compilation_unit`

#### 2. 声明 (Declaration)

- `declaration`
  - `class_declaration`
  - `constructor_declaration`
  - `conversion_operator_declaration`
  - `delegate_declaration`
  - `destructor_declaration`
  - `enum_declaration`
  - `event_declaration`
  - `event_field_declaration`
  - `field_declaration`
  - `indexer_declaration`
  - `interface_declaration`
  - `method_declaration`
  - `namespace_declaration`
  - `operator_declaration`
  - `preproc_if` (作为声明的一种形式)
  - `property_declaration`
  - `record_declaration`
  - `struct_declaration`
  - `using_directive`
- `type_declaration`
  - `class_declaration`
  - `delegate_declaration`
  - `enum_declaration`
  - `interface_declaration`
  - `record_declaration`
  - `struct_declaration`
- `local_function_statement` (局部函数声明)
- `variable_declaration` (变量声明)

#### 3. 表达式 (Expression)

- `expression`
  - `lvalue_expression`
  - `non_lvalue_expression`
- `lvalue_expression` (左值表达式)
  - `element_access_expression`
  - `element_binding_expression`
  - `generic_name`
  - `identifier`
  - `member_access_expression`
  - `parenthesized_expression`
  - `prefix_unary_expression`
  - `this`
  - `tuple_expression`
- `non_lvalue_expression` (非左值表达式)
  - `anonymous_method_expression`
  - `anonymous_object_creation_expression`
  - `array_creation_expression`
  - `as_expression`
  - `assignment_expression`
  - `await_expression`
  - `base`
  - `binary_expression`
  - `cast_expression`
  - `checked_expression`
  - `conditional_access_expression`
  - `conditional_expression`
  - `default_expression`
  - `implicit_array_creation_expression`
  - `implicit_object_creation_expression`
  - `implicit_stackalloc_expression`
  - `initializer_expression`
  - `interpolated_string_expression`
  - `invocation_expression`
  - `is_expression`
  - `is_pattern_expression`
  - `lambda_expression`
  - `literal`
  - `makeref_expression`
  - `object_creation_expression`
  - `parenthesized_expression`
  - `postfix_unary_expression`
  - `prefix_unary_expression`
  - `preproc_if` (作为表达式的一种形式)
  - `query_expression`
  - `range_expression`
  - `ref_expression`
  - `reftype_expression`
  - `refvalue_expression`
  - `sizeof_expression`
  - `stackalloc_expression`
  - `switch_expression`
  - `throw_expression`
  - `typeof_expression`
  - `with_expression`
- `literal` (字面量)
  - `boolean_literal`
  - `character_literal`
  - `integer_literal`
  - `null_literal`
  - `raw_string_literal`
  - `real_literal`
  - `string_literal`
  - `verbatim_string_literal`

#### 4. 语句 (Statement)

- `statement`
  - `block`
  - `break_statement`
  - `checked_statement`
  - `continue_statement`
  - `do_statement`
  - `empty_statement`
  - `expression_statement`
  - `fixed_statement`
  - `for_statement`
  - `foreach_statement`
  - `goto_statement`
  - `if_statement`
  - `labeled_statement`
  - `local_declaration_statement`
  - `local_function_statement` (也是局部函数声明)
  - `lock_statement`
  - `preproc_if` (作为语句的一种形式)
  - `return_statement`
  - `switch_statement`
  - `throw_statement`
  - `try_statement`
  - `unsafe_statement`
  - `using_statement`
  - `while_statement`
  - `yield_statement`

#### 5. 模式 (Pattern)

- `pattern`
  - `and_pattern`
  - `constant_pattern`
  - `declaration_pattern`
  - `discard`
  - `list_pattern`
  - `negated_pattern`
  - `or_pattern`
  - `parenthesized_pattern`
  - `recursive_pattern`
  - `relational_pattern`
  - `type_pattern`
  - `var_pattern`

#### 6. 类型 (Type)

- `type`
  - `alias_qualified_name`
  - `array_type`
  - `function_pointer_type`
  - `generic_name`
  - `identifier`
  - `implicit_type`
  - `nullable_type`
  - `pointer_type`
  - `predefined_type`
  - `qualified_name`
  - `ref_type`
  - `scoped_type`
  - `tuple_type`

#### 7. 访问器/属性/索引器

- `accessor_declaration`
- `accessor_list`

#### 8. 参数/参数列表

- `parameter`
- `parameter_list`
- `bracketed_parameter_list`

#### 9. 属性 (Attribute)

- `attribute`
- `attribute_list`
- `global_attribute`

#### 10. 预处理器指令 (Preprocessor Directive)

- `preproc_if`
- `preproc_elif`
- `preproc_else`
- `preproc_define`
- `preproc_undef`
- `preproc_error`
- `preproc_warning`
- `preproc_region`
- `preproc_endregion`
- `preproc_line`
- `preproc_pragma`
- `preproc_nullable`

#### 11. 查询表达式 (Query Expression)

- `from_clause`
- `let_clause`
- `where_clause`
- `join_clause`
- `join_into_clause`
- `order_by_clause`
- `group_clause`
- `select_clause`

#### 12. 其他辅助节点

- `argument`
- `argument_list`
- `bracketed_argument_list`
- `attribute_argument`
- `attribute_argument_list`
- `base_list`
- `block`
- `catch_clause`
- `catch_declaration`
- `catch_filter_clause`
- `finally_clause`
- `declaration_list`
- `enum_member_declaration`
- `enum_member_declaration_list`
- `explicit_interface_specifier`
- `initializer_expression`
- `interpolation`
- `interpolation_alignment_clause`
- `interpolation_format_clause`
- `modifier`
- `positional_pattern_clause`
- `property_pattern_clause`
- `subpattern`
- `switch_body`
- `switch_expression_arm`
- `switch_section`
- `tuple_element`
- `type_argument_list`
- `type_parameter`
- `type_parameter_list`
- `type_parameter_constraint`
- `type_parameter_constraints_clause`
- `variable_declarator`
- `when_clause`
- `with_initializer`

#### 13. 基础/内部节点

- `identifier`
- `generic_name`
- `qualified_name`
- `alias_qualified_name`
- `implicit_parameter`
- `discard` (作为模式的一部分)
- `parenthesized_variable_designation`
- `tuple_pattern`
