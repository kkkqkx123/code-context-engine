/*
C# Pattern Matching-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Pattern matching expressions - important for type-based matching
(is_pattern_expression) @definition.pattern_is

; Switch expressions - important for pattern-based switching
(switch_expression) @definition.switch_expression
(switch_expression_arm) @definition.switch_arm

; Pattern types - important for different pattern kinds
(constant_pattern) @definition.constant_pattern
(relational_pattern) @definition.relational_pattern
(var_pattern
  name: (identifier) @name.definition.var_pattern) @definition.var_pattern
(discard_pattern) @definition.discard_pattern
(binary_pattern) @definition.binary_pattern
(unary_pattern) @definition.unary_pattern
(parenthesized_pattern) @definition.parenthesized_pattern
(list_pattern) @definition.list_pattern
(slice_pattern) @definition.slice_pattern
(recursive_pattern) @definition.recursive_pattern
(positional_pattern) @definition.positional_pattern

; Property pattern clauses - important for property-based matching
(property_pattern_clause) @definition.property_pattern

; Subpatterns - important for nested patterns
(subpattern) @definition.subpattern

; Switch statements and expressions - important for control flow
(switch_statement) @definition.switch_statement
(switch_section) @definition.switch_section
(case_switch_label) @definition.case_label
(default_switch_label) @definition.default_label
(case_pattern_switch_label) @definition.pattern_label
`;