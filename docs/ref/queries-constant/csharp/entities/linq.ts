/*
C# LINQ and Query-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; LINQ expressions - important for data querying
(query_expression) @definition.linq_expression

; LINQ from clause - important for data source
(from_clause
  name: (identifier) @name.definition.linq_from) @definition.linq_from

; LINQ where clause - important for filtering
(where_clause) @definition.linq_where

; LINQ select clause - important for projection
(select_clause
  (identifier) @name.definition.linq_select) @definition.linq_select

; LINQ group clause - important for grouping
(group_clause
  (identifier) @name.definition.linq_group) @definition.linq_group

; LINQ order by clause - important for sorting
(order_by_clause) @definition.linq_order

; LINQ join clause - important for joining data
(join_clause
  left: (identifier) @name.definition.linq_join_left
  right: (identifier) @name.definition.linq_join_right) @definition.linq_join

; LINQ let clause - important for intermediate variables
(let_clause
  name: (identifier) @name.definition.linq_let) @definition.linq_let
`;