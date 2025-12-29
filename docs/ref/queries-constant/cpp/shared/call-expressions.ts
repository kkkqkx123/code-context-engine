/*
C++ Call Expression Tree-Sitter Query Patterns
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
  function: (field_expression
    argument: (identifier) @call.object
    field: (field_identifier) @call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.method

; 模板函数调用表达式
(call_expression
  function: (template_function
    name: (identifier) @call.template.function
    arguments: (template_argument_list)) @call.expression.template

; Lambda 调用表达式
(call_expression
  function: (lambda_expression) @call.lambda
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.lambda

; 递归调用表达式
(call_expression
  function: (identifier) @recursive.call
  arguments: (argument_list)) @call.expression.recursive

; 链式调用表达式
(call_expression
  function: (field_expression
    argument: (call_expression
      function: (identifier) @chained.call.function
      arguments: (argument_list))
    field: (field_identifier) @chained.call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.chained
`;