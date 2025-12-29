/*
C++ Control Flow and Expression-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
Optimized based on tree-sitter best practices
*/
export default `
; 异常处理查询 - 使用交替模式和字段名
[
  (try_statement
    body: (compound_statement) @try.body
    (catch_clause
      parameter: (parameter_declaration
        declarator: (identifier) @catch.param)
      body: (compound_statement) @catch.body)+) @definition.exception.handling
  (throw_statement
    (expression) @throw.expression) @definition.exception.throw
] @definition.exception

; 异常规范查询 - 使用交替模式
[
  (throw_specifier
    (type_identifier) @exception.type) @definition.exception.specification
  (noexcept_specifier
    (expression) @noexcept.expression) @definition.noexcept.specification
] @definition.exception.spec

; 范围for循环查询 - 使用字段名和锚点
(range_based_for_statement
  declarator: (identifier) @range.variable
  range: (_) @range.expression
  body: (compound_statement) @range.body) @definition.range.for

; 控制语句查询 - 使用交替模式和字段名
[
  (if_statement
    condition: (_) @if.condition
    consequence: (compound_statement) @if.body
    alternative: (_)? @if.else) @definition.control.if
  (for_statement
    initializer: (_)? @for.init
    condition: (_)? @for.condition
    update: (_)? @for.update
    body: (_) @for.body) @definition.control.for
  (while_statement
    condition: (_) @while.condition
    body: (_) @while.body) @definition.control.while
  (do_statement
    body: (_) @do.body
    condition: (_) @do.condition) @definition.control.do
  (switch_statement
    condition: (_) @switch.condition
    body: (compound_statement) @switch.body) @definition.control.switch
] @definition.control.statement

; 跳转语句查询 - 使用交替模式
[
  (break_statement) @definition.jump.break
  (continue_statement) @definition.jump.continue
  (return_statement
    (expression)? @return.expression) @definition.jump.return
  (goto_statement
    label: (statement_identifier) @goto.label) @definition.jump.goto
] @definition.jump.statement

; 标签语句查询 - 使用锚点确保精确匹配
(labeled_statement
  label: (statement_identifier) @label.name
  statement: (_) @label.statement) @definition.labeled.statement

; 表达式查询 - 使用交替模式合并相似表达式
[
  (binary_expression
    left: (_) @binary.left
    operator: _ @binary.operator
    right: (_) @binary.right) @definition.expression.binary
  (unary_expression
    operator: _ @unary.operator
    argument: (_) @unary.argument) @definition.expression.unary
  (update_expression
    argument: (_) @update.argument
    operator: _ @update.operator) @definition.expression.update
  (conditional_expression
    condition: (_) @conditional.condition
    consequence: (_) @conditional.consequence
    alternative: (_) @conditional.alternative) @definition.expression.conditional
] @definition.expression

; 类型转换和大小查询 - 使用交替模式
[
  (cast_expression
    type: (type_descriptor) @cast.type
    value: (_) @cast.value) @definition.expression.cast
  (sizeof_expression
    argument: (_) @sizeof.argument) @definition.expression.sizeof
  (typeid_expression
    argument: (_) @typeid.argument) @definition.expression.typeid
] @definition.expression.type

; 内存管理查询 - 使用交替模式
[
  (new_expression
    type: (_) @new.type
    arguments: (argument_list)? @new.arguments) @definition.expression.new
  (delete_expression
    argument: (_) @delete.argument) @definition.expression.delete
] @definition.expression.memory

; 复合表达式查询 - 简化模式
[
  (parenthesized_expression
    expression: (_) @parenthesized.expression) @definition.expression.parenthesized
] @definition.expression.compound

; 字面量查询 - 使用交替模式
[
  (number_literal) @definition.literal.number
  (string_literal) @definition.literal.string
  (char_literal) @definition.literal.char
  (true) @definition.literal.true
  (false) @definition.literal.false
  (null) @definition.literal.null
] @definition.literal
`;