/*
JavaScript Function Annotation Tree-Sitter Query Patterns
Shared queries for function annotations used across entities and relationships
*/
export default `
; 异步函数注解
(function_declaration
  "async"
  name: (identifier) @async.function.name
  body: (statement_block) @async.function.body) @function.async.annotation

; 生成器函数注解
(generator_function_declaration
  "*"
  name: (identifier) @generator.function.name
  body: (statement_block) @generator.function.body) @function.generator.annotation

; 方法注解 - 异步方法
(method_definition
  "async"
  name: (property_identifier) @async.method.name
  body: (statement_block) @async.method.body) @method.async.annotation

; 方法注解 - 生成器方法
(generator_method_definition
  "*"
  name: (property_identifier) @generator.method.name
  body: (statement_block) @generator.method.body) @method.generator.annotation

; 静态方法注解
(method_definition
  "static"
  name: (property_identifier) @static.method.name
  body: (statement_block) @static.method.body) @method.static.annotation

; 私有方法注解
(method_definition
  name: (private_property_identifier) @private.method.name
  body: (statement_block) @private.method.body) @method.private.annotation

; 箭头函数注解
(arrow_function
  "async"
  parameters: (formal_parameters) @arrow.params
  body: (_) @arrow.body) @arrow.async.annotation

; 函数表达式注解
(assignment_expression
  left: (identifier) @function.name
  right: (function_expression
    parameters: (formal_parameters) @func.params
    body: (statement_block) @func.body)) @function.expression.annotation

; 异步函数表达式注解
(assignment_expression
  left: (identifier) @async.function.name
  right: (async_function_expression
    parameters: (formal_parameters) @async.func.params
    body: (statement_block) @async.func.body)) @async.function.expression.annotation

; 高阶函数注解
(call_expression
  function: (identifier) @higher.order.function
  arguments: (argument_list
    (function_expression) @callback.function
    (arrow_function) @callback.arrow)*) @higher.order.annotation

; React Hook函数注解
(function_declaration
  name: (identifier) @hook.function.name
  (#match? @hook.function.name "^use[A-Z].*")
  body: (statement_block) @hook.body) @hook.function.annotation

; React组件函数注解
(function_declaration
  name: (identifier) @component.function.name
  (#match? @component.function.name "^[A-Z][a-zA-Z]*$")
  body: (statement_block) @component.body) @component.function.annotation
`;