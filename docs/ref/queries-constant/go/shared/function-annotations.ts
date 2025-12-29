/*
Go Function Annotation Tree-Sitter Query Patterns
Shared queries for function annotations used across entities and relationships
*/
export default `
; 测试函数注解
(function_declaration
  name: (identifier) @test.function
  body: (block) @test.body
  (#match? @test.function "^(Test|Benchmark|Example)")) @function.test.annotation

; 主函数注解
(function_declaration
  name: (identifier) @main.function
  body: (block) @main.body
  (#eq? @main.function "main")) @function.main.annotation

; init函数注解
(function_declaration
  name: (identifier) @init.function
  body: (block) @init.body
  (#eq? @init.function "init")) @function.init.annotation

; 工厂函数注解
(function_declaration
  name: (identifier) @factory.function
  return_type: (type_identifier) @factory.return.type
  (#match? @factory.function "^(New|Create|Make|Build|From|Parse)$")) @function.factory.annotation

; 获取器函数注解
(function_declaration
  name: (identifier) @getter.function
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @getter.receiver))
  (#match? @getter.function "^(Get|Is|Has|Can|Should)$")) @function.getter.annotation

; 设置器函数注解
(function_declaration
  name: (identifier) @setter.function
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @setter.receiver)
    (parameter_declaration
      name: (identifier) @setter.value))
  (#match? @setter.function "^(Set|Add|Remove|Update|Delete)$")) @function.setter.annotation

; 处理器函数注解
(function_declaration
  name: (identifier) @handler.function
  parameters: (parameter_list
    (parameter_declaration
      type: (type_identifier) @handler.param.type))
  (#match? @handler.function "^(Handle|Process|Execute|Run|Serve)$")) @function.handler.annotation

; 回调函数注解
(function_declaration
  name: (identifier) @callback.function
  parameters: (parameter_list
    (parameter_declaration
      type: (function_type) @callback.param.type))
  (#match? @callback.function "^(On|After|Before|When|Do)$")) @function.callback.annotation

; 验证函数注解
(function_declaration
  name: (identifier) @validate.function
  return_type: (type_identifier) @validate.return.type
  (#match? @validate.function "^(Validate|Check|Verify|Ensure)$")) @function.validate.annotation

; 转换函数注解
(function_declaration
  name: (identifier) @convert.function
  parameters: (parameter_list
    (parameter_declaration
      type: (type_identifier) @convert.param.type))
  return_type: (type_identifier) @convert.return.type
  (#match? @convert.function "^(To|From|As|Convert|Transform)$")) @function.convert.annotation
`;