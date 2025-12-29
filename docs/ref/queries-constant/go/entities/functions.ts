/*
Go Function-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的函数和方法查询 - 使用交替模式
[
  (function_declaration
    name: (identifier) @function.name)
  (method_declaration
    name: (field_identifier) @method.name)
] @definition.function

; 带参数的函数查询 - 使用量词操作符
(function_declaration
  name: (identifier) @function.name
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @param.name)*)
  body: (block) @function.body) @definition.function.with_params

; 函数字面量查询
(func_literal
  body: (block) @lambda.body) @definition.lambda

; 测试函数查询 - 使用谓词过滤
(function_declaration
  name: (identifier) @test.name
  (#match? @test.name "^(Test|Benchmark|Example)"))
  body: (block) @test.body) @definition.test_function

; 包声明查询
(package_clause
  name: (package_identifier) @package.name) @definition.package

; 导入声明查询
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @import.path)) @definition.import
`;