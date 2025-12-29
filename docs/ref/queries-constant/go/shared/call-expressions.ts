/*
Go Call Expression Tree-Sitter Query Patterns
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
  function: (selector_expression
    operand: (identifier) @call.object
    field: (field_identifier) @call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.method

; 包方法调用表达式
(call_expression
  function: (selector_expression
    operand: (package_identifier) @call.package
    field: (identifier) @call.package.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.package

; 递归调用表达式
(call_expression
  function: (identifier) @recursive.call
  arguments: (argument_list)) @call.expression.recursive

; 链式调用表达式
(call_expression
  function: (selector_expression
    operand: (call_expression
      function: (selector_expression
        operand: (identifier) @chained.call.object
        field: (field_identifier) @chained.call.method)
      arguments: (argument_list))
    field: (field_identifier) @chained.call.next.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.chained

; 异步调用表达式
(go_statement
  (call_expression
    function: (identifier) @go.function
    arguments: (argument_list
      (_) @go.argument)*)) @call.expression.go

; 延迟调用表达式
(defer_statement
  (call_expression
    function: (identifier) @defer.function
    arguments: (argument_list
      (_) @defer.argument)*)) @call.expression.defer
`;