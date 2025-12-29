/*
JavaScript Function-specific Tree-Sitter Query Patterns
Extracted from the main javascript.ts file for better maintainability and performance
*/
export default `
; 统一的函数声明查询 - 使用交替模式合并重复查询
[
  (function_declaration
    name: (identifier) @function.name)
  (generator_function_declaration
    name: (identifier) @generator.function.name)
  (arrow_function)
] @definition.function

; 带文档注释的函数声明查询
(
  (comment)* @doc
  .
  [
    (function_declaration
      name: (identifier) @function.name)
    (generator_function_declaration
      name: (identifier) @generator.function.name)
    (lexical_declaration
      (variable_declarator
        name: (identifier) @function.name
        value: [(arrow_function) (function_expression)]) @definition.function)
    (variable_declaration
      (variable_declarator
        name: (identifier) @function.name
        value: [(arrow_function) (function_expression)]))
  ] @definition.function
  (#strip! @doc "^[\\s\\*/]+|^[\\s\\*/]$")
  (#select-adjacent! @doc @definition.function)
)

; Lambda表达式查询
(arrow_function) @definition.lambda

; Test functions - important for test code segmentation
(function_declaration
  name: (identifier) @test.function.name
  (#match? @test.function.name "^(test|it|describe|before|after|beforeEach|afterEach).*")) @definition.test

; 带参数的函数查询 - 提供更多上下文信息
(function_declaration
  name: (identifier) @function.name
  parameters: (formal_parameters
    (required_parameter
      name: (identifier) @param.name)*)
  body: (statement_block) @function.body) @definition.function.with_params

; 箭头函数参数查询
(arrow_function
  parameters: (formal_parameters
    (required_parameter
      name: (identifier) @arrow.param.name)*)?
  body: (_) @arrow.body) @definition.arrow_function.with_params
`;