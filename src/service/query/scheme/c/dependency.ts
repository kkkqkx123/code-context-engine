/*
C Dependency Relationships Tree-Sitter Query Patterns
用于识别和分析代码中的依赖关系
基于文档：函数调用分类与导入处理.md
*/
export default `
; ============================================
; 1. 头文件包含依赖关系
; ============================================

; 用户头文件包含（同项目文件）
(preproc_include
  path: (string_literal) @include.user.path
) @include.user

; 系统头文件包含（标准库）
(preproc_include
  path: (system_lib_string) @include.system.path
) @include.system

; ============================================
; 2. 宏定义依赖关系
; ============================================

; 宏定义
(preproc_def
  name: (identifier) @macro.name
) @macro.definition

; 宏函数定义
(preproc_function_def
  name: (identifier) @macro.function.name
  parameters: (parameter_list) @macro.function.params
) @macro.function.definition

; 宏取消定义
(preproc_undef
  name: (identifier) @macro.undef.name
) @macro.undef

; ============================================
; 3. 条件编译依赖
; ============================================

; 1. 捕获 #if 或 #ifdef 块内部直接定义的宏
; 匹配结构: #if TEST -> #define TEST
(preproc_if
  condition: (identifier) @condition_name
  (preproc_def
    name: (identifier) @def_name
    value: (_)? @def_value) @def_node) @if_scope

(preproc_ifdef
  name: (identifier) @condition_name
  (preproc_def
    name: (identifier) @def_name
    value: (_)? @def_value) @def_node) @ifdef_scope

; 2. 捕获 #else 块内部定义的宏
; 匹配结构: #else -> #define DEV
; 注意：#else 的上下文（属于哪个 if）可以通过 @else_scope 的位置或父节点推断
(preproc_else
  (preproc_def
    name: (identifier) @def_name
    value: (_)? @def_value) @def_node) @else_scope

; 3. 捕获 #elif 块（如果代码中有，逻辑同 #if）
(preproc_elif
  condition: (identifier) @condition_name
  (preproc_def
    name: (identifier) @def_name
    value: (_)? @def_value) @def_node) @elif_scope

; ============================================
; 4. 类型引用依赖关系
; ============================================

; 类型标识符引用
(declaration
  type: (type_identifier) @type.reference.name
) @type.reference

; 结构体类型引用
(declaration
  type: (struct_specifier
    name: (type_identifier) @struct.reference.name
  )
) @struct.reference

; 联合体类型引用
(declaration
  type: (union_specifier
    name: (type_identifier) @union.reference.name
  )
) @union.reference

; 枚举类型引用
(declaration
  type: (enum_specifier
    name: (type_identifier) @enum.reference.name
  )
) @enum.reference

; ============================================
; 5. 函数声明依赖关系
; ============================================

; 函数声明（非定义）
(declaration
  declarator: (function_declarator
    declarator: (identifier) @function.declaration.name
  )
) @function.declaration

; 函数指针声明
(declaration
  declarator: (pointer_declarator
    declarator: (function_declarator
      declarator: (identifier) @function.pointer.declaration.name
    )
  )
) @function.pointer.declaration

; ============================================
; 6. 变量声明依赖关系
; ============================================

; 全局变量声明
(declaration
  declarator: (identifier) @global.variable.name
) @global.variable

; 外部变量声明
(declaration
  declarator: (identifier) @extern.variable.name
  storage_class: "extern"
) @extern.variable

; 静态变量声明
(declaration
  declarator: (identifier) @static.variable.name
  storage_class: "static"
) @static.variable

; ============================================
; 7. 类型定义依赖关系
; ============================================

; typedef 类型定义
(type_definition
  declarator: (type_identifier) @typedef.name
  type: (_) @typedef.type
) @typedef.definition

; typedef 结构体定义
(type_definition
  declarator: (type_identifier) @typedef.struct.name
  type: (struct_specifier
    body: (field_declaration_list) @typedef.struct.body
  )
) @typedef.struct.definition

; typedef 联合体定义
(type_definition
  declarator: (type_identifier) @typedef.union.name
  type: (union_specifier
    body: (field_declaration_list) @typedef.union.body
  )
) @typedef.union.definition

; typedef 函数指针定义
(type_definition
  declarator: (type_identifier) @typedef.function.pointer.name
  type: (pointer_declarator
    declarator: (function_declarator
      parameters: (parameter_list) @typedef.function.pointer.params
    )
  )
) @typedef.function.pointer.definition

; ============================================
; 8. 结构体/联合体/枚举定义依赖
; ============================================

; 结构体定义
(struct_specifier
  name: (type_identifier) @struct.definition.name
  body: (field_declaration_list) @struct.definition.body
) @struct.definition

; 联合体定义
(union_specifier
  name: (type_identifier) @union.definition.name
  body: (field_declaration_list) @union.definition.body
) @union.definition

; 枚举定义
(enum_specifier
  name: (type_identifier) @enum.definition.name
  body: (enumerator_list) @enum.definition.body
) @enum.definition

; 枚举值定义
(enumerator
  name: (identifier) @enum.value.name
) @enum.value

; ============================================
; 9. 函数定义依赖关系
; ============================================

; 函数定义
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @function.definition.name
  )
  body: (compound_statement) @function.definition.body
) @function.definition

; 静态函数定义
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @static.function.definition.name
  )
  storage_class: "static"
) @static.function.definition

; 内联函数定义
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @inline.function.definition.name
  )
  storage_class: "inline"
) @inline.function.definition

; ============================================
; 10. 外部库依赖识别
; ============================================

; 标准库头文件（用于识别外部库依赖）
(preproc_include
  path: (system_lib_string) @stdlib.include.path
  (#match? @stdlib.include.path "^(stdio|stdlib|string|math|time|ctype|assert|signal|setjmp|stdarg|stddef|stdbool|stdint|stddef|locale|errno|float|limits|wchar|wctype)$")
) @stdlib.include

; POSIX 标准库头文件
(preproc_include
  path: (system_lib_string) @posix.include.path
  (#match? @posix.include.path "^(unistd|fcntl|sys/types|sys/stat|sys/socket|netinet/in|arpa/inet|netdb|pthread|semaphore|mqueue|signal|time|dirent|pwd|grp|utime|fcntl|sys/wait|sys/ipc|sys/msg|sys/sem|sys/shm)$")
) @posix.include

; ============================================
; 11. 同项目文件依赖识别
; ============================================

; 相对路径头文件包含（同项目文件）
(preproc_include
  path: (string_literal) @project.include.path
  (#match? @project.include.path "^\\./")
) @project.include.relative

; 上级目录头文件包含（同项目文件）
(preproc_include
  path: (string_literal) @project.include.path
  (#match? @project.include.path "^\\.\\./")
) @project.include.parent

; 项目内头文件（非相对路径，但可能是项目文件）
(preproc_include
  path: (string_literal) @project.include.path
  (#not-match? @project.include.path "^\\./")
  (#not-match? @project.include.path "^\\.\\./")
) @project.include.absolute

; ============================================
; 12. 依赖关系三元组构建
; ============================================

; 文件到头文件的依赖关系
(translation_unit
  (preproc_include
    path: (string_literal) @dependency.target
  ) @dependency.edge
) @dependency.source

; 函数到类型的依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @dependency.source.function
  )
  body: (compound_statement
    (declaration
      type: (type_identifier) @dependency.target.type
    )
  )
) @dependency.function.to.type

; 函数到函数的依赖关系（通过函数声明）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @dependency.source.function
  )
  body: (compound_statement
    (declaration
      declarator: (function_declarator
        declarator: (identifier) @dependency.target.function
      )
    )
  )
) @dependency.function.to.function

; ============================================
; 13. 宏使用依赖关系
; ============================================

; 宏使用（在表达式中）
(call_expression
  function: (identifier) @macro.usage.name
  (#match? @macro.usage.name "^[A-Z_]+$")
) @macro.usage

; 宏条件使用
(preproc_if
  condition: (identifier) @macro.condition.name
) @macro.condition.usage

; ============================================
; 14. 类型别名依赖
; ============================================

; 类型别名使用
(declaration
  type: (type_identifier) @type.alias.name
) @type.alias.usage

; 结构体字段类型依赖
(field_declaration
  type: (type_identifier) @field.type.name
) @field.type.dependency

; ============================================
; 15. 完整依赖关系提取
; ============================================

; 提取函数的所有依赖（类型、函数、宏等）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @function.name
  )
  body: (compound_statement
    (_
      (declaration
        type: (type_identifier) @dependency.type
      )
    )
    (_
      (call_expression
        function: (identifier) @dependency.function
      )
    )
  )
) @function.dependencies
`;