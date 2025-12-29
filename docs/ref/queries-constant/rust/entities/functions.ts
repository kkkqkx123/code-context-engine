/*
Rust Function-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的函数声明查询 - 使用交替模式
[
  (function_item
    name: (identifier) @function.name)
  (function_signature_item
    name: (identifier) @function.name)
] @definition.function

; 带参数的函数查询 - 使用量词操作符
(function_item
  name: (identifier) @function.name
  parameters: (parameters
    (parameter
      name: (identifier) @param.name
      type: (_) @param.type)*)
  body: (block) @function.body) @definition.function.with_params

; 异步函数查询 - 使用谓词过滤
(function_item
  (async_modifier) @async.modifier
  name: (identifier) @async.function)
  body: (block) @async.body) @definition.async.function

; 闭包表达式查询
(closure_expression
  parameters: (closure_parameters
    (parameter
      name: (identifier) @closure.param)*)
  body: (_) @closure.body) @definition.closure

; 异步块查询
(async_block
  body: (block) @async.body) @definition.async.block

; 泛型参数查询 - 使用量词操作符
(function_item
  name: (identifier) @generic.function
  type_parameters: (type_parameters
    (type_parameter
      name: (type_identifier) @type.param)*)
  body: (block) @generic.body) @definition.generic.function

; 生命周期参数查询 - 使用量词操作符
(function_item
  name: (identifier) @lifetime.function
  parameters: (parameters
    (parameter
      name: (identifier) @param.name
      type: (reference_type
        lifetime: (lifetime) @param.lifetime)*)*)
  body: (block) @lifetime.body) @definition.lifetime.function

; 可见性修饰符查询 - 使用量词操作符
(function_item
  (visibility_modifier) @visibility.modifier
  name: (identifier) @visible.function) @definition.visible.function

; 属性查询 - 使用量词操作符
(function_item
  (attribute_item
    (attribute) @attribute)*
  name: (identifier) @attributed.function) @definition.attributed.function
`;