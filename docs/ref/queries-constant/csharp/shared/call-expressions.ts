/*
C# Call Expression Tree-Sitter Query Patterns
Shared queries for call expressions used across entities and relationships
*/
export default `
; 方法调用表达式 - 基础模式
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @call.object
    name: (identifier) @call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.method

; 静态方法调用表达式
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @call.static.class
    name: (identifier) @call.static.method)
 arguments: (argument_list
    (_) @call.argument)*) @call.expression.static

; 函数调用表达式
(invocation_expression
  function: (identifier) @call.function
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.function

; 构造函数调用表达式
(object_creation_expression
 type: (identifier) @call.constructor
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.constructor

; 泛型方法调用表达式
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @call.generic.object
    name: (generic_name
      identifier: (identifier) @call.generic.method))
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.generic

; 链式调用表达式
(invocation_expression
  function: (member_access_expression
    expression: (invocation_expression
      function: (member_access_expression
        expression: (identifier) @chained.call.object
        name: (identifier) @chained.call.method)
      arguments: (argument_list))
    name: (identifier) @chained.call.next.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.chained
`;