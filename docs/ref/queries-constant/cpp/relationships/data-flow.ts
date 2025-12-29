/*
C++ Data Flow-specific Tree-Sitter Query Patterns
用于追踪变量间的数据传递关系
Optimized based on tree-sitter best practices
*/
export default `
; 统一的赋值数据流查询 - 使用交替模式
(assignment_expression
  left: [
    (identifier) @target.variable
    (field_expression
      object: (identifier) @target.object
      field: (field_identifier) @target.field)
    (subscript_expression
      argument: (identifier) @target.array
      index: (identifier) @target.index)
    (dereference_expression
      operand: (identifier) @target.pointer)
  ]
  right: [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (binary_expression
      left: (_) @binary.left
      right: (_) @binary.right)
  ]) @data.flow.assignment

; 返回值数据流 - 使用锚点操作符
(return_statement
  .
  [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (field_expression
      object: (identifier) @source.object
      field: (field_identifier) @source.field)
  ]) @data.flow.return.value

; 智能指针数据流 - 使用谓词过滤
(assignment_expression
  left: (identifier) @target.smart.ptr
  right: (call_expression
    function: (qualified_identifier
      scope: (identifier) @std.namespace
      name: (identifier) @make.function)
    arguments: (argument_list
      (identifier) @source.parameter))
  (#match? @make.function "^(make_unique|make_shared)$")
  (#eq? @std.namespace "std")) @data.flow.smart.pointer.creation

; 类型转换数据流 - 简化模式
(cast_expression
  type: (type_identifier) @target.type
  value: (identifier) @source.variable) @data.flow.type.conversion

; STL 容器操作数据流 - 使用谓词过滤
(call_expression
  function: (field_expression
    object: (identifier) @source.container
    field: (field_identifier) @container.method)
  arguments: (argument_list
    (identifier) @target.parameter)?)
  (#match? @container.method "^(push_back|push_front|insert|emplace|pop_back|pop_front|erase|clear)$")) @data.flow.container.operation

; 移动语义数据流 - 使用谓词过滤
(call_expression
  function: (identifier) @move.function
  arguments: (argument_list
    (identifier) @source.moved.variable))
  (#match? @move.function "^(move|forward)$")) @data.flow.move.semantics

; 结构化绑定数据流 - 使用锚点确保精确匹配
(declaration
  declarator: (structured_binding_declarator
    (identifier) @target.binding)
  value: (identifier) @source.variable) @data.flow.structured.binding

; 初始化列表数据流 - 简化模式
(call_expression
  function: (qualified_identifier
    scope: (identifier) @target.class
    name: (identifier) @target.constructor)
  arguments: (argument_list
    (initializer_list
      (identifier) @source.parameter))) @data.flow.initializer.list

; Lambda捕获数据流 - 使用锚点和量词操作符
(lambda_expression
  capture_list: (capture_list
    (capture_default
      (identifier) @capture.source)?
    (capture
      (identifier) @captured.variable)*)) @data.flow.lambda.capture

; 范围for循环数据流 - 使用字段名和锚点
(range_based_for_statement
  declarator: (identifier) @target.variable
  range: (identifier) @source.range) @data.flow.range.for

; 迭代器数据流 - 使用谓词过滤
(call_expression
  function: (field_expression
    object: (identifier) @source.container
    field: (field_identifier) @iterator.method)
  arguments: (argument_list)?)
  (#match? @iterator.method "^(begin|end|rbegin|rend|cbegin|cend)$")) @data.flow.iterator.operation

; 流操作数据流 - 使用谓词过滤
(call_expression
  function: (field_expression
    object: (identifier) @source.stream
    field: (field_identifier) @stream.method)
  arguments: (argument_list
    (identifier) @target.parameter))
  (#match? @stream.method "^(operator<<|operator>>)$")) @data.flow.stream.operation

; 静态转换数据流 - 使用谓词过滤
(call_expression
  function: (qualified_identifier
    scope: (identifier) @std.scope
    name: (identifier) @cast.function)
  arguments: (argument_list
    (identifier) @source.variable
    (type_identifier) @target.type))
  (#eq? @std.scope "std")
  (#match? @cast.function "^(static_cast|dynamic_cast|reinterpret_cast|const_cast)$")) @data.flow.cast.operation
`;