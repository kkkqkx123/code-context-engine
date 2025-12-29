/*
TypeScript Call Expression Tree-Sitter Query Patterns
Shared queries for call expressions used across entities and relationships
*/
export default `
; 函数调用表达式 - 基础模式
(call_expression
  function: (identifier) @call.function
  arguments: (arguments
    (_) @call.argument)*) @call.expression

; 方法调用表达式
(call_expression
  function: (member_expression
    object: (identifier) @call.object
    property: (property_identifier) @call.method)
  arguments: (arguments
    (_) @call.argument)*) @call.expression.method

; 泛型函数调用表达式
(call_expression
  function: (generic_type
    name: (identifier) @call.generic.function
    type_arguments: (type_arguments)) @call.expression.generic

; 箭头函数调用表达式
(call_expression
  function: (arrow_function) @call.arrow
  arguments: (arguments
    (_) @call.argument)*) @call.expression.arrow

; 递归调用表达式
(call_expression
  function: (identifier) @recursive.call
  arguments: (arguments)) @call.expression.recursive

; 链式调用表达式
(call_expression
  function: (member_expression
    argument: (call_expression
      function: (identifier) @chained.call.function
      arguments: (arguments))
    property: (property_identifier) @chained.call.method)
  arguments: (arguments
    (_) @call.argument)*) @call.expression.chained

; 可选链调用表达式
(call_expression
  function: (optional_chain
    object: (identifier) @optional.object
    property: (property_identifier) @optional.method)
  arguments: (arguments
    (_) @call.argument)*) @call.expression.optional

; 构造函数调用表达式
(new_expression
  constructor: (identifier) @constructor.function
  arguments: (arguments
    (_) @constructor.argument)*) @constructor.expression

; 异步函数调用表达式
(call_expression
  function: (identifier) @async.call.function
  arguments: (arguments
    (_) @async.call.argument)*) 
(await_expression
  value: (call_expression
    function: (identifier) @await.function)) @async.call.expression
`;