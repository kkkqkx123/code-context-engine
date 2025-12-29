/*
Rust Function Annotation Tree-Sitter Query Patterns
Shared queries for function annotations used across entities and relationships
*/
export default `
; 测试函数注解
(function_item
  name: (identifier) @test.function
  body: (block) @test.body
  (#match? @test.function "^(test|should_|it_|when_|then_)$")) @function.test.annotation

; 异步函数注解
(function_item
  (async_modifier) @async.modifier
  name: (identifier) @async.function
  body: (block) @async.body) @function.async.annotation

; 不安全函数注解
(function_item
  (unsafe_modifier) @unsafe.modifier
  name: (identifier) @unsafe.function
  body: (block) @unsafe.body) @function.unsafe.annotation

; 外部函数注解
(function_item
  (extern_modifier) @extern.modifier
  name: (identifier) @extern.function
  parameters: (parameters) @extern.params) @function.extern.annotation

; const函数注解
(function_item
  (const_modifier) @const.modifier
  name: (identifier) @const.function
  body: (block) @const.body) @function.const.annotation

; 工厂函数注解
(function_item
  name: (identifier) @factory.function
  return_type: (type_identifier) @factory.return.type
  (#match? @factory.function "^(new|create|build|make|from|parse|of|value_of)$")) @function.factory.annotation

; 构造器函数注解
(function_item
  name: (identifier) @constructor.function
  return_type: (type_identifier) @constructor.return.type
  (#match? @constructor.function "^(new|default|with_|from_)$")) @function.constructor.annotation

; 获取器函数注解
(function_item
  name: (identifier) @getter.function
  parameters: (parameters
    (parameter
      name: (identifier) @getter.self.param))
  (#match? @getter.function "^(get|is|has|can|should)$")) @function.getter.annotation

; 设置器函数注解
(function_item
  name: (identifier) @setter.function
  parameters: (parameters
    (parameter
      name: (identifier) @setter.self.param)
    (parameter
      name: (identifier) @setter.value))
  (#match? @setter.function "^(set|add|remove|update|delete|with_|mut_)$")) @function.setter.annotation

; 处理器函数注解
(function_item
  name: (identifier) @handler.function
  parameters: (parameters
    (parameter
      type: (type_identifier) @handler.param.type))
  (#match? @handler.function "^(handle|process|execute|run|serve|on_|do_|try_)$")) @function.handler.annotation

; 回调函数注解
(function_item
  name: (identifier) @callback.function
  parameters: (parameters
    (parameter
      type: (function_type) @callback.param.type))
  (#match? @callback.function "^(on_|after|before|when|do|cb_|callback_)$")) @function.callback.annotation

; 验证函数注解
(function_item
  name: (identifier) @validate.function
  return_type: (type_identifier) @validate.return.type
  (#match? @validate.function "^(validate|check|verify|ensure|is_|has_|can_)$")) @function.validate.annotation

; 转换函数注解
(function_item
  name: (identifier) @convert.function
  parameters: (parameters
    (parameter
      type: (type_identifier) @convert.param.type))
  return_type: (type_identifier) @convert.return.type
  (#match? @convert.function "^(to|from|as|convert|transform|into|try_)$")) @function.convert.annotation

; 迭代器函数注解
(function_item
  name: (identifier) @iterator.function
  return_type: (type_identifier) @iterator.return.type
  (#match? @iterator.function "^(iter|iter_mut|into_iter|into_iter_mut)$")) @function.iterator.annotation
`;