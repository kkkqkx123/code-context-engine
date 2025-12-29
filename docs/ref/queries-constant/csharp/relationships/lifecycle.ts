/*
C# Lifecycle Relationships-specific Tree-Sitter Query Patterns
用于识别对象实例化、初始化、销毁、生命周期管理等关系
*/
export default `
; 构造函数定义生命周期
(constructor_declaration
  name: (identifier) @constructor.name
  parameters: (parameter_list)) @lifecycle.relationship.constructor.definition

; 构造函数调用生命周期
(object_creation_expression
  type: (identifier) @constructed.class
  arguments: (argument_list
    (argument
      (identifier) @constructor.argument))) @lifecycle.relationship.constructor.call

; 静态构造函数生命周期
(static_constructor_declaration
  name: (identifier) @static.constructor.name
  body: (block) @static.constructor.body) @lifecycle.relationship.static.constructor

; 析构函数生命周期
(destructor_declaration
  name: (identifier) @destructor.name
  body: (block) @destructor.body) @lifecycle.relationship.destructor

; 对象初始化器生命周期
(object_creation_expression
  type: (identifier) @initialized.object.type
  initializer: (initializer_expression
    (assignment_expression
      left: (identifier) @initialized.property
      right: (identifier) @initialization.value))) @lifecycle.relationship.object.initialization

; 集合初始化器生命周期
(object_creation_expression
  type: (identifier) @collection.type
  initializer: (initializer_expression
    (identifier) @collection.element)) @lifecycle.relationship.collection.initialization

; 字段声明生命周期
(field_declaration
  (variable_declaration
    (variable_declarator
      name: (identifier) @field.name
      value: (identifier) @field.initialization))) @lifecycle.relationship.field.declaration

; 属性声明生命周期
(property_declaration
  name: (identifier) @property.name
  (accessor_list
    (accessor_declaration
      name: (identifier) @getter.setter.name
      body: (block) @getter.setter.body))) @lifecycle.relationship.property.declaration

; 自动属性生命周期
(property_declaration
  name: (identifier) @auto.property.name) @lifecycle.relationship.auto.property

; 只读属性生命周期
(property_declaration
  (modifier) @readonly.modifier
  name: (identifier) @readonly.property.name) @lifecycle.relationship.readonly.property

; 事件声明生命周期
(event_declaration
  name: (identifier) @event.name
  type: (identifier) @event.type) @lifecycle.relationship.event.declaration

; 事件访问器生命周期
(event_declaration
  name: (identifier) @event.name
  (accessor_list
    (accessor_declaration
      name: (identifier) @add.remove.name
      body: (block) @add.remove.body))) @lifecycle.relationship.event.accessor

; using语句生命周期
(using_statement
  resource: (identifier) @resource.to.dispose
  body: (block) @using.body) @lifecycle.relationship.using.statement

; using声明生命周期
(local_declaration_statement
  (variable_declaration
    (variable_declarator
      name: (identifier) @using.declaration
      value: (invocation_expression
        function: (identifier) @resource.factory)))) @lifecycle.relationship.using.declaration

; IDisposable实现生命周期
(class_declaration
  name: (identifier) @disposable.class
  base_class_clause: (base_class_clause
    (identifier) @idisposable.interface)) @lifecycle.relationship.implements.idisposable

; Dispose方法生命周期
(method_declaration
  name: (identifier) @dispose.method
  parameters: (parameter_list)) @lifecycle.relationship.dispose.method

; Dispose模式实现生命周期
(method_declaration
  name: (identifier) @dispose.method
  parameters: (parameter_list
    (parameter
      type: (predefined_type) @dispose.bool.param.type
      name: (identifier) @dispose.bool.param))) @lifecycle.relationship.dispose.pattern

; 资源分配生命周期
(invocation_expression
  function: (identifier) @allocation.function
  arguments: (argument_list)) @lifecycle.relationship.resource.allocation

; 资源释放生命周期
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @resource.object
    name: (identifier) @resource.release.method)) @lifecycle.relationship.resource.release

; 异步资源释放生命周期
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @async.resource.object
    name: (identifier) @async.resource.release.method)) @lifecycle.relationship.async.resource.release

; 异步using生命周期
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @async.disposable.object
    name: (identifier) @using.async.method)) @lifecycle.relationship.async.using

; Finalizer模式生命周期
(destructor_declaration
  name: (identifier) @finalizer.name
  body: (block
    (invocation_expression
      function: (member_access_expression
        expression: (identifier) @disposable.object
        name: (identifier) @dispose.method)
      arguments: (argument_list
        (argument
          (false_literal)))))) @lifecycle.relationship.finalizer.pattern

; WeakReference生命周期
(object_creation_expression
  type: (identifier) @weak.reference.type
  arguments: (argument_list
    (argument
      (identifier) @referenced.object))) @lifecycle.relationship.weak.reference

; WeakEventManager生命周期
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @event.manager
    name: (identifier) @add.weak.method)
  arguments: (argument_list
    (argument
      (identifier) @source.object)
    (argument
      (identifier) @event.name))) @lifecycle.relationship.weak.event.manager

; 异步生命周期 - 异步方法
(method_declaration
  (modifier) @async.modifier
  name: (identifier) @async.method.name
  body: (block) @async.method.body) @lifecycle.relationship.async.method

; 异步生命周期 - await表达式
(await_expression
  (identifier) @awaited.expression) @lifecycle.relationship.await

; 任务创建生命周期
(object_creation_expression
  type: (identifier) @task.type
  arguments: (argument_list)) @lifecycle.relationship.task.creation

; 任务开始生命周期
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @task.object
    name: (identifier) @task.start.method)) @lifecycle.relationship.task.start

; 任务完成生命周期
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @task.object
    name: (identifier) @task.wait.method)) @lifecycle.relationship.task.completion

; 协程生命周期
(yield_statement
  (identifier) @yielded.value) @lifecycle.relationship.coroutine.yield

; 协程结束生命周期
(yield_statement) @lifecycle.relationship.coroutine.end

; 延迟初始化生命周期
(object_creation_expression
  type: (identifier) @lazy.type
  arguments: (argument_list
    (argument
      (identifier) @lazy.value.factory))) @lifecycle.relationship.lazy.initialization

; 延迟线程安全初始化生命周期
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @lazy.object
    name: (identifier) @lazy.thread.safe.init.method)) @lifecycle.relationship.lazy.thread.safe

; 静态字段初始化生命周期
(field_declaration
  (modifier) @static.modifier
  (variable_declaration
    (variable_declarator
      name: (identifier) @static.field.name
      value: (identifier) @static.field.initialization))) @lifecycle.relationship.static.field.initialization

; 静态类初始化生命周期
(class_declaration
  (modifier) @static.modifier
  name: (identifier) @static.class.name) @lifecycle.relationship.static.class

; 局部变量声明生命周期
(local_declaration_statement
  (variable_declaration
    type: (identifier) @local.variable.type
    (variable_declarator
      name: (identifier) @local.variable.name
      value: (identifier) @local.variable.initialization))) @lifecycle.relationship.local.variable.declaration

; 局部函数生命周期
(local_function_statement
  name: (identifier) @local.function.name
  body: (block) @local.function.body) @lifecycle.relationship.local.function

; 块作用域生命周期
(block
  (local_declaration_statement
    (variable_declaration
      (variable_declarator
        name: (identifier) @scoped.variable.name)))) @lifecycle.relationship.block.scope

; 参数生命周期
(parameter
  type: (identifier) @parameter.type
  name: (identifier) @parameter.name) @lifecycle.relationship.parameter

; 引用参数生命周期
(parameter
  (modifier) @ref.out.modifier
  type: (identifier) @ref.parameter.type
  name: (identifier) @ref.parameter.name) @lifecycle.relationship.reference.parameter

; 只读引用参数生命周期
(parameter
  (modifier) @in.modifier
  type: (ref_type) @in.parameter.type
  name: (identifier) @in.parameter.name) @lifecycle.relationship.in.parameter

; 可空类型生命周期
(nullable_type
  type: (identifier) @nullable.base.type) @lifecycle.relationship.nullable.type

; 可空值处理生命周期
(binary_expression
  left: (member_access_expression
    expression: (identifier) @nullable.variable
    name: (identifier) @nullable.has.value)
  operator: (identifier) @null.check.operator
  right: (true_literal)) @lifecycle.relationship.nullable.check

; 模式匹配生命周期
(is_pattern_expression
  expression: (identifier) @pattern.matched.expression
  pattern: (constant_pattern
    (null_literal))) @lifecycle.relationship.pattern.matching

; 空合并操作生命周期
(binary_expression
  left: (identifier) @left.operand
  operator: "??"
  right: (identifier) @right.operand) @lifecycle.relationship.null.coalescing

; 空条件操作生命周期
(member_access_expression
  expression: (identifier) @safe.nav.object
  name: (identifier) @safe.nav.property
  operator: "?") @lifecycle.relationship.null.conditional

; 内存分配生命周期
(object_creation_expression
  type: (identifier) @allocated.type
  arguments: (argument_list)) @lifecycle.relationship.memory.allocation

; 内存释放生命周期
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @memory.object
    name: (identifier) @memory.dispose.method)) @lifecycle.relationship.memory.disposal

; 作用域生命周期 - scoped引用
(parameter
  (modifier) @scoped.modifier
  type: (ref_type) @scoped.ref.type
  name: (identifier) @scoped.ref.name) @lifecycle.relationship.scoped.reference

; 作用域生命周期 - scoped本地变量
(local_declaration_statement
  (variable_declaration
    type: (ref_type) @scoped.local.type
    (variable_declarator
      name: (identifier) @scoped.local.name))) @lifecycle.relationship.scoped.local

; 记录类型生命周期
(record_declaration
  name: (identifier) @record.name
  (parameter_list
    (parameter
      type: (identifier) @record.parameter.type
      name: (identifier) @record.parameter.name))) @lifecycle.relationship.record.type

; with表达式生命周期
(with_expression
  (identifier) @source.record
  (with_initializer
    (assignment_expression
      left: (identifier) @property.to.change
      right: (identifier) @new.property.value))) @lifecycle.relationship.with.expression

; 初始化器表达式生命周期
(initializer_expression
  (assignment_expression
    left: (identifier) @initialized.property
    right: (identifier) @initialization.value)) @lifecycle.relationship.initializer.expression

; 委托生命周期 - 事件订阅
(assignment_expression
  left: (member_access_expression
    expression: (identifier) @event.source
    name: (identifier) @event.name)
  right: (identifier) @event.handler) @lifecycle.relationship.event.subscription

; 委托生命周期 - 事件取消订阅
(assignment_expression
  left: (member_access_expression
    expression: (identifier) @event.source
    name: (identifier) @event.name)
  right: (identifier) @event.handler) @lifecycle.relationship.event.unsubscription

; 委托生命周期 - 委托组合
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @delegate.instance
    name: (identifier) @delegate.composition.method)
  arguments: (argument_list
    (argument
      (identifier) @delegate.to.combine))) @lifecycle.relationship.delegate.composition

; 委托生命周期 - 委托移除
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @delegate.instance
    name: (identifier) @delegate.remove.method)
  arguments: (argument_list
    (argument
      (identifier) @delegate.to.remove))) @lifecycle.relationship.delegate.removal

; 异步流生命周期
(invocation_expression
  function: (identifier) @async.enumerable.method
  arguments: (argument_list)) @lifecycle.relationship.async.enumerable

; 异步流迭代生命周期
(await_expression
  (member_access_expression
    expression: (identifier) @async.enumerable
    name: (identifier) @async.enumerator.get.method)) @lifecycle.relationship.async.enumerator

; 配置生命周期 - 配置类
(class_declaration
  name: (identifier) @configuration.class
  body: (declaration_list
    (property_declaration
      name: (identifier) @config.property.name
      type: (identifier) @config.property.type))) @lifecycle.relationship.configuration.class

; 配置生命周期 - 配置注入
(parameter
  type: (identifier) @config.injected.type
  name: (identifier) @config.parameter.name) @lifecycle.relationship.configuration.injection
`;