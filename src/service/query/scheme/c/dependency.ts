/*
C Dependency Relationships Tree-Sitter Query Patterns
用于识别和分析代码中的依赖关系
职责：只关注依赖关系（使用、引用、包含），不包含定义模式
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
; 2. 宏使用依赖关系
; ============================================

; 注意：在C语言中，宏使用和函数调用在语法上是一样的，Tree-sitter无法区分。
; 宏使用需要在预处理阶段才能准确识别，这里只能捕获可能的宏使用（基于命名约定）。
; 建议结合预处理器的宏定义列表来准确识别宏使用。

; 可能的宏使用（基于命名约定：全大写字母）
(call_expression
  function: (identifier) @macro.usage.name
  (#match? @macro.usage.name "^[A-Z][A-Z0-9_]*$")
) @macro.usage

; 宏条件使用（在预处理指令中）
(preproc_if
  condition: (identifier) @macro.condition.name
) @macro.condition.usage

; 包括ifndef
(preproc_ifdef
  name: (identifier) @macro.condition.name
) @macro.condition.usage


; ============================================
; 3. 类型引用依赖关系
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
; 4. 外部变量声明依赖关系
; ============================================

; 外部变量声明（extern，表示依赖）
(declaration
  declarator: (identifier) @extern.variable.name
  storage_class: "extern"
) @extern.variable

; ============================================
; 5. 类型别名使用依赖关系
; ============================================

; 类型别名使用（typedef定义的类型的引用）
(declaration
  type: (type_identifier) @type.alias.name
) @type.alias.usage

; 结构体字段类型依赖
(field_declaration
  type: (type_identifier) @field.type.name
) @field.type.dependency

`;