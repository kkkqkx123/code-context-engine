/*
TypeScript Data Flow-specific Tree-Sitter Query Patterns
用于追踪变量间的数据传递关系
基于JavaScript数据流查询，添加TypeScript特有功能
*/
export default `
; 统一的赋值数据流查询 - 使用交替模式
(assignment_expression
  left: [
    (identifier) @target.variable
    (member_expression
      object: (identifier) @target.object
      property: (property_identifier) @target.property)
    (subscript_expression
      object: (identifier) @target.array
      index: (identifier) @target.index)
  ]
  right: [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (binary_expression
      left: (_) @binary.left
      right: (_) @binary.right)
  ]) @data.flow.assignment

; 解构赋值数据流 - 使用交替模式
(assignment_expression
  left: [
    (object_pattern
      (pair_pattern
        key: (property_identifier) @source.property
        value: (identifier) @target.variable)*)
    (array_pattern
      (identifier) @target.variable*)
  ]
  right: (identifier) @source.object) @data.flow.destructuring

; 函数调用数据流 - 参数化查询
(call_expression
  function: [
    (identifier) @target.function
    (member_expression
      object: (identifier) @target.object
      property: (property_identifier) @target.method)
  ]
  arguments: (arguments
    (identifier) @source.parameter)*) @data.flow.parameter.passing

; 返回值数据流 - 使用锚点操作符
(return_statement
  .
  [
    (identifier) @source.variable
    (call_expression
      function: (identifier) @source.function)
    (member_expression
      object: (identifier) @source.object
      property: (property_identifier) @source.property)
  ]) @data.flow.return.value

; TypeScript特有的数据流模式

; 类型注解变量赋值数据流
(lexical_declaration
  (variable_declarator
    name: (identifier) @target.variable
    type: (type_annotation
      type: (type_identifier) @variable.type)
    value: (identifier) @source.variable))) @data.flow.typed.assignment

; 泛型函数调用参数传递数据流
(call_expression
  function: (generic_type
    name: (identifier) @target.generic.function
    type_arguments: (type_arguments
      (type_identifier) @type.argument)*)
  arguments: (arguments
    (identifier) @source.parameter)*) @data.flow.generic.parameter

; 类型断言数据流
(as_expression
  value: (identifier) @source.variable
  type: (type_identifier) @target.type) @data.flow.type.assertion

; 非空断言数据流
(non_null_expression
  value: (identifier) @source.variable) @data.flow.non.null.assertion

; 可选链数据流
(member_expression
  object: (identifier) @source.object
  property: (property_identifier) @target.property
  ?) @data.flow.optional.chain

; 空值合并数据流
(binary_expression
  left: (identifier) @source.variable
  right: (identifier) @target.variable
  operator: "??") @data.flow.null.coalescing

; Promise异步数据流
(assignment_expression
  left: (identifier) @target.variable
  right: (await_expression
    value: (identifier) @source.promise)) @data.flow.async.await

; 枚举成员数据流
(member_expression
  object: (identifier) @source.enum
  property: (property_identifier) @source.member)) @data.flow.enum.member

; 类构造函数参数数据流
(class_declaration
  name: (identifier) @target.class
  body: (class_body
    (constructor_definition
      parameters: (formal_parameters
        (required_parameter
          name: (identifier) @source.parameter
          type: (type_annotation)? @parameter.type)*)))) @data.flow.constructor.parameter

; 类属性初始化数据流
(class_declaration
  name: (identifier) @target.class
  body: (class_body
    (property_definition
      name: (property_identifier) @target.property
      type: (type_annotation)? @property.type
      value: (identifier) @source.value)))) @data.flow.property.initialization

; 方法重写数据流
(class_declaration
  name: (identifier) @child.class
  heritage: (heritage_clause
    (extends_clause
      (type_identifier) @parent.class))
  body: (class_body
    (method_definition
      name: (property_identifier) @overridden.method)))) @data.flow.method.override

; 泛型约束数据流
(type_parameters
  (type_parameter
    name: (type_identifier) @constrained.type
    constraint: (type_identifier) @constraint.type)) @data.flow.generic.constraint
`;