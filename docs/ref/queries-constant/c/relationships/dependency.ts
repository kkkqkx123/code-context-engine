/*
C Dependency Relationships-specific Tree-Sitter Query Patterns
用于识别和分析代码中的依赖关系
*/
export default `
; 头文件包含依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.dependency.function.from)
  body: (compound_statement
    (_
      (preproc_include
        path: (string_literal) @relationship.dependency.include.to
      ) @relationship.dependency.include
    )
  )
)

; 系统头文件包含依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.dependency.function.from)
  body: (compound_statement
    (_
      (preproc_include
        path: (system_lib_string) @relationship.dependency.system.to
      ) @relationship.dependency.system
    )
  )
)

; 宏定义依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.dependency.function.from)
  body: (compound_statement
    (_
      (preproc_def
        name: (identifier) @relationship.dependency.macro.to
      ) @relationship.dependency.macro
    )
  )
)

; 宏函数定义依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.dependency.function.from)
  body: (compound_statement
    (_
      (preproc_function_def
        name: (identifier) @relationship.dependency.macro-function.to
      ) @relationship.dependency.macro.function
    )
  )
)

; 类型引用依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.dependency.function.from)
  body: (compound_statement
    (_
      (declaration
        type: (type_identifier) @relationship.dependency.type.to
      ) @relationship.dependency.type
    )
  )
)

; 结构体引用依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.dependency.function.from)
  body: (compound_statement
    (_
      (field_declaration
        type: (type_identifier) @relationship.dependency.struct.to
      ) @relationship.dependency.struct
    )
  )
)

; 函数声明依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.dependency.function.from)
  body: (compound_statement
    (_
      (declaration
        declarator: (function_declarator
          declarator: (identifier) @relationship.dependency.function.to
        )
      ) @relationship.dependency.function
    )
  )
)

; 枚举引用依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.dependency.function.from)
  body: (compound_statement
    (_
      (declaration
        type: (enum_specifier
          name: (type_identifier) @relationship.dependency.enum.to)
      ) @relationship.dependency.enum
    )
  )
)

; 联合体引用依赖关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.dependency.function.from)
  body: (compound_statement
    (_
      (declaration
        type: (union_specifier
          name: (type_identifier) @relationship.dependency.union.to)
      ) @relationship.dependency.union
    )
  )
)

`;