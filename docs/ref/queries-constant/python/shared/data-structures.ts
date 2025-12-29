/*
Python Data Structure and Pattern Matching-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Comprehensions with field names for better context
[
  (list_comprehension
    (identifier) @comprehension.variable)
  (dictionary_comprehension
    (identifier) @comprehension.variable)
  (set_comprehension
    (identifier) @comprehension.variable)
  (generator_expression
    (identifier) @comprehension.variable)
] @definition.comprehension

; Literal collections with anchor for precise matching
[
  (list
    (expression) @list.element)
  (tuple
    (expression) @tuple.element)
  (set
    (expression) @set.element)
  (dictionary
    (pair
      key: (expression) @dict.key
      value: (expression) @dict.value))
] @definition.collection

; Literal values with predicate filtering
[
  (string) @literal.string
  (integer) @literal.integer
  (float) @literal.float
  (true) @literal.boolean.true
 (false) @literal.boolean.false
 (none) @literal.none
  (ellipsis) @literal.ellipsis
] @definition.literal

; Pattern matching constructs (Python 3.10+) using alternation
[
  (class_pattern
    (identifier) @pattern.class)
  (tuple_pattern
    (identifier) @pattern.tuple.element)
  (list_pattern
    (identifier) @pattern.list.element)
 (dict_pattern
    (identifier) @pattern.dict.key)
] @definition.pattern.matching

; Expressions and special constructs
[
  (slice) @expression.slice
  (parenthesized_expression) @expression.parenthesized
  (expression_list) @expression.list
  (generator_expression) @expression.generator
] @definition.expression
`;