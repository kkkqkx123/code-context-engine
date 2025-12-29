/*
Python Import-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Import statements with field names for better context using alternation
[
  (import_statement
    (dotted_name) @import.module
    (aliased_import
      name: (dotted_name) @import.name
      alias: (identifier) @import.alias))
  (import_from_statement
    (dotted_name) @import.source
    (dotted_name) @import.name))
] @definition.import

; Special import patterns with predicate filtering
[
  (import_from_statement
    (wildcard_import)) @definition.wildcard_import
 (import_from_statement
    (relative_import)) @definition.relative_import
] @definition.special_import

; Scope-related statements with anchor for precise matching
[
  (global_statement
    (identifier) @global.variable)
  (nonlocal_statement
    (identifier) @nonlocal.variable)
] @definition.scope.statement
`;