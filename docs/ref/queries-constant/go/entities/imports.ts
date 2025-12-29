/*
Go Import-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 导入声明查询 - 使用量词操作符
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @import.path
    name: (_)? @import.name)*) @definition.import

; 包声明查询 - 使用锚点确保精确匹配
(package_clause
  name: (package_identifier) @package.name) @definition.package
`;