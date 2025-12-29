/*
Java Import/Export Tree-Sitter Query Patterns
Shared queries for module imports and exports
*/
export default `
; 模块和包声明查询 - 使用交替模式
[
  (module_declaration
    name: (scoped_identifier) @module.name)
  (package_declaration
    name: (scoped_identifier) @package.name)
] @definition.namespace

; 导入声明查询 - 使用交替模式
[
  (import_declaration
    (scoped_identifier) @import.name)
  (import_declaration
    (identifier) @import.static
    (asterisk) @import.wildcard)
] @definition.import

; 别名导入查询
(import_declaration
  (scoped_identifier) @import.name
  "as"
  (identifier) @import.alias) @definition.import.alias

; 通配符导入查询
(import_declaration
  (scoped_identifier) @import.package
  "."
  "*"
  (identifier)? @import.wildcard) @definition.import.wildcard

; 静态导入查询
(import_declaration
  "static"
  (scoped_identifier) @import.static.class
  "."
  (identifier) @import.static.member) @definition.import.static

; 静态通配符导入查询
(import_declaration
  "static"
  (scoped_identifier) @import.static.class
  "."
  "*") @definition.import.static.wildcard

; 类型标识符查询 - 使用交替模式
[
  (type_identifier) @type.simple
  (scoped_identifier) @type.qualified
  (generic_type) @type.generic
] @definition.type.identifier

; 修饰符查询
(modifiers
  (modifier) @modifier.name) @definition.modifiers

; 注解查询 - 使用交替模式
[
  (annotation
    name: (identifier) @annotation.name)
  (marker_annotation
    name: (identifier) @marker.annotation)
] @definition.annotation

; 字符串相关查询 - 使用交替模式
[
  (string_literal) @literal.string
  (string_fragment) @string.fragment
  (escape_sequence) @string.escape
] @definition.string.literal

; 数值字面量查询 - 使用交替模式
[
  (decimal_integer_literal) @literal.decimal.int
  (hex_integer_literal) @literal.hex.int
  (octal_integer_literal) @literal.octal.int
  (binary_integer_literal) @literal.binary.int
  (decimal_floating_point_literal) @literal.decimal.float
  (hex_floating_point_literal) @literal.hex.float
] @definition.numeric.literal

; 布尔和空值字面量查询 - 使用交替模式
[
  (true) @literal.true
  (false) @literal.false
  (null_literal) @literal.null
] @definition.boolean.literal

; 字符字面量查询
(character_literal) @literal.character

; 标识符查询
(identifier) @definition.identifier
`;