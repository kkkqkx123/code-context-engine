export default `
; 1. 预处理器 -> entity.preprocessor 5
[
  [
  (preproc_def
    name: (identifier) @name.entity.macro)
  (preproc_function_def
    name: (identifier) @name.entity.macro)
  (preproc_ifdef
    name: (identifier) @name.entity.preproc_ifdef) @entity.preproc_ifdef
  ]

  [
    (preproc_if
      condition: (_) @name.entity.preproc_condition) @entity.preproc_condition
    (preproc_elif
      condition: (_) @name.entity.preproc_condition) @entity.preproc_condition
    (preproc_else) @entity.preproc_condition
  ]
  
  [
  (preproc_include
    path: (system_lib_string) @name.entity.include_lib)
  (preproc_include
    path: (string_literal)@name.included)
  ]
] @entity.preprocessor

; 2. 结构体、联合体、枚举定义 -> entity.type 4
[
    (class_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list) @type.body) @entity.class
  (struct_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list) @type.body) @entity.struct
  (union_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list) @type.body) @entity.union
]

; 3. 函数声明 -> entity.function 3
[
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @function.name)
    body: (compound_statement) @function.body) @entity.function

  (function_definition
    type: (primitive_type)
    declarator: (function_declarator
      declarator: (identifier) @function.name
      parameters: (parameter_list) @function.parameters)) @entity.function.prototype

  ; 函数指针查询 - 修复：函数指针声明的正确结构
  (declaration
    type: (primitive_type) @return.type
    declarator: (function_declarator
      declarator: (parenthesized_declarator
        (pointer_declarator
          declarator: (identifier) @function.pointer.name))
      parameters: (parameter_list))) @entity.function.pointer
] @entity.function

; 4. 数组和指针声明 -> entity.variable 2
[
; 数组查询只能捕获单维数组，但对于多维数组能够捕获最外层结构
  (declaration
    type: (_)
    declarator: (array_declarator
      declarator: (identifier) @array.name
      size: (_)? @array.size)) @entity.array
  (declaration
    type: (_)
    declarator: (pointer_declarator
      declarator: (identifier) @pointer.name)) @entity.compound_variable
] @entity.variable

; 5. 变量声明查询 - 使用交替模式合并 1
[
  (declaration
    type: (_)
    declarator: (identifier) @entity.name.variable)
  (declaration
    type: (_)
    declarator: (init_declarator
      declarator: (identifier) @entity.name.variable))
] @entity.variable

; 6. 注释与注解 - 优先级0
(comment) @comment.entity

; C11属性说明符
(attribute_declaration
  (attribute
    name: (identifier) @annotation.name
    arguments: (argument_list
      (_) @annotation.argument)*)) @annotation.attribute

; 类型注解
(type_definition
  (attribute
    name: (identifier) @annotation.name
    arguments: (argument_list
      (_) @annotation.argument)*)?) @annotation.type

; 变量注解
(declaration
  (attribute
    name: (identifier) @annotation.name
    arguments: (argument_list
      (_) @annotation.argument)*)?) @annotation.variable

; 结构体字段注解
(field_declaration
  (attribute
    name: (identifier) @annotation.name
    arguments: (argument_list
      (_) @annotation.argument)*)?) @annotation.field
`;