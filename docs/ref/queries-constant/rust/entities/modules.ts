/*
Rust Module-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 模块声明查询
(mod_item
  name: (identifier) @module.name) @definition.module

; 外部函数接口查询 - 使用交替模式
[
  (foreign_item_fn
    name: (identifier) @foreign.function.name
    parameters: (parameters
      (parameter
        name: (identifier) @foreign.param
        type: (_) @foreign.type))*
    return_type: (_) @foreign.return?)
  (foreign_static_item
    name: (identifier) @foreign.static.name
    type: (_) @foreign.static.type
    mutability: (mut)? @foreign.mutability)
] @definition.foreign.item
`;