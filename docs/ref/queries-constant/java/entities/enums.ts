/*
Java Enum-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 枚举声明查询
(enum_declaration
  name: (identifier) @enum.name) @definition.enum

; 枚举常量查询
(enum_declaration
  body: (enum_body
    (enum_constant
      name: (identifier) @enum.constant))) @definition.enum.constant

; 带参数的枚举常量查询
(enum_declaration
  body: (enum_body
    (enum_constant
      name: (identifier) @enum.constant
      arguments: (argument_list
        (expression) @enum.arg)*))) @definition.enum.constant.with.args

; 带类体的枚举常量查询
(enum_declaration
  body: (enum_body
    (enum_constant
      name: (identifier) @enum.constant
      body: (class_body) @enum.body)*)) @definition.enum.constant.with.body

; 枚举方法声明查询 - 使用量词操作符
(enum_declaration
  body: (enum_body
    (method_declaration
      name: (identifier) @enum.method
      parameters: (formal_parameters
        (formal_parameter
          name: (identifier) @method.param)*)*))) @enum.method

; 枚举字段声明查询 - 使用量词操作符
(enum_declaration
  body: (enum_body
    (field_declaration
      declarator: (variable_declarator
        name: (identifier) @enum.field)
      type: (_) @field.type)*)) @enum.field

; 枚举构造函数查询
(enum_declaration
  body: (enum_body
    (constructor_declaration
      name: (identifier) @enum.constructor
      parameters: (formal_parameters
        (formal_parameter
          name: (identifier) @enum.constructor.param)*)*))) @enum.constructor

; 类型体查询
(enum_body) @body.enum

; 修饰符查询
(modifiers
  (modifier) @modifier.name) @definition.modifiers
`;