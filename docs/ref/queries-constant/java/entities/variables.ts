/*
Java Variable-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 字段声明查询 - 使用量词操作符
(field_declaration
  declarator: (variable_declarator
    name: (identifier) @field.name)
  type: (_) @field.type) @definition.field

; 变量声明查询 - 使用交替模式
(local_variable_declaration
  declarator: (variable_declarator
    name: (identifier) @local.var.name)
  type: (_) @local.var.type) @definition.variable

; 带初始化的变量声明
(local_variable_declaration
  declarator: (variable_declarator
    name: (identifier) @var.name
    value: (_) @var.value)
  type: (_) @var.type) @definition.variable.with.initializer

; 变量声明器查询
(variable_declarator
  name: (identifier) @var.name
  value: (_) @var.value?) @definition.variable.declarator

; 泛型类型查询 - 使用量词操作符
(generic_type
  (type_arguments
    (type_identifier) @generic.arg)+) @definition.generic.type

; 数组类型查询 - 使用量词操作符
(array_type
  (type_identifier) @array.element
  (dimensions) @array.dims) @definition.array.type

; 类型标识符查询 - 使用交替模式
[
  (type_identifier) @type.simple
  (scoped_type_identifier) @type.qualified
  (integral_type) @type.integral
  (floating_point_type) @type.float
  (boolean_type) @type.boolean
  (void_type) @type.void
] @definition.type.identifier

; 字面量查询 - 使用交替模式
[
  (string_literal) @literal.string
  (character_literal) @literal.char
  (decimal_integer_literal) @literal.int
  (hex_integer_literal) @literal.hex
  (decimal_floating_point_literal) @literal.float
  (hex_floating_point_literal) @literal.hex.float
  (true) @literal.true
  (false) @literal.false
  (null_literal) @literal.null
] @definition.literal

; 特殊表达式查询 - 使用交替模式
[
  (this) @expr.this
  (super) @expr.super
  (class_literal
    type: (type_identifier) @class.literal.type)
] @definition.special.expression

; 修饰符查询
(modifiers
  (modifier) @modifier.name) @definition.modifiers
`;