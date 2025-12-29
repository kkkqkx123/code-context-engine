/*
JavaScript Call Expression Tree-Sitter Query Patterns
Shared queries for call expressions used across entities and relationships
*/
export default `
; 函数调用表达式 - 基础模式
(call_expression
  function: (identifier) @call.function
  arguments: (argument_list
    (_) @call.argument)*) @call.expression

; 方法调用表达式
(call_expression
  function: (member_expression
    object: (identifier) @call.object
    property: (property_identifier) @call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.method

; 箭头函数调用表达式
(call_expression
  function: (arrow_function) @call.arrow
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.arrow

; 递归调用表达式
(call_expression
  function: (identifier) @recursive.call
  arguments: (argument_list)) @call.expression.recursive

; 链式调用表达式
(call_expression
  function: (member_expression
    argument: (call_expression
      function: (identifier) @chained.call.function
      arguments: (argument_list))
    property: (property_identifier) @chained.call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.chained

; 可选链调用表达式
(call_expression
  function: (optional_chain
    object: (identifier) @optional.object
    property: (property_identifier) @optional.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.optional

; 构造函数调用表达式
(new_expression
  constructor: (identifier) @constructor.function
  arguments: (argument_list
    (_) @constructor.argument)*) @constructor.expression

; 异步函数调用表达式
(call_expression
  function: (identifier) @async.call.function
  arguments: (argument_list
    (_) @async.call.argument)*) 
(await_expression
  value: (call_expression
    function: (identifier) @await.function)) @async.call.expression

; Promise链式调用
(call_expression
  function: (member_expression
    object: (identifier) @promise.object
    property: (property_identifier) @promise.method
    (#match? @promise.method "^(then|catch|finally)$"))
  arguments: (argument_list
    (function_expression) @promise.handler)*) @promise.chain.expression
`;