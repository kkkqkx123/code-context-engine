/*
Rust Control Flow Relationships Tree-Sitter Query Patterns
用于追踪控制流关系和条件分支
*/
export default `
; 控制流语句查询 - 使用交替模式
[
  (if_statement) @control.if
  (while_statement) @control.while
  (loop_statement) @control.loop
  (for_statement) @control.for
  (match_expression) @control.match
] @definition.control.flow

; 分支语句查询 - 使用交替模式
[
  (match_arm
    pattern: (match_pattern) @branch.pattern
    value: (_) @branch.value)
  (match_arm
    pattern: (wildcard_pattern) @branch.wildcard)
] @definition.control.branch

; 跳转语句查询 - 使用交替模式
[
  (break_expression
    (label)? @break.label
    (expression)? @break.value)
  (continue_expression
    (label)? @continue.label)
  (return_expression
    (expression)? @return.value)
] @definition.control.jump

; 异步控制流查询 - 使用交替模式
[
  (async_block
    body: (block) @async.body)
  (await_expression
    (expression) @await.expression)
] @definition.control.async
`;