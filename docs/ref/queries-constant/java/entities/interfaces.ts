/*
Java Interface-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 接口声明查询
(interface_declaration
  name: (identifier) @interface.name) @definition.interface

; 接口继承关系查询
(interface_declaration
  name: (identifier) @subinterface.interface
  super_interfaces: (super_interfaces
    (type_list
      (type_identifier) @superinterface.interface)+)) @interface.inheritance

; 接口方法声明查询 - 使用量词操作符
(interface_declaration
  body: (interface_body
    (method_declaration
      name: (identifier) @interface.method
      parameters: (formal_parameters
        (formal_parameter
          name: (identifier) @method.param)*)*))) @interface.method

; 接口常量声明查询 - 使用量词操作符
(interface_declaration
  body: (interface_body
    (field_declaration
      (modifiers
        (modifier) @field.modifier)
      declarator: (variable_declarator
        name: (identifier) @interface.constant)
      type: (_) @field.type)*))) @interface.constant

; 注解类型声明查询
(annotation_type_declaration
  name: (identifier) @annotation.name) @definition.annotation.type

; 注解方法声明查询 - 使用量词操作符
(annotation_type_declaration
  body: (annotation_type_body
    (annotation_method_declaration
      name: (identifier) @annotation.method
      type: (_) @annotation.type)*)) @annotation.method

; 类型体查询
(interface_body) @body.interface
(annotation_type_body) @body.annotation

; 修饰符查询
(modifiers
  (modifier) @modifier.name) @definition.modifiers
`;