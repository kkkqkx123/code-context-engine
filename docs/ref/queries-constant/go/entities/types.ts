/*
Go Type-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 类型声明查询 - 使用交替模式
[
  (type_declaration
    (type_spec
      name: (type_identifier) @type.name))
  (type_alias
    name: (type_identifier) @alias.name)
] @definition.type

; 接口和结构体查询 - 使用交替模式
[
  (type_declaration
    (type_spec
      name: (type_identifier) @interface.name
      type: (interface_type)))
  (type_declaration
    (type_spec
      name: (type_identifier) @struct.name
      type: (struct_type)))
] @definition.composite_type

; 泛型类型查询 - 使用谓词过滤
(type_declaration
  (type_spec
    name: (type_identifier) @generic.name
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @type.param))))
  (#match? @generic.name "^[A-Z][a-zA-Z0-9]*$") @definition.generic_type

; 字段声明查询 - 使用量词操作符
(field_declaration
  name: (field_identifier) @field.name
  type: (_) @field.type) @definition.field

; 限定类型查询 - 使用锚点确保精确匹配
(qualified_type
  package: (package_identifier) @qualified.package
  name: (type_identifier) @qualified.name) @definition.qualified.type

; 数组和切片类型查询 - 使用交替模式
[
  (array_type
    length: (_) @array.length
    element: (_) @array.element)
  (slice_type
    element: (_) @slice.element)
] @definition.collection.type

; 映射和函数类型查询 - 使用交替模式
[
  (map_type
    key: (_) @map.key
    value: (_) @map.value)
  (function_type
    parameters: (parameter_list) @func.params
    result: (_)? @func.result)
] @definition.complex.type

; 指针类型查询
(pointer_type
  element: (_) @pointer.element) @definition.pointer.type

; 类型标识符查询 - 使用交替模式
[
  (type_identifier) @type.identifier
  (package_identifier) @package.identifier
  (field_identifier) @field.identifier
] @definition.identifier
`;