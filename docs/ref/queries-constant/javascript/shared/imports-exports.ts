/*
JavaScript Import/Export Tree-Sitter Query Patterns
Shared queries for module imports and exports
*/
export default `
; Import statements - important for understanding dependencies
(import_statement) @name.definition.import

; Export statements - important for understanding public API
(export_statement) @name.definition.export

; Export statements with function declarations
(export_statement
  (function_declaration
    name: (identifier) @name.export)) @definition.export

; Export statements with class declarations
(export_statement
  (class_declaration
    name: (identifier) @name.export)) @definition.export

; Export statements with variable declarations
(export_statement
  (lexical_declaration
    (variable_declarator
      name: (identifier) @name.export))) @definition.export

; Export statements with variable declarations (var)
(export_statement
  (variable_declaration
    (variable_declarator
      name: (identifier) @name.export))) @definition.export

; Named exports with export_clause
(export_statement
  (export_clause
    (export_specifier
      (identifier) @name.export))) @definition.export

; Default exports
(export_statement
  "default"
  (identifier) @name.default) @definition.export

; Export default function
(export_statement
  "default"
  (function_declaration
    name: (identifier) @name.default)) @definition.export

; Export default class
(export_statement
  "default"
  (class_declaration
    name: (identifier) @name.default)) @definition.export

; Export from statements
(export_statement
  (export_clause) @export.clause
  "from"
  (string) @source.export) @definition.export
`;