/*
Rust Struct-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的结构体声明查询 - 使用交替模式
[
  (struct_item
    name: (type_identifier) @struct.name)
  (unit_struct_item
    name: (type_identifier) @unit_struct.name)
  (tuple_struct_item
    name: (type_identifier) @tuple_struct.name)
] @definition.struct

; 带字段的结构体查询 - 使用量词操作符
(struct_item
  name: (type_identifier) @struct.name
  body: (field_declaration_list
    (field_declaration
      name: (field_identifier) @field.name
      type: (_) @field.type)*)) @definition.struct.with_fields

; 元组结构体查询 - 使用量词操作符
(tuple_struct_item
  name: (type_identifier) @tuple_struct.name
  body: (field_declaration_list
    (field_declaration
      type: (_) @field.type)*)) @definition.tuple_struct

; 枚举声明查询
(enum_item
  name: (type_identifier) @enum.name
  body: (enum_variant_list
    (enum_variant
      name: (identifier) @enum.variant)*)) @definition.enum

; Trait声明查询
(trait_item
  name: (type_identifier) @trait.name
  body: (declaration_list
    (function_item
      name: (identifier) @trait.method)*)) @definition.trait

; Impl块查询 - 使用交替模式
[
  (impl_item
    type: (type_identifier) @impl.type)
  (impl_item
    trait: (type_identifier) @impl.trait
    type: (type_identifier) @impl.for)
] @definition.impl

; Impl块中的方法查询 - 使用锚点确保精确匹配
(impl_item
  body: (declaration_list
    .
    (function_item
      name: (identifier) @impl.method))) @definition.impl.method

; 联合体声明查询
(union_item
  name: (type_identifier) @union.name
  body: (field_declaration_list
    (field_declaration
      type: (_) @union.field)*)) @definition.union

; 模块声明查询
(mod_item
  name: (identifier) @module.name) @definition.module

; 类型别名查询
(type_item
  name: (type_identifier) @type.alias.name
  type: (_) @type.alias.type) @definition.type.alias

; 常量声明查询
(const_item
  name: (identifier) @const.name
  type: (_) @const.type
  value: (_) @const.value) @definition.constant

; 静态项声明查询
(static_item
  name: (identifier) @static.name
  type: (_) @static.type
  value: (_) @static.value?) @definition.static
`;