/*
Rust Call Expression Tree-Sitter Query Patterns
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
  function: (field_expression
    value: (identifier) @call.object
    field: (field_identifier) @call.method)
  arguments: (arguments
    (_) @call.argument)*) @call.expression.method

; 关联函数调用表达式
(call_expression
  function: (field_expression
    value: (type_identifier) @call.type
    field: (field_identifier) @call.associated.function)
  arguments: (arguments
    (_) @call.argument)*) @call.expression.associated

; 泛型函数调用表达式
(call_expression
  function: (identifier) @call.generic.function
  type_arguments: (type_arguments
    (type_identifier) @call.generic.arg)*
  arguments: (arguments
    (_) @call.argument)*) @call.expression.generic

; 递归调用表达式
(call_expression
  function: (identifier) @recursive.call
  arguments: (arguments)) @call.expression.recursive

; 链式调用表达式
(call_expression
  function: (field_expression
    value: (call_expression
      function: (field_expression
        value: (identifier) @chained.call.object
        field: (field_identifier) @chained.call.method)
      arguments: (arguments))
    field: (field_identifier) @chained.call.next.method)
  arguments: (arguments
    (_) @call.argument)*) @call.expression.chained

; 闭包调用表达式
(call_expression
  function: (identifier) @closure.variable
  arguments: (arguments
    (_) @closure.argument)*) @call.expression.closure

; 宏调用表达式
(macro_invocation
  macro: (identifier) @macro.name
  arguments: (token_tree
    (_) @macro.argument)*) @call.expression.macro

; 异步函数调用表达式
(call_expression
  function: (field_expression
    value: (identifier) @async.object
    field: (field_identifier) @async.method)
  arguments: (arguments
    (_) @async.argument)*) @call.expression.async

; 迭代器方法调用表达式
(call_expression
  function: (field_expression
    value: (identifier) @iterator.object
    field: (field_identifier) @iterator.method)
  arguments: (arguments)*
  (#match? @iterator.method "^(iter|iter_mut|into_iter|into_iter_mut|map|filter|fold|collect)$")) @call.expression.iterator
`;