/*
Go Data Flow-specific Tree-Sitter Query Patterns
用于追踪变量间的数据传递关系
*/
export default `
; 统一的赋值数据流查询 - 使用交替模式
(assignment_statement
  left: (expression_list
    [
      (identifier) @target.variable
      (selector_expression
        operand: (identifier) @target.object
        field: (field_identifier) @target.field)
      (index_expression
        operand: (identifier) @target.array
        index: (identifier) @target.index)
    ])
  right: (expression_list
    [
      (identifier) @source.variable
      (call_expression
        function: (identifier) @source.function)
      (binary_expression
        left: (_) @binary.left
        right: (_) @binary.right)
    ])) @data.flow.assignment

; 短变量声明数据流
(short_var_declaration
  left: (expression_list
    (identifier) @target.variable)
  right: (expression_list
    (identifier) @source.variable)) @data.flow.short.declaration

; 函数调用数据流 - 使用参数化查询
(call_expression
  function: [
    (identifier) @target.function
    (selector_expression
      operand: (identifier) @target.object
      field: (field_identifier) @target.method)
  ]
  arguments: (argument_list
    (identifier) @source.parameter)+) @data.flow.parameter.passing

; 返回值数据流 - 使用锚点操作符
(return_statement
  .
  [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (selector_expression
      operand: (identifier) @source.object
      field: (field_identifier) @source.field)
  ]) @data.flow.return.value

; 闭包捕获数据流 - 使用谓词过滤
(func_literal
  body: (block
    (expression_statement
      (identifier) @captured.variable))
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @capture.source))) @data.flow.closure.capture

; 范围for循环数据流
(range_clause
  left: (expression_list
    (identifier) @target.variable)
  right: (expression_list
    (identifier) @source.range)) @data.flow.range.for

; 通道操作数据流 - 使用交替模式
[
  (send_statement
    channel: (identifier) @target.channel
    value: (identifier) @source.value)
  (unary_expression
    ["<-"] @receive.op
    operand: (identifier) @source.channel)
] @data.flow.channel.operation

; 映射操作数据流 - 使用谓词过滤
(call_expression
  function: (selector_expression
    operand: (identifier) @source.map
    field: (field_identifier) @map.method)
  arguments: (argument_list
    (identifier) @target.key
    (identifier) @target.value)?)
  (#match? @map.method "^(set|put|insert|delete|get)$")) @data.flow.map.operation

; 指针操作数据流 - 使用交替模式
[
  (unary_expression
    ["&"] @address.op
    operand: (identifier) @source.variable)
  (unary_expression
    ["*"] @dereference.op
    operand: (identifier) @source.pointer)
] @data.flow.pointer.operation

; 类型转换数据流
(type_conversion_expression
  type: (type_identifier) @target.type
  operand: (identifier) @source.variable) @data.flow.type.conversion

; 复合字面量数据流 - 使用参数化查询
(composite_literal
  type: (type_identifier) @target.type
  body: (literal_value
    (literal_element
      (identifier) @source.element)+)) @data.flow.composite.literal

; 切片操作数据流 - 使用谓词过滤
(slice_expression
  operand: (identifier) @source.array
  low: (_)? @slice.low
  high: (_)? @slice.high
  max: (_)? @slice.max) @data.flow.slice.operation

; 结构体字段访问数据流
(selector_expression
  operand: (identifier) @source.object
  field: (field_identifier) @source.field) @data.flow.field.access

; 数组/切片访问数据流
(index_expression
  operand: (identifier) @source.array
  index: (identifier) @source.index) @data.flow.array.access
`;