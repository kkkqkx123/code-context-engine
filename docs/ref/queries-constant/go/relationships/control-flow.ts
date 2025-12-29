/*
Go Control Flow Relationships Tree-Sitter Query Patterns
用于追踪控制流关系和条件分支
*/
export default `
; 控制流语句查询 - 使用交替模式
[
  (if_statement) @control.if
  (for_statement) @control.for
  (switch_statement) @control.switch
  (select_statement) @control.select
] @definition.control.flow

; 分支语句查询 - 使用交替模式
[
  (expression_case) @branch.case
  (default_case) @branch.default
  (type_case) @branch.type.case
] @definition.control.branch

; 跳转语句查询 - 使用交替模式
[
  (return_statement) @jump.return
  (break_statement) @jump.break
  (continue_statement) @jump.continue
  (go_to_statement) @jump.goto
  (fallthrough_statement) @jump.fallthrough
] @definition.control.jump

; 延迟和协程语句查询 - 使用交替模式
[
  (defer_statement) @async.defer
  (go_statement) @async.go
] @definition.async.operation
`;