/*
Go Lifecycle Relationships-specific Tree-Sitter Query Patterns
用于识别对象实例化、初始化、销毁、生命周期管理等关系
只包含基于抽象语法树的真实查询，不使用字符匹配
*/
export default `
; ===== 对象实例化关系 =====

; 结构体字面量实例化关系
(composite_literal
  type: [
    (type_identifier) @instantiated.type
    (struct_type) @instantiated.struct
    (slice_type) @instantiated.slice
    (map_type) @instantiated.map
    (array_type) @instantiated.array
  ]
  body: (literal_value
    (literal_element
      (identifier) @constructor.param)*)) @lifecycle.instantiation

; 函数调用实例化关系
(call_expression
  function: (identifier) @constructor.function
  arguments: (argument_list
    (identifier) @constructor.param)*)) @lifecycle.function.instantiation

; ===== 初始化关系 =====

; 变量声明初始化关系
(var_declaration
  (var_spec
    name: (identifier) @initialized.var
    value: (identifier) @initial.value)) @lifecycle.variable.initialization

; 短变量声明初始化关系
(short_var_declaration
  left: (expression_list
    (identifier) @initialized.var)
  right: (expression_list
    (identifier) @initial.value))) @lifecycle.short.var.initialization

; 常量初始化关系
(const_declaration
  (const_spec
    name: (identifier) @initialized.const
    value: (identifier) @initial.value)) @lifecycle.constant.initialization

; ===== 资源获取关系 =====

; 文件打开关系
(call_expression
  function: (selector_expression
    operand: (identifier) @file.object
    field: (field_identifier) @file.method)
  arguments: (argument_list
    (identifier) @file.param)*)) @lifecycle.file.operation

; 网络连接关系
(call_expression
  function: (selector_expression
    operand: (identifier) @network.object
    field: (field_identifier) @network.method)
  arguments: (argument_list
    (identifier) @network.param)*)) @lifecycle.network.operation

; 数据库连接关系
(call_expression
  function: (selector_expression
    operand: (identifier) @database.object
    field: (field_identifier) @database.method)
  arguments: (argument_list
    (identifier) @database.param)*)) @lifecycle.database.operation

; ===== 资源释放关系 =====

; defer资源释放关系
(defer_statement
  (call_expression
    function: (selector_expression
      operand: (identifier) @resource.object
      field: (field_identifier) @resource.method))) @lifecycle.deferred.operation

; 函数调用资源释放关系
(call_expression
  function: (selector_expression
    operand: (identifier) @resource.object
    field: (field_identifier) @resource.method)
  arguments: (argument_list
    (identifier) @resource.param)*)) @lifecycle.resource.release

; ===== 通道生命周期 =====

; 通道创建关系
(call_expression
  function: (identifier) @channel.function
  arguments: (argument_list
    (channel_type) @channel.type))) @lifecycle.channel.creation

; 通道操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @channel.object
    field: (field_identifier) @channel.method)
  arguments: (argument_list
    (identifier) @channel.param)*)) @lifecycle.channel.operation

; ===== 锁生命周期 =====

; 锁创建关系
(call_expression
  function: (selector_expression
    operand: (identifier) @lock.factory
    field: (field_identifier) @lock.create.method)
  arguments: (argument_list
    (identifier) @lock.create.param)*)) @lifecycle.lock.creation

; 锁操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @lock.object
    field: (field_identifier) @lock.method)
  arguments: (argument_list
    (identifier) @lock.param)*)) @lifecycle.lock.operation

; ===== WaitGroup生命周期 =====

; WaitGroup创建关系
(call_expression
  function: (selector_expression
    operand: (identifier) @waitgroup.factory
    field: (field_identifier) @waitgroup.create.method)
  arguments: (argument_list
    (identifier) @waitgroup.create.param)*)) @lifecycle.waitgroup.creation

; WaitGroup操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @waitgroup.object
    field: (field_identifier) @waitgroup.method)
  arguments: (argument_list
    (identifier) @waitgroup.param)*)) @lifecycle.waitgroup.operation

; ===== Context生命周期 =====

; Context创建关系
(call_expression
  function: (selector_expression
    operand: (identifier) @context.factory
    field: (field_identifier) @context.create.method)
  arguments: (argument_list
    (identifier) @context.create.param)*)) @lifecycle.context.creation

; Context操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @context.object
    field: (field_identifier) @context.method)
  arguments: (argument_list
    (identifier) @context.param)*)) @lifecycle.context.operation

; ===== 协程生命周期 =====

; 协程创建关系
(go_statement
  (call_expression
    function: (identifier) @goroutine.function
    arguments: (argument_list
      (identifier) @goroutine.param)*)) @lifecycle.goroutine.creation

; ===== 定时器生命周期 =====

; 定时器创建关系
(call_expression
  function: (identifier) @timer.function
  arguments: (argument_list
    (identifier) @timer.param)*)) @lifecycle.timer.creation

; 定时器操作关系
(call_expression
  function: (selector_expression
    operand: (identifier) @timer.object
    field: (field_identifier) @timer.method)
  arguments: (argument_list
    (identifier) @timer.param)*)) @lifecycle.timer.operation

; ===== 函数生命周期 =====

; 函数声明关系
(function_declaration
  name: (identifier) @function.name
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @param.name
      type: (_) @param.type)*)?
  body: (block) @function.body) @lifecycle.function.declaration

; 方法声明关系
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @receiver.name
      type: (_) @receiver.type))
  name: (field_identifier) @method.name
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @param.name
      type: (_) @param.type)*)?
  body: (block) @method.body) @lifecycle.method.declaration

; 函数字面量关系
(func_literal
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @lambda.param
      type: (_) @lambda.param.type)*)?
  body: (block) @lambda.body) @lifecycle.function.literal

; ===== 类型生命周期 =====

; 类型声明关系
(type_declaration
  (type_spec
    name: (type_identifier) @type.name
    type: (_) @type.definition)) @lifecycle.type.declaration

; 类型别名关系
(type_alias
  name: (type_identifier) @alias.name
  type: (_) @alias.type) @lifecycle.type.alias

; ===== 包生命周期 =====

; 包声明关系
(package_clause
  name: (package_identifier) @package.name) @lifecycle.package.declaration

; ===== 导入生命周期 =====

; 导入声明关系
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @import.path
    name: (_)? @import.name)) @lifecycle.import.declaration
`;