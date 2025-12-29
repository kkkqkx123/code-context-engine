/*
Go Expression-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 类型操作查询 - 使用交替模式
[
  (type_assertion_expression) @expression.type_assertion
  (type_conversion_expression) @expression.type_conversion
] @definition.type.operation

; 选择器和调用表达式查询 - 使用交替模式
[
  (selector_expression
    operand: (identifier) @selector.object
    field: (field_identifier) @selector.field)
  (call_expression
    function: (identifier) @call.function
    arguments: (argument_list
      (identifier) @call.arg)*)
] @definition.invocation

; 复合字面量查询 - 使用参数化模式
(composite_literal
  type: (_) @literal.type
  body: (literal_value
    (literal_element
      (identifier) @element.value)*)) @definition.composite.literal

; 切片和索引表达式查询 - 使用交替模式
[
  (slice_expression
    operand: (identifier) @slice.array
    low: (_)? @slice.low
    high: (_)? @slice.high)
  (index_expression
    operand: (identifier) @index.array
    index: (identifier) @index.value)
] @definition.collection.access

; 二元和一元表达式查询 - 使用交替模式
[
  (binary_expression
    left: (_) @binary.left
    operator: (_) @binary.op
    right: (_) @binary.right)
  (unary_expression
    operator: (_) @unary.op
    operand: (_) @unary.operand)
] @definition.expression.operation

; 增减语句查询 - 使用交替模式
[
  (inc_statement
    operand: (identifier) @inc.var)
  (dec_statement
    operand: (identifier) @dec.var)
] @definition.update.operation

; 字面量查询 - 使用交替模式
[
  (interpreted_string_literal) @literal.string
  (raw_string_literal) @literal.raw_string
  (int_literal) @literal.int
  (float_literal) @literal.float
  (imaginary_literal) @literal.imaginary
  (rune_literal) @literal.rune
  (true) @literal.true
  (false) @literal.false
  (nil) @literal.nil
] @definition.literal

; 通道操作查询 - 使用交替模式
[
  (send_statement
    channel: (identifier) @channel.send
    value: (identifier) @channel.value)
  (unary_expression
    ["<-"] @receive.op
    operand: (identifier) @channel.receive)
] @definition.channel.operation

; 块和表达式语句查询 - 使用交替模式
[
  (block) @structure.block
  (expression_statement) @statement.expression
] @definition.structure

; 表达式查询 - 使用交替模式
[
  (identifier) @expression.identifier
  (selector_expression
    operand: (identifier) @selector.object
    field: (field_identifier) @selector.field)
  (index_expression
    operand: (identifier) @index.array
    index: (identifier) @index.value)
] @definition.expression

; 注释查询
(comment) @definition.comment

; 标签语句查询
(labeled_statement
  label: (label_name) @label.name
  statement: (_) @label.statement) @definition.labeled.statement
`;