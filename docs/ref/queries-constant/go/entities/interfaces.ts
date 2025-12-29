/*
Go Interface and Struct-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 接口声明查询 - 使用交替模式
[
  (type_declaration
    (type_spec
      name: (type_identifier) @interface.name
      type: (interface_type
        (method_spec_list
          (method_spec
            name: (field_identifier) @interface.method
            parameters: (parameter_list
              (parameter_declaration
                name: (identifier) @param.name
                type: (_) @param.type)*)?
            result: (_)? @method.return.type)*))))
  (type_declaration
    (type_spec
      name: (type_identifier) @interface.name
      type: (interface_type)))
] @definition.interface

; 结构体声明查询 - 使用交替模式
[
  (type_declaration
    (type_spec
      name: (type_identifier) @struct.name
      type: (struct_type
        (field_declaration_list
          (field_declaration
            name: (field_identifier) @struct.field
            type: (_) @field.type)*))))
  (type_declaration
    (type_spec
      name: (type_identifier) @struct.name
      type: (struct_type)))
] @definition.struct

; 泛型接口查询 - 使用谓词过滤
(type_declaration
  (type_spec
    name: (type_identifier) @generic.interface.name
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @type.parameter))
    type: (interface_type
      (method_spec_list
        (method_spec
          name: (field_identifier) @generic.method
          parameters: (parameter_list
            (parameter_declaration
              name: (identifier) @param.name
              type: (_) @param.type)*)?
          result: (_)? @method.return.type)*)))) @definition.generic.interface

; 泛型结构体查询 - 使用谓词过滤
(type_declaration
  (type_spec
    name: (type_identifier) @generic.struct.name
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @type.parameter))
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @generic.field
          type: (_) @field.type)*)))) @definition.generic.struct

; 嵌入式结构体查询 - 使用锚点确保精确匹配
(type_declaration
  (type_spec
    name: (type_identifier) @embedding.struct.name
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @embedded.type)
        (field_declaration
          name: (field_identifier) @regular.field
          type: (_) @regular.type)*)))) @definition.embedding.struct

; 嵌入式接口查询 - 使用锚点确保精确匹配
(type_declaration
  (type_spec
    name: (type_identifier) @embedding.interface.name
    type: (interface_type
      (method_spec_list
        (method_spec
          name: (field_identifier) @embedded.method)
        (method_spec
          name: (field_identifier) @regular.method
          parameters: (parameter_list
            (parameter_declaration
              name: (identifier) @param.name
              type: (_) @param.type)*)?
          result: (_)? @method.return.type)*)))) @definition.embedding.interface


; 空接口查询 - 使用锚点确保精确匹配
(type_declaration
  (type_spec
    name: (type_identifier) @empty.interface.name
    type: (interface_type))) @definition.empty.interface

; 接口组合查询 - 使用锚点确保精确匹配
(type_declaration
  (type_spec
    name: (type_identifier) @composite.interface.name
    type: (interface_type
      (method_spec_list
        (method_spec
          type: (type_identifier) @embedded.interface)*)))) @definition.interface.composition

; 方法集查询 - 使用交替模式
[
  (method_declaration
    receiver: (parameter_list
      (parameter_declaration
        type: (type_identifier) @value.receiver.type))
    name: (field_identifier) @value.method)
  (method_declaration
    receiver: (parameter_list
      (parameter_declaration
        type: (pointer_type
          element: (type_identifier) @pointer.receiver.type)))
    name: (field_identifier) @pointer.method)
] @definition.method.set


; 接口约束查询 - 使用谓词过滤
(type_declaration
  (type_spec
    name: (type_identifier) @constrained.type
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @type.parameter
        type: (interface_type
          (method_spec_list
            (method_spec
              name: (field_identifier) @constraint.method)*))))) @definition.interface.constraint

; 结构体标签查询 - 使用锚点确保精确匹配
(type_declaration
  (type_spec
    name: (type_identifier) @tagged.struct.name
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @tagged.field
          type: (_) @field.type
          tag: (raw_string_literal) @field.tag)*)))) @definition.struct.tags

; 接口变量声明查询 - 使用锚点确保精确匹配
(var_declaration
  (var_spec
    name: (identifier) @interface.variable
    type: (interface_type))) @definition.interface.variable

; 结构体变量声明查询 - 使用锚点确保精确匹配
(var_declaration
  (var_spec
    name: (identifier) @struct.variable
    type: (struct_type))) @definition.struct.variable

; 结构体字面量查询 - 使用锚点确保精确匹配
(composite_literal
  type: (type_identifier) @literal.struct.type
  body: (literal_value
    (keyed_element
      (literal_element
        (field_identifier) @field.name)
      (literal_element
        (identifier) @field.value))*)) @definition.struct.literal

`;