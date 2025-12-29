/*
Go Pattern-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
包含Go语言特有的语法结构模式，不基于字符匹配
*/
export default `
; ===== 错误处理语法结构 =====

; 多返回值函数声明
(function_declaration
  name: (identifier) @error.function
  result: (type_identifier) @error.return.type) @definition.error.returning.function

; 错误检查结构
(if_statement
  condition: (binary_expression
    left: (identifier) @error.variable
    operator: "!="
    right: (identifier) @error.nil)
  consequence: (block) @error.handling.block) @definition.error.checking.structure

; defer语句结构
(defer_statement
  (call_expression
    function: (selector_expression
      operand: (identifier) @resource.object
      field: (field_identifier) @resource.method))) @definition.defer.structure

; ===== 并发语法结构 =====

; 协程创建结构
(go_statement
  (call_expression
    function: (identifier) @goroutine.function
    arguments: (argument_list
      (identifier) @goroutine.arg)*)) @definition.goroutine.structure

; 通道操作结构
[
  (send_statement
    channel: (identifier) @channel.send
    value: (identifier) @channel.value)
  (unary_expression
    ["<-"] @channel.receive.op
    operand: (identifier) @channel.receive))
] @definition.channel.structure

; select语句结构
(select_statement
  body: (block
    (comm_case
      [
        (send_statement
          channel: (identifier) @select.channel
          value: (identifier) @select.value)
        (expression_statement
          (unary_expression
            ["<-"] @select.receive.op
            operand: (identifier) @select.channel))
      ]
      (block) @select.case.body)*
    (default_case
      (block) @select.default.body)?)) @definition.select.structure

; ===== 接口语法结构 =====

; 接口断言结构
(type_assertion_expression
  expression: (identifier) @asserted.value
  type: (type_identifier) @asserted.type) @definition.interface.assertion.structure

; 类型切换结构
(type_switch_statement
  (expression_statement
    (type_assertion_expression
      expression: (identifier) @switch.value
      type: (identifier) @switch.type)))
  (type_switch_statement
    body: (block
      (type_case
        type: (type_identifier) @case.type
        (block) @case.body)*
      (default_case
        (block) @default.body)*)) @definition.type.switch.structure

; 空接口结构
(var_declaration
  (var_spec
    name: (identifier) @empty.interface.var
    type: (interface_type))) @definition.empty.interface.structure

; ===== 函数式语法结构 =====

; 函数字面量结构
(func_literal
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @lambda.param
      type: (_) @lambda.param.type)*)?
  body: (block) @lambda.body) @definition.function.literal.structure

; 函数类型结构
(var_declaration
  (var_spec
    name: (identifier) @function.variable
    type: (function_type
      parameters: (parameter_list
        (parameter_declaration
          name: (identifier) @param.name
          type: (_) @param.type)*)?
      result: (_)? @return.type))) @definition.function.type.structure

; ===== 初始化语法结构 =====

; init函数结构
(function_declaration
  name: (identifier) @init.function
  body: (block) @init.body
  (#eq? @init.function "init")) @definition.init.function.structure

; 包声明结构
(package_clause
  name: (package_identifier) @package.name) @definition.package.structure

; ===== 复合字面量结构 =====

; 结构体字面量结构
(composite_literal
  type: (type_identifier) @struct.type
  body: (literal_value
    (keyed_element
      (literal_element
        (field_identifier) @field.name)
      (literal_element
        (identifier) @field.value))*)) @definition.struct.literal.structure

; 切片字面量结构
(composite_literal
  type: (slice_type
    element: (type_identifier) @slice.element)
  body: (literal_value
    (literal_element
      (identifier) @slice.element)*)) @definition.slice.literal.structure

; 映射字面量结构
(composite_literal
  type: (map_type
    key: (type_identifier) @map.key
    value: (type_identifier) @map.value)
  body: (literal_value
    (keyed_element
      (literal_element
        (identifier) @map.key.value)
      (literal_element
        (identifier) @map.value.value))*)) @definition.map.literal.structure

; ===== 类型定义结构 =====

; 类型别名结构
(type_alias
  name: (type_identifier) @alias.name
  type: (type_identifier) @alias.type) @definition.type.alias.structure

; 泛型类型结构
(type_declaration
  (type_spec
    name: (type_identifier) @generic.type
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @type.parameter)))) @definition.generic.type.structure

; ===== 控制流结构 =====

; for循环结构
(for_statement
  condition: (_) @for.condition
  body: (block) @for.body) @definition.for.loop.structure

; range循环结构
(for_statement
  range_clause: (range_clause
    left: (expression_list
      (identifier) @range.variable)
    right: (expression_list
      (identifier) @range.expression))
  body: (block) @range.body) @definition.range.loop.structure

; ===== 资源管理结构 =====

; 使用语句结构（Go没有using，但有类似的资源管理模式）
(block
  (expression_statement
    (call_expression
      function: (selector_expression
        operand: (identifier) @resource.object
        field: (field_identifier) @resource.open.method)))
  (defer_statement
    (call_expression
      function: (selector_expression
        operand: (identifier) @resource.object
        field: (field_identifier) @resource.close.method)))) @definition.resource.management.structure

; ===== 标签和跳转结构 =====

; 标签语句结构
(labeled_statement
  label: (label_name) @label.name
  statement: (_) @label.statement) @definition.label.structure

; 跳转语句结构
[
  (break_statement
    label: (label_name)? @break.label) @definition.break.structure
  (continue_statement
    label: (label_name)? @continue.label) @definition.continue.structure
  (go_to_statement
    label: (label_name) @goto.label) @definition.goto.structure
] @definition.jump.structure
`;