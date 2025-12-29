/*
Go Dependency Relationships-specific Tree-Sitter Query Patterns
用于识别和分析代码中的依赖关系
只包含基于抽象语法树的真实查询，不使用字符匹配
*/
export default `
; ===== 包导入依赖关系 =====

; 基本包导入依赖关系
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @dependency.import.path
    name: (_)? @dependency.import.name)) @dependency.relationship.import

; 包别名导入依赖关系
(import_declaration
  (import_spec
    name: (package_identifier) @dependency.alias.name
    path: (interpreted_string_literal) @dependency.alias.path)) @dependency.relationship.import.alias

; 点导入依赖关系
(import_declaration
  (import_spec
    name: (dot) @dependency.dot.import
    path: (interpreted_string_literal) @dependency.dot.path)) @dependency.relationship.dot.import

; 下划线导入依赖关系
(import_declaration
  (import_spec
    name: (blank_identifier) @dependency.blank.import
    path: (interpreted_string_literal) @dependency.blank.path)) @dependency.relationship.blank.import

; ===== 类型依赖关系 =====

; 类型引用依赖关系
(var_declaration
  (var_spec
    name: (identifier) @dependency.variable.name
    type: (type_identifier) @dependency.type.reference))) @dependency.relationship.type.reference

; 函数参数类型依赖关系
(parameter_declaration
  name: (identifier) @dependency.parameter.name
  type: (type_identifier) @dependency.parameter.type) @dependency.relationship.parameter.type

; 函数返回类型依赖关系
(function_declaration
  name: (identifier) @dependency.function.name
  result: (type_identifier) @dependency.return.type)) @dependency.relationship.return.type

; 方法接收者类型依赖关系
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @dependency.receiver.name
      type: (type_identifier) @dependency.receiver.type))
  name: (field_identifier) @dependency.method.name) @dependency.relationship.receiver.type

; 结构体字段类型依赖关系
(field_declaration
  name: (field_identifier) @dependency.field.name
  type: (type_identifier) @dependency.field.type) @dependency.relationship.field.type

; 接口方法参数类型依赖关系
(method_spec
  name: (field_identifier) @dependency.interface.method
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @dependency.param.name
      type: (type_identifier) @dependency.param.type)*)?) @dependency.relationship.interface.parameter

; 接口方法返回类型依赖关系
(method_spec
  name: (field_identifier) @dependency.interface.method
  result: (type_identifier) @dependency.interface.return.type)) @dependency.relationship.interface.return

; ===== 函数调用依赖关系 =====

; 函数调用依赖关系
(call_expression
  function: (identifier) @dependency.called.function
  arguments: (argument_list
    (identifier) @dependency.call.argument)*) @dependency.relationship.function.call

; 包函数调用依赖关系
(call_expression
  function: (selector_expression
    operand: (package_identifier) @dependency.called.package
    field: (identifier) @dependency.called.package.function)
  arguments: (argument_list
    (identifier) @dependency.call.argument)*) @dependency.relationship.package.function.call

; 方法调用依赖关系
(call_expression
  function: (selector_expression
    operand: (identifier) @dependency.called.object
    field: (field_identifier) @dependency.called.method)
  arguments: (argument_list
    (identifier) @dependency.call.argument)*) @dependency.relationship.method.call

; 链式方法调用依赖关系
(call_expression
  function: (selector_expression
    operand: (call_expression
      function: (selector_expression
        operand: (identifier) @dependency.chained.object
        field: (field_identifier) @dependency.chained.method))
    field: (field_identifier) @dependency.chained.next.method)
  arguments: (argument_list
    (identifier) @dependency.call.argument)*) @dependency.relationship.chained.call

; ===== 接口依赖关系 =====

; 接口实现依赖关系
(type_declaration
  (type_spec
    name: (type_identifier) @dependency.implementing.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @dependency.implemented.interface))))) @dependency.relationship.interface.implementation

; 接口组合依赖关系
(type_declaration
  (type_spec
    name: (type_identifier) @dependency.composite.interface
    type: (interface_type
      (method_spec_list
        (method_spec
          type: (type_identifier) @dependency.embedded.interface)*)))) @dependency.relationship.interface.composition

; ===== 结构体组合依赖关系 =====

; 结构体嵌入依赖关系
(type_declaration
  (type_spec
    name: (type_identifier) @dependency.embedding.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @dependency.embedded.type))))) @dependency.relationship.struct.embedding

; 多重嵌入依赖关系
(type_declaration
  (type_spec
    name: (type_identifier) @dependency.multiple.embedding.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @dependency.embedded.type.1)
        (field_declaration
          type: (type_identifier) @dependency.embedded.type.2)*)))) @dependency.relationship.multiple.embedding

; ===== 泛型依赖关系 =====

; 泛型类型参数依赖关系
(type_declaration
  (type_spec
    name: (type_identifier) @dependency.generic.type
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @dependency.type.parameter)))) @dependency.relationship.generic.type

; 泛型函数依赖关系
(function_declaration
  name: (identifier) @dependency.generic.function
  type_parameters: (type_parameter_list
    (type_parameter
      name: (identifier) @dependency.function.type.parameter))
  parameters: (parameter_list
    (parameter_declaration
      type: (type_identifier) @dependency.param.type)*)?) @dependency.relationship.generic.function

; ===== 数据结构依赖关系 =====

; 切片类型依赖关系
(var_declaration
  (var_spec
    name: (identifier) @dependency.slice.variable
    type: (slice_type
      element: (type_identifier) @dependency.slice.element))) @dependency.relationship.slice.type

; 数组类型依赖关系
(var_declaration
  (var_spec
    name: (identifier) @dependency.array.variable
    type: (array_type
      element: (type_identifier) @dependency.array.element))) @dependency.relationship.array.type

; 映射类型依赖关系
(var_declaration
  (var_spec
    name: (identifier) @dependency.map.variable
    type: (map_type
      key: (type_identifier) @dependency.map.key
      value: (type_identifier) @dependency.map.value))) @dependency.relationship.map.type

; 指针类型依赖关系
(var_declaration
  (var_spec
    name: (identifier) @dependency.pointer.variable
    type: (pointer_type
      element: (type_identifier) @dependency.pointer.element))) @dependency.relationship.pointer.type

; 通道类型依赖关系
(var_declaration
  (var_spec
    name: (identifier) @dependency.channel.variable
    type: (channel_type
      value: (type_identifier) @dependency.channel.element))) @dependency.relationship.channel.type

; 函数类型依赖关系
(var_declaration
  (var_spec
    name: (identifier) @dependency.function.variable
    type: (function_type
      parameters: (parameter_list
        (parameter_declaration
          type: (type_identifier) @dependency.param.type)*)?
      result: (type_identifier) @dependency.return.type))) @dependency.relationship.function.type

; ===== 并发依赖关系 =====

; 协程创建依赖关系
(go_statement
  (call_expression
    function: (identifier) @dependency.goroutine.function)) @dependency.relationship.goroutine.creation

; 通道操作依赖关系
(send_statement
  channel: (identifier) @dependency.channel.send
  value: (identifier) @dependency.channel.value) @dependency.relationship.channel.send

; 通道接收依赖关系
(unary_expression
  ["<-"] @dependency.receive.op
  operand: (identifier) @dependency.channel.receive) @dependency.relationship.channel.receive

; ===== 错误处理依赖关系 =====

; 错误返回依赖关系
(return_statement
  (expression_list
    (identifier) @dependency.error.return
    (identifier) @dependency.error.value)) @dependency.relationship.error.return

; 错误检查依赖关系
(if_statement
  condition: (binary_expression
    left: (identifier) @dependency.error.variable
    operator: "!="
    right: (identifier) @dependency.nil))
  consequence: (block) @dependency.error.handling) @dependency.relationship.error.checking
`;