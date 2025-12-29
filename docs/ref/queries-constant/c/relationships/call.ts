/*
C Call Relationships-specific Tree-Sitter Query Patterns
用于识别和分析代码中的函数调用关系
优先级4
*/
export default `
; 函数调用关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.call.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.call.function.to
      ) @relationship.call.function
    )
  )
)

; 函数指针调用关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.call.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (pointer_expression
          argument: (identifier) @relationship.call.pointer.to)
      ) @relationship.call.pointer
    )
  )
)

; 结构体方法调用关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.call.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (field_expression
          argument: (identifier) @relationship.call.object.to)
        field: (field_identifier) @relationship.call.method.to)
      ) @relationship.call.method
    )
  )
)

; 递归调用关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.call.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (identifier) @relationship.call.recursive.to
        (#eq? @relationship.call.recursive.to @relationship.call.function.from)
      ) @relationship.call.recursive
    )
  )
)

; 链式调用关系
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.call.function.from)
  body: (compound_statement
    (_
      (call_expression
        function: (field_expression
          argument: (call_expression
            function: (identifier) @relationship.call.chained.from)
          field: (field_identifier) @relationship.call.chained.to)
      ) @relationship.call.chained
    )
  )
)
`;