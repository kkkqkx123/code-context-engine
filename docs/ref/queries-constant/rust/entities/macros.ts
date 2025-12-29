/*
Rust Macro-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 宏定义查询 - 使用参数化模式
(macro_definition
  name: (identifier) @macro.name
  (macro_rule
    (macro_matcher) @macro.matcher
    (transformation) @macro.transformation)*) @definition.macro

; 属性查询 - 使用量词操作符
(attribute_item
  (attribute
    name: (identifier) @attribute.name
    arguments: (token_tree
      (identifier) @attribute.arg)*))*) @definition.attribute

; 模式匹配查询 - 使用交替模式
[
  (match_pattern) @pattern.match
  (or_pattern) @pattern.or
  (tuple_pattern) @pattern.tuple
  (struct_pattern) @pattern.struct
  (tuple_struct_pattern) @pattern.tuple_struct
  (slice_pattern) @pattern.slice
  (captured_pattern) @pattern.captured
  (range_pattern) @pattern.range
  (reference_pattern) @pattern.reference
  (wildcard_pattern) @pattern.wildcard
] @definition.pattern

; 条件编译属性查询 - 使用谓词过滤
(attribute_item
  (attribute
    name: (identifier) @cfg.name
    arguments: (token_tree
      (identifier) @cfg.condition)*)
  (#match? @cfg.name "^cfg$")) @definition.conditional.compilation

; 循环标签查询
(loop_label
  (lifetime
    (identifier) @loop.label.name)) @definition.loop.label

; 内联汇编查询
(asm_expression
  template: (string_literal) @asm.template
  operands: (asm_operand_list
    (asm_operand) @asm.operand)*
  clobber_abis: (asm_clobber_abi_list
    (string_literal) @asm.clobber)*) @definition.asm.expression

; Const块查询
(const_block
  (statement) @const.statement*) @definition.const.block
`;