/*
TypeScript Import/Export Tree-Sitter Query Patterns
Shared queries for module imports and exports
*/
export default `
; Import statements with source
(import_statement
  "from"
  (string) @source.import) @definition.import

; Named imports within import_clause
(import_statement
  (import_clause
    (named_imports
      (import_specifier
        (identifier) @name.import)))) @definition.import

; Namespace imports
(import_statement
  (import_clause
    (namespace_import
      "*"
      "as"
      (identifier) @name.namespace))) @definition.import

; Default imports
(import_statement
  (import_clause
    (identifier) @name.default)
  "from"
  (string) @source.import) @definition.import

; Mixed imports (default + named)
(import_statement
  (import_clause
    (identifier) @name.default
    ","
    (named_imports
      (import_specifier
        (identifier) @name.import)))
  "from"
  (string) @source.import) @definition.import

; Export statements with function declarations
(export_statement
  (function_declaration
    name: (identifier) @name.export)) @definition.export

; Export statements with class declarations
(export_statement
  (class_declaration
    name: (type_identifier) @name.export)) @definition.export

; Export statements with interface declarations
(export_statement
  (interface_declaration
    name: (type_identifier) @name.export)) @definition.export

; Export statements with type alias declarations
(export_statement
  (type_alias_declaration
    name: (type_identifier) @name.export)) @definition.export

; Named exports with export_clause
(export_statement
  (export_clause
    (export_specifier
      (identifier) @name.export))) @definition.export

; Default exports
(export_statement
  "default"
  (identifier) @name.default) @definition.export

; Export from statements
(export_statement
  (export_clause) @export.clause
  "from"
  (string) @source.export) @definition.export
`;