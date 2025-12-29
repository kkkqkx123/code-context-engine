/*
Kotlin Import/Export Tree-Sitter Query Patterns
Shared queries for module imports and exports
*/
export default `
; 包声明查询
(package_header
  (identifier) @package.name) @definition.package

; 导入查询 - 使用交替模式
[
  (import_header
    (identifier) @import.name)
  (import_header
    (identifier) @import.alias
    (identifier) @import.name)
] @definition.import

; 别名导入查询
(import_header
  (identifier) @import.alias
  "as"
  (identifier) @import.name) @definition.import.alias

; 通配符导入查询
(import_header
  (identifier) @import.package
  "."
  "*"
  (identifier)? @import.wildcard) @definition.import.wildcard

; 类型导入查询
(import_header
  (identifier) @import.package
  "."
  (identifier) @import.type) @definition.import.type

; 函数导入查询
(import_header
  (identifier) @import.package
  "."
  (identifier) @import.function) @definition.import.function

; 属性导入查询
(import_header
  (identifier) @import.package
  "."
  (identifier) @import.property) @definition.import.property

; 可见性修饰符查询 - 使用量词操作符
(modifiers
  [
    (visibility_modifier) @visibility.modifier
    (class_modifier) @class.modifier
    (function_modifier) @function.modifier
    (property_modifier) @property.modifier
    (inheritance_modifier) @inheritance.modifier
    (parameter_modifier) @parameter.modifier
    (type_modifier) @type.modifier
  ]+) @definition.modifiers

; 注解查询 - 使用量词操作符
(annotation
  (user_type
    (simple_identifier) @annotation.name)
  (arguments
    (argument
      (simple_identifier) @annotation.arg
      (_)? @annotation.value)*)?) @definition.annotation

; 文件结构查询 - 使用交替模式
[
  (kotlinFile) @file.kotlin
  (script) @file.script
  (source_file) @file.source
] @definition.file

; 类型标识符查询 - 使用交替模式
[
  (simple_identifier) @identifier.simple
  (type_identifier) @identifier.type
  (user_type) @identifier.user
] @definition.identifier

; 类型查询 - 使用交替模式
[
  (type) @type.simple
  (nullable_type) @type.nullable
  (function_type) @type.function
  (generic_type) @type.generic
  (user_type) @type.user
] @definition.type
`;