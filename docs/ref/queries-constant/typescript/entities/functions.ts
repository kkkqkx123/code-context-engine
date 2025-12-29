/*
TypeScript Function-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的函数声明查询 - 使用交替模式合并重复查询
[
  (function_declaration
    name: (identifier) @function.name)
  (arrow_function)
  (method_definition
    name: (property_identifier) @method.name)
] @definition.function

; Async functions - important for understanding async flow
(function_declaration
  "async"
  name: (identifier) @async.function.name) @definition.async_function

; Generic function declarations - important for type understanding
(function_declaration
  type_parameters: (type_parameters)
  name: (identifier) @generic.function.name) @definition.generic_function

; Test functions - important for test code segmentation
(function_declaration
  name: (identifier) @test.function.name
  (#match? @test.function.name "^(test|it|describe|before|after|beforeEach|afterEach).*")) @definition.test

; React Hook functions - important for React component structure
(function_declaration
  name: (identifier) @hook.function.name
  (#match? @hook.function.name "^use[A-Z].*")) @definition.hook

; 函数参数查询 - 提供更多上下文信息
(function_declaration
  name: (identifier) @function.name
  parameters: (required_parameter
    name: (identifier) @param.name
    type: (type_annotation)? @param.type)*
  return_type: (type_annotation)? @return.type) @definition.function.with_params

; 箭头函数参数查询
(arrow_function
  parameters: (required_parameter
    name: (identifier) @arrow.param.name
    type: (type_annotation)? @arrow.param.type)*
  return_type: (type_annotation)? @arrow.return.type) @definition.arrow_function.with_params
`;