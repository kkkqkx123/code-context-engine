/*
Go Variable-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的变量声明查询 - 使用交替模式
[
  (var_declaration
    (var_spec
      name: (identifier) @var.name
      type: (_)? @var.type
      value: (_)? @var.value))
  (short_var_declaration
    left: (expression_list
      (identifier) @short_var.name)
    right: (expression_list
      (identifier) @short_var.value))
] @definition.variable

; 常量声明查询
(const_declaration
  (const_spec
    name: (identifier) @const.name
    type: (_)? @const.type
    value: (_)? @const.value)) @definition.constant

; 参数声明查询 - 使用量词操作符
(parameter_declaration
  name: (identifier) @param.name
  type: (_) @param.type) @definition.parameter

; 可变参数声明查询
(variadic_parameter_declaration
  name: (identifier) @variadic.name
  type: (_) @variadic.type) @definition.variadic

; 特殊标识符查询 - 使用谓词过滤
(identifier) @blank.identifier
  (#eq? @blank.identifier "_")

(identifier) @iota.identifier
  (#eq? @iota.identifier "iota")
`;