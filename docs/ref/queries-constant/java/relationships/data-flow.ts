/*
Java Data Flow-specific Tree-Sitter Query Patterns
用于追踪变量间的数据传递关系
*/
export default `
; 统一的赋值数据流查询 - 使用交替模式
(assignment_expression
  left: [
    (identifier) @target.variable
    (field_access
      object: (identifier) @target.object
      field: (identifier) @target.field)
    (array_access
      array: (identifier) @target.array
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

; 方法调用参数传递数据流 - 使用参数化查询
(method_invocation
  name: [
    (identifier) @target.method
    (field_access
      field: (identifier) @target.method)
  ]
  arguments: (argument_list
    (identifier) @source.parameter)+) @data.flow.parameter.passing

; 对象方法调用数据流 - 使用锚点确保精确匹配
(method_invocation
  function: (field_access
    object: (identifier) @target.object
    field: (identifier) @target.method)
  arguments: (argument_list
    (identifier) @source.parameter)+) @data.flow.method.parameter

; 返回值数据流 - 使用锚点操作符
(return_statement
  .
  [
    (identifier) @source.variable
    (field_access
      object: (identifier) @source.object
      field: (identifier) @source.field)
    (call_expression
      function: (identifier) @source.function)
  ]) @data.flow.return.value

; 构造函数调用数据流 - 使用参数化查询
(object_creation_expression
  type: (type_identifier) @target.class
  arguments: (argument_list
    (identifier) @source.parameter)+) @data.flow.constructor.parameter

; Lambda表达式数据流 - 使用锚点确保精确匹配
(assignment_expression
  left: (identifier) @target.variable
  right: (lambda_expression) @source.lambda) @data.flow.lambda.assignment

; 泛型方法调用数据流 - 使用量词操作符
(method_invocation
  name: (identifier) @target.method
  type_arguments: (type_arguments
    (type_identifier) @type.arg)+
  arguments: (argument_list
    (identifier) @source.parameter)+) @data.flow.generic.parameter

; 静态方法调用数据流 - 使用谓词过滤
(method_invocation
  function: (scoped_identifier
    scope: (identifier) @target.class
    name: (identifier) @target.method)
  arguments: (argument_list
    (identifier) @source.parameter)+) @data.flow.static.parameter

; 数组访问数据流
(array_access
  array: (identifier) @source.array
  index: (identifier) @source.index) @data.flow.array.access

; 类型转换数据流
(cast_expression
  value: (identifier) @source.variable
  type: (type_identifier) @target.type) @data.flow.type.conversion

; 增强for循环数据流 - 使用锚点确保精确匹配
(enhanced_for_statement
  name: (identifier) @target.variable
  value: (identifier) @source.iterable
  body: (statement) @loop.body) @data.flow.enhanced.for

; 变量初始化数据流 - 使用交替模式
[
  (local_variable_declaration
    declarator: (variable_declarator
      name: (identifier) @target.variable
      value: (identifier) @source.value))
  (field_declaration
    declarator: (variable_declarator
      name: (identifier) @target.field
      value: (identifier) @source.value))
] @data.flow.variable.initialization

; 集合操作数据流 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @target.collection
    field: (identifier) @collection.method)
  arguments: (argument_list
    (identifier) @source.parameter)*)
  (#match? @collection.method "^(add|put|set|remove|get|contains|addAll)$")) @data.flow.collection.operation

; 字符串操作数据流 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @target.string
    field: (identifier) @string.method)
  arguments: (argument_list
    (identifier) @source.parameter)*)
  (#match? @string.method "^(concat|substring|replace|format|valueOf)$")) @data.flow.string.operation

; 流操作数据流 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @target.stream
    field: (identifier) @stream.method)
  arguments: (argument_list
    (identifier) @source.parameter)*)
  (#match? @stream.method "^(map|filter|flatMap|collect|reduce|forEach)$")) @data.flow.stream.operation

; 异常处理数据流 - 使用锚点确保精确匹配
(catch_clause
  parameter: (catch_formal_parameter
    name: (identifier) @caught.exception)
  body: (block
    (expression_statement
      (identifier) @used.variable))) @data.flow.exception.handling

; 同步块数据流
(synchronized_statement
  (parenthesized_expression
    (identifier) @lock.object)
  (block
    (expression_statement
      (identifier) @protected.variable))) @data.flow.synchronization
`;