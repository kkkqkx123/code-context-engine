/*
C Semantic Relationships-specific Tree-Sitter Query Patterns
用于识别和分析代码中的高级语义关系
从 ref/semantic-relationships.ts 迁移而来，排除已在其他文件中的功能
*/
export default `

; 错误处理返回模式
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.semantic.function.from)
  body: (compound_statement
    (_
      (return_statement
        (identifier) @relationship.semantic.error-return.to
        (#match? @relationship.semantic.error-return.to "^(ERROR|FAIL|INVALID|NULL)$")
      ) @relationship.semantic.error.return
    )
  )
)

; 错误检查模式
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.semantic.function.from)
  body: (compound_statement
    (_
      (if_statement
        condition: (binary_expression
          left: (identifier) _
          operator: ["==" "!="]
          right: (identifier) @relationship.semantic.error-check.to)
      ) @relationship.semantic.error.checking
    )
  )
)

; 资源管理模式
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.semantic.function.from)
  body: (compound_statement
    (_
      (declaration
        declarator: (init_declarator
          declarator: (identifier) @relationship.semantic.resource.to
          value: (call_expression
            function: (identifier) _))
      ) @relationship.semantic.resource.initialization
    )
  )
)

; 回调函数赋值模式
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.semantic.function.from)
  body: (compound_statement
    (_
      (declaration
        declarator: (init_declarator
          declarator: (identifier) _
          value: (identifier) @relationship.semantic.callback.to)
      ) @relationship.semantic.callback.assignment
    )
  )
)

; 回调函数初始化模式
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.semantic.function.from)
  body: (compound_statement
    (_
      (init_declarator
        declarator: (identifier) _
        value: (identifier) @relationship.semantic.callback-init.to
      ) @relationship.semantic.callback.assignment
    )
  )
)

; 回调函数类型定义模式
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.semantic.function.from)
  body: (compound_statement
    (_
      (type_definition
        (function_declarator
          (parenthesized_declarator
            (pointer_declarator
              (type_identifier) @relationship.semantic.callback-type.to)))
      ) @relationship.semantic.callback.type
    )
  )
)
`;