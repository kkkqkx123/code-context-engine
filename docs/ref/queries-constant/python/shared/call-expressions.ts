/*
Python Call Expression Tree-Sitter Query Patterns
Shared queries for call expressions used across entities and relationships
*/
export default `
; 函数调用表达式 - 基础模式
(call
  function: (identifier) @call.function
  arguments: (argument_list
    (_) @call.argument)*) @call.expression

; 方法调用表达式
(call
  function: (attribute
    object: (identifier) @call.object
    attribute: (identifier) @call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.method

; 链式调用表达式
(call
  function: (attribute
    object: (call
      function: (identifier) @chained.call.function
      arguments: (argument_list))
    attribute: (identifier) @chained.call.method)
  arguments: (argument_list
    (_) @call.argument)*) @call.expression.chained

; 递归调用表达式
(call
  function: (identifier) @recursive.call
  arguments: (argument_list)) @call.expression.recursive

; 类实例化调用表达式
(call
  function: (identifier) @instantiated.class
  arguments: (argument_list
    (_) @constructor.parameter)*) @call.expression.instantiation

; 生成器函数调用
(call
  function: (identifier) @generator.function
  arguments: (argument_list
    (function_definition
      name: (identifier) @generator.definition))) @call.expression.generator

; 资源管理调用
(call
  function: (attribute
    object: (identifier) @resource.object
    attribute: (identifier) @resource.method
    (#match? @resource.method "^(acquire|open|start|connect|release|close|stop|disconnect|cleanup)$"))) @call.expression.resource.management

; 装饰器调用表达式
(decorator
  (call
    function: (identifier) @decorator.function
    arguments: (argument_list
      (_) @decorator.argument)*) @decorator.call) @decorator.expression
`;