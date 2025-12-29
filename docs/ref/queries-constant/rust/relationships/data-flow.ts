/*
Rust Data Flow-specific Tree-Sitter Query Patterns
用于追踪变量间的数据传递关系
*/
export default `
; 统一的赋值数据流查询 - 使用交替模式
(assignment_expression
  left: [
    (identifier) @target.variable
    (field_expression
      value: (identifier) @target.object
      field: (field_identifier) @target.field)
    (index_expression
      value: (identifier) @target.array
      index: (identifier) @target.index)
  ]
  right: [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (binary_expression
      left: (_) @binary.left
      right: (_) @binary.right)
    (closure_expression) @source.closure
  ]) @data.flow.assignment

; 函数调用参数传递数据流 - 使用参数化查询
(call_expression
  function: [
    (identifier) @target.function
    (field_expression
      value: (identifier) @target.object
      field: (field_identifier) @target.method)
  ]
  arguments: (arguments
    (identifier) @source.parameter)+) @data.flow.parameter.passing

; 返回值数据流 - 使用锚点操作符
(return_expression
  .
  [
    (identifier) @source.variable
    (field_expression
      value: (identifier) @source.object
      field: (field_identifier) @source.field)
    (call_expression
      function: (identifier) @source.function)
  ]) @data.flow.return.value

; 闭包赋值数据流 - 使用锚点确保精确匹配
(assignment_expression
  left: (identifier) @target.variable
  right: (closure_expression) @source.closure) @data.flow.closure.assignment

; 结构体实例化数据流 - 使用参数化查询
(struct_expression
  type: (type_identifier) @target.struct
  (field_initializer_list
    (field_initializer
      name: (field_identifier) @field.name
      value: (identifier) @field.value)+)) @data.flow.struct.initialization

; 元组解构赋值数据流 - 使用锚点确保精确匹配
(assignment_expression
  left: (tuple_pattern
    (identifier) @target.variable+)
  right: (identifier) @source.variable)) @data.flow.tuple.destructuring

; 模式匹配数据流 - 使用锚点确保精确匹配
(match_expression
  value: (identifier) @source.variable
  body: (match_block
    (match_arm
      pattern: (match_pattern
        (identifier) @target.variable)
      value: (identifier) @target.value)+)) @data.flow.match.pattern

; 引用和解引用数据流 - 使用交替模式
[
  (reference_expression
    (identifier) @source.variable) @data.flow.reference
  (dereference_expression
    (identifier) @source.pointer) @data.flow.dereference
] @data.flow.pointer.operation

; 类型转换数据流
(type_cast_expression
  value: (identifier) @source.variable
  type: (type_identifier) @target.type) @data.flow.type.conversion

; 宏调用参数数据流 - 使用参数化查询
(macro_invocation
  macro: (identifier) @target.macro
  arguments: (token_tree
    (identifier) @source.parameter)+) @data.flow.macro.parameter

; Let绑定数据流 - 使用锚点确保精确匹配
(let_statement
  pattern: (identifier) @target.variable
  value: (identifier) @source.value
  type: (type)? @let.type) @data.flow.let.binding

; 模式匹配中的数据流 - 使用量词操作符
(match_expression
  value: (identifier) @source.variable
  body: (match_block
    (match_arm
      pattern: (tuple_pattern
        (identifier) @target.variable1
        (identifier) @target.variable2)
      value: (identifier) @target.value)+)) @data.flow.match.destructuring

; 字段访问数据流 - 使用锚点确保精确匹配
(field_expression
  value: (identifier) @source.object
  field: (field_identifier) @source.field) @data.flow.field.access

; 数组/切片访问数据流 - 使用锚点确保精确匹配
(index_expression
  value: (identifier) @source.array
  index: (identifier) @source.index) @data.flow.array.access

; 方法调用数据流 - 使用锚点确保精确匹配
(call_expression
  function: (field_expression
    value: (identifier) @target.object
    field: (field_identifier) @target.method)
  arguments: (arguments
    (identifier) @source.parameter)+) @data.flow.method.call

; 泛型函数调用数据流 - 使用量词操作符
(call_expression
  function: (field_expression
    value: (identifier) @target.object
    field: (field_identifier) @target.method)
  type_arguments: (type_arguments
    (type_identifier) @type.arg)+
  arguments: (arguments
    (identifier) @source.parameter)+) @data.flow.generic.call

; 迭代器数据流 - 使用谓词过滤
(call_expression
  function: (field_expression
    value: (identifier) @target.iterable
    field: (field_identifier) @iterator.method)
  arguments: (arguments)*
  (#match? @iterator.method "^(iter|iter_mut|into_iter|into_iter_mut)$")) @data.flow.iterator.operation

; 集合操作数据流 - 使用谓词过滤
(call_expression
  function: (field_expression
    value: (identifier) @target.collection
    field: (field_identifier) @collection.method)
  arguments: (arguments
    (identifier) @source.parameter)*)
  (#match? @collection.method "^(push|pop|insert|remove|get|contains|len|is_empty)$")) @data.flow.collection.operation
`;