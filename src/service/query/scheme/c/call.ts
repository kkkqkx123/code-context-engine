/*
C Call Relationships Tree-Sitter Query Patterns
用于识别和分析代码中的函数调用关系
基于文档：函数调用关系处理.md、函数调用分类与导入处理.md
*/
export default `
; ============================================
; 1. 基础函数调用关系
; ============================================

; 直接函数调用（同项目函数）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.function.name
      ) @call.expression
    )
  )
)

; 函数指针调用
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (pointer_expression
          argument: (identifier) @callee.pointer.name)
      ) @call.pointer
    )
  )
)

; 结构体方法调用
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (field_expression
          argument: (identifier) @object.name
        )
        field: (field_identifier) @method.name
      ) @call.method
    )
  )
)

; ============================================
; 2. 特殊调用类型
; ============================================

; 递归调用（函数调用自身）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.recursive.name
        (#eq? @callee.recursive.name @caller.function.name)
      ) @call.recursive
    )
  )
)

; 链式调用（如 obj.method1().method2()）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (field_expression
          argument: (call_expression
            function: (identifier) @chained.from
          )
          field: (field_identifier) @chained.to
        )
      ) @call.chained
    )
  )
)

; ============================================
; 3. 标准库函数调用
; ============================================

; 标准库函数调用（如 printf, scanf, malloc, free 等）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.stdlib.name
        (#match? @callee.stdlib.name "^(printf|scanf|malloc|free|calloc|realloc|memcpy|memset|strlen|strcpy|strcat|strcmp|fopen|fclose|fread|fwrite|exit|abort|assert|rand|srand|time|clock|system|getenv|atoi|atof|atol|strtod|strtol|strtoul)$")
      ) @call.stdlib
    )
  )
)

; 标准库字符串函数调用
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (field_expression
          argument: (identifier) @object.name
          (#match? @object.name "^(string|str|buf|buffer)$")
        )
        field: (field_identifier) @callee.string.method.name
      ) @call.string.method
    )
  )
)

; ============================================
; 4. 内置函数调用
; ============================================

; 内置函数调用（如 sizeof, typeof, alignof 等）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.builtin.name
        (#match? @callee.builtin.name "^(sizeof|typeof|alignof|__builtin_|__attribute__)$")
      ) @call.builtin
    )
  )
)

; ============================================
; 5. 构造函数/初始化函数调用
; ============================================

; 结构体初始化调用
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.constructor.name
        (#match? @callee.constructor.name "^(create|init|new|construct|build|make)$")
      ) @call.constructor
    )
  )
)

; ============================================
; 6. 回调函数调用
; ============================================

; 函数指针作为参数（回调函数）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.function.name
        arguments: (argument_list
          (argument
            (identifier) @callback.function.pointer
          )
        )
      ) @call.with.callback
    )
  )
)

; 函数指针类型作为参数
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.function.name
        arguments: (argument_list
          (argument
            (pointer_expression
              argument: (identifier) @callback.pointer.name
            )
          )
        )
      ) @call.with.pointer.callback
    )
  )
)

; ============================================
; 7. 宏函数调用
; ============================================

; 宏函数调用（如 MAX, MIN, ASSERT 等）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.macro.name
        (#match? @callee.macro.name "^[A-Z_]+$")
      ) @call.macro
    )
  )
)

; ============================================
; 8. 调用位置信息
; ============================================

; 捕获所有调用的行号（用于构建调用关系三元组）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression) @call.location
    )
  )
)

; ============================================
; 9. 复杂调用场景
; ============================================

; 嵌套调用（如 func1(func2(arg))）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.outer.name
        arguments: (argument_list
          (argument
            (call_expression
              function: (identifier) @callee.inner.name
            )
          )
        )
      ) @call.nested
    )
  )
)

; 条件表达式中的调用（如 condition ? func1() : func2()）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.function.name)
  body: (compound_statement
    (_
      (conditional_expression
        consequence: (call_expression
          function: (identifier) @callee.conditional.true
        )
        alternative: (call_expression
          function: (identifier) @callee.conditional.false
        )
      ) @call.conditional
    )
  )
)

; ============================================
; 10. 调用关系三元组构建
; ============================================

; 完整的调用关系信息（用于构建三元组）
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @caller.name)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @callee.name
      ) @call.full
    )
  )
)
`;