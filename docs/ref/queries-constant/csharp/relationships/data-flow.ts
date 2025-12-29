/*
C# Data Flow-specific Tree-Sitter Query Patterns
用于追踪变量间的数据传递关系
Optimized based on tree-sitter best practices
*/
export default `
; 统一的赋值数据流查询 - 使用交替模式
(assignment_expression
  left: [
    (identifier) @target.variable
    (member_access_expression
      expression: (identifier) @target.object
      name: (identifier) @target.field)
    (element_access_expression
      expression: (identifier) @target.array
      subscript: (bracketed_argument_list
        (argument
          (identifier) @target.index)))
  ]
  right: [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (binary_expression
      left: (_) @binary.left
      right: (_) @binary.right)
    (conditional_expression
      condition: (_) @conditional.condition
      consequence: (_) @conditional.consequence
      alternative: (_) @conditional.alternative)
  ]) @data.flow.assignment

; 变量声明和初始化查询 - 使用交替模式
[
  (local_declaration_statement
    (variable_declaration
      type: (identifier) @variable.type
      declarators: (variable_declarator_list
        (variable_declarator
          name: (identifier) @target.variable
          value: (identifier) @source.variable)))) @data.flow.declaration.assignment
  (local_declaration_statement
    (variable_declaration
      declarators: (variable_declarator_list
        (variable_declarator
          name: (identifier) @target.variable
          value: (identifier) @source.variable)))) @data.flow.var.assignment
] @data.flow.variable.declaration

; 元组解构赋值查询 - 使用锚点和量词操作符
(assignment_expression
  left: (tuple_pattern
    (identifier) @target.variable1
    (identifier) @target.variable2)
  right: (tuple_expression
    (argument
      (identifier) @source.value1)
    (argument
      (identifier) @source.value2))) @data.flow.tuple.assignment

; 对象创建数据流 - 使用锚点确保精确匹配
(object_creation_expression
  type: (identifier) @target.class
  arguments: (argument_list
    (argument
      (identifier) @source.parameter)*)?) @data.flow.constructor.call

; 属性访问数据流 - 使用交替模式
[
  (assignment_expression
    left: (member_access_expression
      expression: (identifier) @target.object
      name: (identifier) @target.property)
    right: (identifier) @source.value) @data.flow.property.set
  (member_access_expression
    expression: (identifier) @source.object
    name: (identifier) @source.property) @data.flow.property.get
] @data.flow.property.access

; 索引器访问数据流 - 简化模式
(element_access_expression
  expression: (identifier) @source.collection
  subscript: (bracketed_argument_list
    (argument
      (identifier) @source.index))) @data.flow.indexer.access

; 事件处理数据流 - 使用交替模式
[
  (assignment_expression
    left: (member_access_expression
      expression: (identifier) @target.object
      name: (identifier) @target.event)
    right: (identifier) @source.handler) @data.flow.event.subscription
  (add_expression
    left: (member_access_expression
      expression: (identifier) @target.object
      name: (identifier) @target.event)
    right: (identifier) @source.handler) @data.flow.event.add
  (subtract_expression
    left: (member_access_expression
      expression: (identifier) @target.object
      name: (identifier) @target.event)
    right: (identifier) @source.handler) @data.flow.event.remove
] @data.flow.event.handling

; 委托数据流 - 使用交替模式
[
  (assignment_expression
    left: (identifier) @target.delegate
    right: (identifier) @source.function) @data.flow.delegate.assignment
  (assignment_expression
    left: (identifier) @target.delegate
    right: (lambda_expression) @source.lambda) @data.flow.delegate.lambda
] @data.flow.delegate

; 异步数据流 - 使用谓词过滤
[
  (await_expression
    expression: (identifier) @await.expression) @data.flow.async.await
  (assignment_expression
    left: (identifier) @target.variable
    right: (await_expression
      expression: (identifier) @source.await.expression))) @data.flow.async.assignment
] @data.flow.async

; LINQ数据流 - 使用锚点和字段名
(query_expression
  (from_clause
    identifier: (identifier) @linq.variable
    expression: (identifier) @linq.source)
  (query_body
    (select_clause
      expression: (identifier) @linq.result))) @data.flow.linq.query

; 类型转换数据流 - 使用交替模式
(cast_expression
  type: (identifier) @target.type
  value: (identifier) @source.variable) @data.flow.cast

; as表达式数据流
(as_expression
  type: (identifier) @target.type
  expression: (identifier) @source.expression) @data.flow.as.cast

; 空值处理数据流 - 使用交替模式
[
  (binary_expression
    left: (identifier) @left.operand
    operator: "??"
    right: (identifier) @right.operand) @data.flow.null.coalescing
  (conditional_access_expression
    expression: (identifier) @source.object
    name: (identifier) @target.property) @data.flow.null.conditional
] @data.flow.null.handling

; 返回值数据流 - 使用锚点操作符
(return_statement
  expression: [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (member_access_expression
      expression: (identifier) @source.object
      name: (identifier) @source.property)
  ]) @data.flow.return.value

; yield数据流 - 使用交替模式
[
   (yield_statement
     expression: (identifier) @yield.value) @data.flow.yield.return
   (yield_statement) @data.flow.yield.break
] @data.flow.yield
`;