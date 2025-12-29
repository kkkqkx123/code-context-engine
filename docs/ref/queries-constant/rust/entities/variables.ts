/*
Rust Variable-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 常量声明查询
(const_item
  name: (identifier) @const.name
  type: (_) @const.type
  value: (_) @const.value) @definition.constant

; 静态项声明查询
(static_item
  name: (identifier) @static.name
  type: (_) @static.type
  value: (_) @static.value?
  mutability: (mut)? @static.mutability) @definition.static

; 变量绑定查询 - 使用锚点确保精确匹配
(let_statement
  pattern: (identifier) @variable.name
  type: (type)? @variable.type
  value: (_) @variable.value) @definition.variable

; 赋值表达式查询
(assignment_expression
  left: (identifier) @assign.target
  right: (_) @assign.source) @definition.assignment

; Self参数查询
(self_parameter) @definition.self.parameter

; 模式查询 - 使用交替模式
[
  (identifier_pattern) @pattern.identifier
  (wildcard_pattern) @pattern.wildcard
  (reference_pattern) @pattern.reference
] @definition.pattern

; 字面量查询 - 使用交替模式
[
  (integer_literal) @literal.integer
  (float_literal) @literal.float
  (string_literal) @literal.string
  (character_literal) @literal.char
  (boolean_literal) @literal.boolean
  (unit_expression) @literal.unit
] @definition.literal

; Self和Super关键字查询 - 使用交替模式
[
  (self) @keyword.self
  (super) @keyword.super
] @definition.keyword
`;