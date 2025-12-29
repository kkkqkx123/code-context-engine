/*
Kotlin Call Expression Tree-Sitter Query Patterns
Shared queries for call expressions used across entities and relationships
*/
export default `
; 函数调用查询 - 使用参数化模式
(call_expression
  function: [
    (simple_identifier) @call.function
    (navigation_expression
      left: (_) @call.receiver
      right: (simple_identifier) @call.method)
  ]
  type_arguments: (type_arguments
    (type) @type.arg)*
  value_arguments: (value_arguments
    (expression) @call.arg)*) @definition.call.expression

; 安全调用查询
(safe_call_expression
  receiver: (_) @safe.receiver
  (navigation_suffix
    (simple_identifier) @safe.method)
  arguments: (value_arguments
    (expression) @safe.arg)*) @definition.safe.call

; 扩展函数调用查询
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @extension.receiver
    right: (simple_identifier) @extension.method)
  arguments: (value_arguments
    (expression) @extension.arg)*) @definition.extension.call

; 泛型函数调用查询
(call_expression
  function: (simple_identifier) @generic.function
  type_arguments: (type_arguments
    (type) @generic.type)*
  arguments: (value_arguments
    (expression) @generic.arg)*) @definition.generic.call

; 中缀调用查询
(infix_expression
  left: (_) @infix.left
  operator: (simple_identifier) @infix.operator
  right: (_) @infix.right) @definition.infix.call

; Lambda调用查询
(call_expression
  function: (simple_identifier) @lambda.function
  arguments: (value_arguments
    (lambda_literal) @lambda.handler)*) @definition.lambda.call

; 协程调用查询 - 使用谓词过滤
(call_expression
  function: (simple_identifier) @coroutine.function
  arguments: (value_arguments
    (lambda_literal) @coroutine.handler)*
  (#match? @coroutine.function "^(launch|async|runBlocking)$")) @definition.coroutine.call

; 流操作调用查询 - 使用谓词过滤
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @flow.object
    right: (simple_identifier) @flow.method)
  arguments: (value_arguments
    (lambda_literal) @flow.handler)*
  (#match? @flow.method "^(collect|emit|transform|filter|map)$")) @definition.flow.call

; 集合操作调用查询 - 使用谓词过滤
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @collection.object
    right: (simple_identifier) @collection.method)
  arguments: (value_arguments
    (expression) @collection.arg)*)
  (#match? @collection.method "^(add|put|set|remove|get|contains|addAll)$")) @definition.collection.call

; 字符串操作调用查询 - 使用谓词过滤
(call_expression
  function: (navigation_expression
    left: (simple_identifier) @string.object
    right: (simple_identifier) @string.method)
  arguments: (value_arguments
    (expression) @string.arg)*)
  (#match? @string.method "^(append|plus|replace|substring|format)$")) @definition.string.call
`;