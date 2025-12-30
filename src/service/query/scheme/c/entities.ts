/*
C Entities Tree-Sitter Query Patterns
用于识别和分析代码中的实体定义
职责：只关注实体定义，不包含依赖关系和调用关系
基于文档：样例-函数转换.md、函数调用关系处理.md
*/
export default `
; ============================================
; 1. 预处理器实体
; ============================================

; 宏定义
(preproc_def
  name: (identifier) @entity.macro.name
) @entity.macro.definition

; 宏函数定义
(preproc_function_def
  name: (identifier) @entity.macro.function.name
  parameters: (preproc_params) @entity.macro.function.params
) @entity.macro.function.definition

; 宏取消定义
; 匹配 #undef 指令，并捕获被取消定义的宏名
(preproc_call
  directive: (preproc_directive) @directive
  argument: (preproc_arg) @entity.macro.name
  (#eq? @directive "#undef")
) @entity.macro.undef

; 条件编译指令

; 1. 捕获 #if 或 #ifdef 块内部直接定义的宏
; 匹配结构: #if TEST -> #define TEST
(preproc_if
  condition: (identifier) @condition_name
  (preproc_def
    name: (identifier) @entity.preproc_def_name
    value: (_)? @def_value) @def_node) @if_scope

(preproc_ifdef
  name: (identifier) @condition_name
  (preproc_def
    name: (identifier) @entity.preproc_def_name
    value: (_)? @def_value) @def_node) @ifdef_scope

; 2. 捕获 #else 块内部定义的宏
; 匹配结构: #else -> #define DEV
; 注意：#else 的上下文（属于哪个 if）可以通过 @else_scope 的位置或父节点推断
(preproc_else
  (preproc_def
    name: (identifier) @entity.preproc_def_name
    value: (_)? @def_value) @def_node) @entity.preproc_else_scope

; 3. 捕获 #elif 块（如果代码中有，逻辑同 #if）
(preproc_elif
  condition: (identifier) @condition_name
  (preproc_def
    name: (identifier) @entity.preproc_def_name
    value: (_)? @def_value) @def_node) @entity.preproc_elif_scope

; ============================================
; 2. 结构体、联合体、枚举定义
; ============================================

; 结构体定义
(struct_specifier
  name: (type_identifier) @entity.struct.name
  body: (field_declaration_list) @entity.struct.body
) @entity.struct

; 匿名结构体定义
(struct_specifier
  body: (field_declaration_list) @entity.struct_anon.body
) @entity.struct_anon

; 联合体定义
(union_specifier
  name: (type_identifier) @entity.union.name
  body: (field_declaration_list) @entity.union.body
) @entity.union

; 匿名联合体定义
(union_specifier
  body: (field_declaration_list) @entity.union_anon.body
) @entity.union_anon

; 枚举定义
(enum_specifier
  name: (type_identifier) @entity.enum.name
  body: (enumerator_list) @entity.enum.body
) @entity.enum

; 匿名枚举定义
(enum_specifier
  body: (enumerator_list) @entity.enum_anon.body
) @entity.enum_anon

; 枚举值
(enumerator
  name: (identifier) @entity.enum_value.name
  value: (_) @entity.enum_value.value?
) @entity.enum_value

; ============================================
; 3. 函数定义和声明
; ============================================

; 函数定义（完整）
(function_definition
  type: (_) @entity.function.return_type
  declarator: (function_declarator
    declarator: (identifier) @entity.function.name
    parameters: (parameter_list) @entity.function.params
  )
  body: (compound_statement) @entity.function.body
) @entity.function

; 函数定义（无返回类型）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @entity.function.name
    parameters: (parameter_list) @entity.function.params
  )
  body: (compound_statement) @entity.function.body
) @entity.function

; 函数声明（原型）
(declaration
  type: (_) @entity.function_prototype.return_type
  declarator: (function_declarator
    declarator: (identifier) @entity.function_prototype.name
    parameters: (parameter_list) @entity.function_prototype.params
  )
) @entity.function_prototype

; 静态函数定义
(function_definition
  type: (_) @entity.static_function.return_type
  declarator: (function_declarator
    declarator: (identifier) @entity.static_function.name
    parameters: (parameter_list) @entity.static_function.params
  )
  storage_class: "static"
  body: (compound_statement) @entity.static_function.body
) @entity.static_function

; 内联函数定义
(function_definition
  type: (_) @entity.inline_function.return_type
  declarator: (function_declarator
    declarator: (identifier) @entity.inline_function.name
    parameters: (parameter_list) @entity.inline_function.params
  )
  storage_class: "inline"
  body: (compound_statement) @entity.inline_function.body
) @entity.inline_function

; ============================================
; 4. 函数指针
; ============================================

; 函数指针声明
(declaration
  type: (_) @entity.function_pointer.return_type
  declarator: (pointer_declarator
    declarator: (function_declarator
      declarator: (identifier) @entity.function_pointer.name
      parameters: (parameter_list) @entity.function_pointer.params
    )
  )
) @entity.function_pointer

; 函数指针数组声明
(declaration
  type: (_) @entity.function_pointer_array.return_type
  declarator: (array_declarator
    declarator: (pointer_declarator
      declarator: (function_declarator
        declarator: (identifier) @entity.function_pointer_array.name
        parameters: (parameter_list) @entity.function_pointer_array.params
      )
    )
    size: (_) @entity.function_pointer_array.size
  )
) @entity.function_pointer_array

; ============================================
; 5. 类型定义（typedef）
; ============================================

; typedef 类型定义
(type_definition
  type: (_) @entity.typedef.original_type
  declarator: (type_identifier) @entity.typedef.alias
) @entity.typedef

; typedef 结构体定义
(type_definition
  type: (struct_specifier
    name: (type_identifier)? @entity.typedef_struct.original_name
    body: (field_declaration_list) @entity.typedef_struct.body
  )
  declarator: (type_identifier) @entity.typedef_struct.alias
) @entity.typedef_struct

; typedef 联合体定义
(type_definition
  type: (union_specifier
    name: (type_identifier)? @entity.typedef_union.original_name
    body: (field_declaration_list) @entity.typedef_union.body
  )
  declarator: (type_identifier) @entity.typedef_union.alias
) @entity.typedef_union

; typedef 枚举定义
(type_definition
  type: (enum_specifier
    name: (type_identifier)? @entity.typedef_enum.original_name
    body: (enumerator_list) @entity.typedef_enum.body
  )
  declarator: (type_identifier) @entity.typedef_enum.alias
) @entity.typedef_enum

; typedef 函数指针定义
(type_definition
  type: (pointer_declarator
    declarator: (function_declarator
      parameters: (parameter_list) @entity.typedef_function_pointer.params
    )
  )
  declarator: (type_identifier) @entity.typedef_function_pointer.alias
) @entity.typedef_function_pointer

; ============================================
; 6. 变量声明
; ============================================

; 全局变量声明
(declaration
  type: (_) @entity.global_variable.type
  declarator: (identifier) @entity.global_variable.name
) @entity.global_variable

; 全局变量声明（带初始化）
(declaration
  type: (_) @entity.global_variable.type
  declarator: (init_declarator
    declarator: (identifier) @entity.global_variable.name
    value: (_) @entity.global_variable.value
  )
) @entity.global_variable

; 静态变量声明
(declaration
  type: (_) @entity.static_variable.type
  declarator: (identifier) @entity.static_variable.name
  storage_class: "static"
) @entity.static_variable

; 外部变量声明
(declaration
  type: (_) @entity.extern_variable.type
  declarator: (identifier) @entity.extern_variable.name
  storage_class: "extern"
) @entity.extern_variable

; 常量变量声明
(declaration
  type: (_) @entity.const_variable.type
  declarator: (identifier) @entity.const_variable.name
  type_qualifier: "const"
) @entity.const_variable

; ============================================
; 7. 数组声明
; ============================================

; 数组声明（单维）
(declaration
  type: (_) @entity.array.type
  declarator: (array_declarator
    declarator: (identifier) @entity.array.name
    size: (_) @entity.array.size
  )
) @entity.array

; 数组声明（多维）
(declaration
  type: (_) @entity.array.type
  declarator: (array_declarator
    declarator: (array_declarator
      declarator: (identifier) @entity.array.name
      size: (_) @entity.array.size_outer
    )
    size: (_) @entity.array.size_inner
  )
) @entity.array

; 数组声明（带初始化）
(declaration
  type: (_) @entity.array.type
  declarator: (init_declarator
    declarator: (array_declarator
      declarator: (identifier) @entity.array.name
      size: (_) @entity.array.size
    )
    value: (_) @entity.array.value
  )
) @entity.array

; ============================================
; 8. 指针声明
; ============================================

; 指针声明
(declaration
  type: (_) @entity.pointer.type
  declarator: (pointer_declarator
    declarator: (identifier) @entity.pointer.name
  )
) @entity.pointer

; 指针声明（带初始化）
(declaration
  type: (_) @entity.pointer.type
  declarator: (init_declarator
    declarator: (pointer_declarator
      declarator: (identifier) @entity.pointer.name
    )
    value: (_) @entity.pointer.value
  )
) @entity.pointer

; 多级指针声明
(declaration
  type: (_) @entity.pointer.type
  declarator: (pointer_declarator
    declarator: (pointer_declarator
      declarator: (identifier) @entity.pointer.name
    )
  )
) @entity.pointer

; ============================================
; 9. 结构体字段
; ============================================

; 结构体字段声明
(field_declaration
  type: (_) @entity.field.type
  declarator: (field_identifier) @entity.field.name
) @entity.field

; 结构体位字段
(field_declaration
  type: (_) @entity.bitfield.type
  declarator: (field_identifier) @entity.bitfield.name
  size: (_) @entity.bitfield.size
) @entity.bitfield

; 结构体数组字段
(field_declaration
  type: (_) @entity.field_array.type
  declarator: (array_declarator
    declarator: (field_identifier) @entity.field_array.name
    size: (_) @entity.field_array.size
  )
) @entity.field_array

; 结构体指针字段
(field_declaration
  type: (_) @entity.field_pointer.type
  declarator: (pointer_declarator
    declarator: (field_identifier) @entity.field_pointer.name
  )
) @entity.field_pointer

; ============================================
; 10. 函数参数
; ============================================

; 函数参数声明
(parameter_declaration
  type: (_) @entity.parameter.type
  declarator: (identifier) @entity.parameter.name
) @entity.parameter

; 函数参数声明（带默认值）
(parameter_declaration
  type: (_) @entity.parameter.type
  declarator: (identifier) @entity.parameter.name
  default_value: (_) @entity.parameter.default
) @entity.parameter

; 函数参数声明（数组）
(parameter_declaration
  type: (_) @entity.parameter_array.type
  declarator: (array_declarator
    declarator: (identifier) @entity.parameter_array.name
  )
) @entity.parameter_array

; 函数参数声明（指针）
(parameter_declaration
  type: (_) @entity.parameter_pointer.type
  declarator: (pointer_declarator
    declarator: (identifier) @entity.parameter_pointer.name
  )
) @entity.parameter_pointer

; 函数参数声明（函数指针）
(parameter_declaration
  type: (_) @entity.parameter_function_pointer.return_type
  declarator: (pointer_declarator
    declarator: (function_declarator
      declarator: (identifier) @entity.parameter_function_pointer.name
      parameters: (parameter_list) @entity.parameter_function_pointer.params
    )
  )
) @entity.parameter_function_pointer

; ============================================
; 11. 注释和文档
; ============================================

; 单行注释
(comment) @entity.comment.line

; 块注释
(comment) @entity.comment.block

; 文档注释（Doxygen风格）
(comment) @entity.comment.documentation

; ============================================
; 12. 属性和注解（C11）
; ============================================

; 属性声明
(attribute_declaration
  (attribute
    name: (identifier) @entity.attribute.name
    arguments: (argument_list
      (_) @entity.attribute.argument*
    )
  )
) @entity.attribute

; 类型属性
(type_definition
  (attribute
    name: (identifier) @entity.type_attribute.name
    arguments: (argument_list
      (_) @entity.type_attribute.argument*
    )
  )
) @entity.type_attribute

; 变量属性
(declaration
  (attribute
    name: (identifier) @entity.variable_attribute.name
    arguments: (argument_list
      (_) @entity.variable_attribute.argument*
    )
  )
) @entity.variable_attribute

; 函数属性
(function_definition
  (attribute
    name: (identifier) @entity.function_attribute.name
    arguments: (argument_list
      (_) @entity.function_attribute.argument*
    )
  )
) @entity.function_attribute

; 结构体字段属性
(field_declaration
  (attribute
    name: (identifier) @entity.field_attribute.name
    arguments: (argument_list
      (_) @entity.field_attribute.argument*
    )
  )
) @entity.field_attribute

; ============================================
; 13. 标签和跳转
; ============================================

; 标签声明
(statement_label
  name: (identifier) @entity.label.name
) @entity.label

; goto 语句
(goto_statement
  label: (identifier) @entity.goto.label
) @entity.goto

; ============================================
; 14. 复合实体提取
; ============================================

; 完整的函数实体（包含所有信息）
(function_definition
  type: (_) @entity.full_function.return_type
  declarator: (function_declarator
    declarator: (identifier) @entity.full_function.name
    parameters: (parameter_list) @entity.full_function.params
  )
  body: (compound_statement
    (comment)* @entity.full_function.documentation
  )
) @entity.full_function

; 完整的结构体实体（包含所有信息）
(struct_specifier
  name: (type_identifier) @entity.full_struct.name
  body: (field_declaration_list
    (comment)* @entity.full_struct.documentation
    (field_declaration)* @entity.full_struct.fields
  )
) @entity.full_struct

; 完整的枚举实体（包含所有信息）
(enum_specifier
  name: (type_identifier) @entity.full_enum.name
  body: (enumerator_list
    (comment)* @entity.full_enum.documentation
    (enumerator)* @entity.full_enum.values
  )
) @entity.full_enum
`;