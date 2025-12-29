/*
C Control Flow-specific Tree-Sitter Query Patterns
用于识别和分析代码中的控制流模式
优先级4
*/
export default `
; if语句控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (if_statement
      condition: (identifier) @relationship.control_flow.if.to
    ) @relationship.control_flow.if
  )
)

; if-else语句控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (if_statement
      condition: (identifier) @relationship.control_flow.else.to
      alternative: (else_clause
        (statement) _)
    ) @relationship.control_flow.if.else
  )
)

; 嵌套if语句控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (if_statement
      condition: (identifier) @relationship.control_flow.nested.to
      consequence: (compound_statement
        (if_statement
          condition: (identifier) _))
    ) @relationship.control_flow.nested.if
  )
)

; else-if语句控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (if_statement
      condition: (identifier) @relationship.control_flow.elseif.to
      alternative: (else_clause
        (if_statement
          condition: (identifier) _))
    ) @relationship.control_flow.else.if
  )
)

; switch语句控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (switch_statement
      condition: (identifier) @relationship.control_flow.switch.to
    ) @relationship.control_flow.switch
  )
)

; case语句控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (case_statement
      value: (identifier) @relationship.control_flow.case.to
    ) @relationship.control_flow.switch.case
  )
)

; while循环控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (while_statement
      condition: (identifier) @relationship.control_flow.while.to
    ) @relationship.control_flow.while
  )
)

; do-while循环控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (do_statement
      condition: (identifier) @relationship.control_flow.dowhile.to
    ) @relationship.control_flow.do.while
  )
)

; for循环控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (for_statement
      condition: (identifier) @relationship.control_flow.for.to
    ) @relationship.control_flow.for
  )
)

; goto语句控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (goto_statement
      (statement_identifier) @relationship.control_flow.goto.to
    ) @relationship.control_flow.goto
  )
)

; return语句控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (return_statement
      (identifier) @relationship.control_flow.return.to
    ) @relationship.control_flow.return
  )
)

; 条件表达式控制流
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @relationship.control_flow.function.from)
  body: (compound_statement
    (conditional_expression
      condition: (identifier) @relationship.control_flow.conditional.to
    ) @relationship.control_flow.conditional.expression
  )
)
`;
