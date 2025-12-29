/*
C Lifecycle Relationships-specific Tree-Sitter Query Patterns
用于识别和分析代码中的生命周期管理模式
优先级3
*/
export default `
; 内存释放生命周期
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.lifecycle.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.lifecycle.free.to
        (#match? @relationship.lifecycle.free.to "^(free)$")
      ) @relationship.lifecycle.memory.deallocation
    )
  )
)

; 内存重新分配生命周期
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.lifecycle.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.lifecycle.realloc.to
        (#match? @relationship.lifecycle.realloc.to "^(realloc)$")
      ) @relationship.lifecycle.memory.reallocation
    )
  )
)

; 局部变量作用域开始
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.lifecycle.function.from)
  body: (compound_statement
    (_
      (declaration
        declarator: (init_declarator
          declarator: (identifier) @relationship.lifecycle.scope-begin.to)
      ) @relationship.lifecycle.scope.local.begin
    )
  )
)

; 局部变量作用域结束
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.lifecycle.function.from)
  body: (compound_statement
    (_
      (declaration
        declarator: (init_declarator
          declarator: (identifier) @relationship.lifecycle.scope-end.to)
      ) @relationship.lifecycle.scope.local.end
    )
  )
)

; 全局变量生命周期
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.lifecycle.function.from)
  body: (compound_statement
    (_
      (declaration
        declarator: (init_declarator
          declarator: (identifier) @relationship.lifecycle.global.to)
      ) @relationship.lifecycle.scope.global
    )
  )
)

; 静态变量生命周期
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.lifecycle.function.from)
  body: (compound_statement
    (_
      (declaration
        (storage_class_specifier) _
        declarator: (init_declarator
          declarator: (identifier) @relationship.lifecycle.static.to)
      ) @relationship.lifecycle.scope.static
    )
  )
)

; 函数参数生命周期
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.lifecycle.function.from)
  body: (compound_statement
    (_
      (parameter_declaration
        declarator: (identifier) @relationship.lifecycle.parameter.to
      ) @relationship.lifecycle.scope.parameter
    )
  )
)
`;