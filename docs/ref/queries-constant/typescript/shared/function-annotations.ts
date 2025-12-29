/*
TypeScript Function Annotation Tree-Sitter Query Patterns
Shared queries for function annotations used across entities and relationships
*/
export default `
; 函数注解 - 装饰器在函数定义中
(function_declaration
  (decorator)
  name: (identifier) @function.name
  body: (statement_block) @function.body) @function.definition.with.annotation

; 异步函数注解
(function_declaration
  "async"
  name: (identifier) @async.function.name
  body: (statement_block) @async.function.body) @function.async.annotation

; 泛型函数注解
(function_declaration
  type_parameters: (type_parameters)
  name: (identifier) @generic.function.name
  body: (statement_block) @generic.function.body) @function.generic.annotation

; 方法注解 - 装饰器在方法定义中
(method_definition
  (decorator)
  name: (property_identifier) @method.name
  body: (statement_block) @method.body) @method.definition.with.annotation

; 异步方法注解
(method_definition
  "async"
  name: (property_identifier) @async.method.name
  body: (statement_block) @async.method.body) @method.async.annotation

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

; 函数类型注解
(function_type
  parameters: (formal_parameters) @type.params
  return_type: (type_annotation) @type.return) @function.type.annotation

; 泛型函数类型注解
(function_type
  type_parameters: (type_parameters)
  parameters: (formal_parameters) @generic.type.params
  return_type: (type_annotation) @generic.type.return) @generic.function.type.annotation

; 重载函数注解
(overload_signature
  name: (identifier) @overload.function.name) @function.overload.annotation

; 重载方法注解
(overload_signature
  name: (property_identifier) @overload.method.name) @method.overload.annotation
`;