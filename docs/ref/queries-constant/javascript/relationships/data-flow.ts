/*
JavaScript Data Flow-specific Tree-Sitter Query Patterns
用于追踪变量间的数据传递关系
*/
export default `
; 统一的赋值数据流查询 - 使用交替模式
(assignment_expression
  left: [
    (identifier) @target.variable
    (member_expression
      object: (identifier) @target.object
      property: (property_identifier) @target.property)
    (subscript_expression
      object: (identifier) @target.array
      index: (identifier) @target.index)
  ]
  right: [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (binary_expression
      left: (_) @binary.left
      right: (_) @binary.right)
  ]) @data.flow.assignment

; 解构赋值数据流 - 使用交替模式
(assignment_expression
  left: [
    (object_pattern
      (pair_pattern
        key: (property_identifier) @source.property
        value: (identifier) @target.variable)*)
    (array_pattern
      (identifier) @target.variable*)
  ]
  right: (identifier) @source.object) @data.flow.destructuring

; 函数调用数据流 - 参数化查询
(call_expression
  function: [
    (identifier) @target.function
    (member_expression
      object: (identifier) @target.object
      property: (property_identifier) @target.method)
  ]
  arguments: (argument_list
    (identifier) @source.parameter)*) @data.flow.parameter.passing

; 返回值数据流 - 使用锚点操作符
(return_statement
  .
  [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (member_expression
      object: (identifier) @source.object
      property: (property_identifier) @source.property)
  ]) @data.flow.return.value

; 函数表达式赋值数据流
(assignment_expression
  left: (identifier) @source.variable
  right: [
    (function_expression) @target.function
    (arrow_function) @target.function
  ]) @data.flow.function.assignment

; 链式调用数据流
(call_expression
  function: (member_expression
    object: (call_expression) @source.call
    property: (property_identifier) @target.method)) @data.flow.chained.call

; Promise异步数据流
(assignment_expression
  left: (identifier) @target.variable
  right: (await_expression
    value: (identifier) @source.promise)) @data.flow.async.await
`;