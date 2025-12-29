/*
Go Semantic Relationships-specific Tree-Sitter Query Patterns
用于识别方法重写、重载、委托、观察者模式等语义关系
只包含基于抽象语法树的真实查询，不使用字符匹配
*/
export default `
; ===== 接口实现语义关系 =====

; 接口实现语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.implementing.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @semantic.implemented.interface))))) @semantic.interface.implementation

; 结构体嵌入语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.embedding.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @semantic.embedded.type)+)))) @semantic.struct.embedding

; ===== 方法定义语义关系 =====

; 方法定义语义关系
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @semantic.receiver.name
      type: (type_identifier) @semantic.receiver.type))
  name: (field_identifier) @semantic.method.name)) @semantic.method.definition

; 函数定义语义关系
(function_declaration
  name: (identifier) @semantic.function.name)) @semantic.function.definition

; ===== 组合语义关系 =====

; 组合语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.composite.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @semantic.component.field
          type: (slice_type
            element: (type_identifier) @semantic.component.type)))))) @semantic.composition.relationship

; ===== 委托语义关系 =====

; 委托语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.delegator.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @semantic.delegate.field
          type: (type_identifier) @semantic.delegate.type))))) @semantic.delegation.relationship

; ===== 工厂方法语义关系 =====

; 工厂方法语义关系
(function_declaration
  name: (identifier) @semantic.factory.method
  parameters: (parameter_list
    (parameter_declaration
      type: (_) @semantic.param.type)*)
  return_type: (type_identifier) @semantic.product.type)) @semantic.factory.relationship

; ===== 单例语义关系 =====

; 单例语义关系
(var_declaration
  (var_spec
    name: (identifier) @semantic.singleton.instance
    type: (pointer_type
      element: (type_identifier) @semantic.singleton.type)
    value: (expression_list
      (call_expression
        function: (identifier) @semantic.constructor.call))))) @semantic.singleton.relationship

; ===== 观察者语义关系 =====

; 观察者方法语义关系
(function_declaration
  name: (identifier) @semantic.observer.method
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @semantic.event.param))
  return_type: (type_identifier) @semantic.observer.return.type)) @semantic.observer.relationship

; ===== 策略语义关系 =====

; 策略接口语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.strategy.interface
    type: (interface_type
      (method_spec_list
        (method_spec
          name: (field_identifier) @semantic.strategy.method)+)))) @semantic.strategy.relationship

; ===== 命令语义关系 =====

; 命令结构语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.command.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @semantic.receiver.field
          type: (type_identifier) @semantic.receiver.type))))) @semantic.command.relationship

; ===== 适配器语义关系 =====

; 适配器结构语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.adapter.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @semantic.adaptee.field
          type: (type_identifier) @semantic.adaptee.type))))) @semantic.adapter.relationship

; ===== 装饰器语义关系 =====

; 装饰器结构语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.decorator.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @semantic.component.field
          type: (type_identifier) @semantic.component.type))))) @semantic.decorator.relationship

; ===== 代理语义关系 =====

; 代理结构语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.proxy.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @semantic.real.subject.field
          type: (type_identifier) @semantic.real.subject.type))))) @semantic.proxy.relationship

; ===== 泛型语义关系 =====

; 泛型约束语义关系
(type_declaration
  (type_spec
    name: (type_identifier) @semantic.generic.type
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @semantic.type.parameter)
        type: (interface_type
          (method_spec_list
            (method_spec
              name: (field_identifier) @semantic.constraint.method)*))))) @semantic.generic.constraint

; ===== 接口方法语义关系 =====

; 接口方法语义关系
(method_spec
  name: (field_identifier) @semantic.interface.method
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @semantic.param.name
      type: (_) @semantic.param.type)*)?
  result: (_)? @semantic.method.return.type) @semantic.interface.method.definition

; ===== 结构体字段语义关系 =====

; 结构体字段语义关系
(field_declaration
  name: (field_identifier) @semantic.struct.field
  type: (_) @semantic.field.type) @semantic.struct.field.definition

; ===== 函数调用语义关系 =====

; 函数调用语义关系
(call_expression
  function: (identifier) @semantic.called.function
  arguments: (argument_list
    (identifier) @semantic.call.argument)*) @semantic.function.call.relationship

; 方法调用语义关系
(call_expression
  function: (selector_expression
    operand: (identifier) @semantic.called.object
    field: (field_identifier) @semantic.called.method)
  arguments: (argument_list
    (identifier) @semantic.call.argument)*) @semantic.method.call.relationship

; ===== 类型断言语义关系 =====

; 类型断言语义关系
(type_assertion_expression
  expression: (identifier) @semantic.asserted.value
  type: (type_identifier) @semantic.asserted.type)) @semantic.type.assertion.relationship

; ===== 错误处理语义关系 =====

; 错误返回语义关系
(return_statement
  (expression_list
    (identifier) @semantic.error.return
    (identifier) @semantic.error.value)) @semantic.error.return.relationship

; ===== 并发语义关系 =====

; 协程创建语义关系
(go_statement
  (call_expression
    function: (identifier) @semantic.goroutine.function)) @semantic.goroutine.creation.relationship

; 通道操作语义关系
(send_statement
  channel: (identifier) @semantic.channel.send
  value: (identifier) @semantic.channel.value) @semantic.channel.send.relationship

; ===== 资源管理语义关系 =====

; defer语义关系
(defer_statement
  (call_expression
    function: (selector_expression
      operand: (identifier) @semantic.resource.object
      field: (field_identifier) @semantic.resource.method))) @semantic.defer.relationship
`;