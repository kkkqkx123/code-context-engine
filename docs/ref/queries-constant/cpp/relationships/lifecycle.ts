/*
C++ Lifecycle Relationships-specific Tree-Sitter Query Patterns
用于识别对象实例化、初始化、销毁、生命周期管理等关系
*/
export default `
; 对象实例化关系
(new_expression
  type: (type_identifier) @instantiated.class
  arguments: (argument_list
    (identifier) @constructor.parameter)) @lifecycle.relationship.instantiation

; 模板类实例化关系
(new_expression
  type: (template_type
    name: (type_identifier) @instantiated.template.class
    arguments: (template_argument_list))) @lifecycle.relationship.template.instantiation

; 构造函数定义
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @constructor.method)
  body: (compound_statement)) @lifecycle.relationship.constructor.definition

; 析构函数定义
(function_definition
  declarator: (function_declarator
    declarator: (destructor_name) @destructor.method)
  body: (compound_statement)) @lifecycle.relationship.destructor.definition

; 构造函数初始化列表
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @constructor.method)
  body: (compound_statement
    (constructor_initializer_list
      (constructor_initializer
        name: (field_identifier) @initialized.member
        value: (identifier) @init.value)))) @lifecycle.relationship.constructor.initializer

; 成员初始化列表
(constructor_initializer
  name: (field_identifier) @initialized.member
  value: (identifier) @init.value) @lifecycle.relationship.member.initialization

; 委托构造函数
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @delegating.constructor)
  body: (compound_statement
    (constructor_initializer_list
      (constructor_initializer
        name: (identifier) @target.constructor
        value: (argument_list))))) @lifecycle.relationship.delegating.constructor

; 继承构造函数
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @inherited.constructor)
  body: (compound_statement
    (inheriting_constructor))) @lifecycle.relationship.inherited.constructor

; 拷贝构造函数
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @copy.constructor)
  parameters: (parameter_list
    (parameter_declaration
      type: (reference_type
        type: (type_identifier) @source.class)))) @lifecycle.relationship.copy.constructor

; 移动构造函数
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @move.constructor)
  parameters: (parameter_list
    (parameter_declaration
      type: (reference_type
        type: (type_identifier) @source.class)
      (type_qualifier) @rvalue.qualifier))) @lifecycle.relationship.move.constructor

; 拷贝赋值运算符
(function_definition
  declarator: (function_declarator
    declarator: (operator_name) @copy.assignment)
  parameters: (parameter_list
    (parameter_declaration
      type: (reference_type
        type: (type_identifier) @source.class)))) @lifecycle.relationship.copy.assignment

; 移动赋值运算符
(function_definition
  declarator: (function_declarator
    declarator: (operator_name) @move.assignment)
  parameters: (parameter_list
    (parameter_declaration
      type: (reference_type
        type: (type_identifier) @source.class)
      (type_qualifier) @rvalue.qualifier))) @lifecycle.relationship.move.assignment

; RAII资源获取
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @resource.acquire.method)
  body: (compound_statement
    (expression_statement
      (call_expression
        function: (identifier) @resource.function)))) @lifecycle.relationship.resource.acquisition

; RAII资源释放
(function_definition
  declarator: (function_declarator
    declarator: (destructor_name) @resource.release.method)
  body: (compound_statement
    (expression_statement
      (call_expression
        function: (identifier) @cleanup.function)))) @lifecycle.relationship.resource.release

; 智能指针创建
(call_expression
  function: (qualified_identifier
    scope: (identifier) @std.scope
    name: (identifier) @make.function)
  arguments: (argument_list
    (identifier) @managed.resource))
  (#match? @make.function "^(make_unique|make_shared)$") @lifecycle.relationship.smart.pointer.creation

; 智能指针重置
(call_expression
  function: (field_expression
    object: (identifier) @smart.pointer
    field: (field_identifier) @reset.method)
  arguments: (argument_list
    (identifier) @new.resource)) @lifecycle.relationship.smart.pointer.reset

; 作用域退出自动清理
(declaration
  type: (type_identifier) @scoped.resource.type
  declarator: (init_declarator
    declarator: (identifier) @scoped.variable)) @lifecycle.relationship.scoped.resource

; 异常安全的资源管理
(try_statement
  body: (compound_statement
    (expression_statement
      (call_expression
        function: (identifier) @resource.acquire)))
  (catch_clause
    parameter: (parameter_declaration
      declarator: (identifier) @exception.parameter)
    body: (compound_statement
      (expression_statement
        (call_expression
          function: (identifier) @resource.cleanup))))) @lifecycle.relationship.exception.safe.resource

; 对象生命周期注解
(function_definition
  (attribute_specifier
    (attribute
      (identifier) @lifecycle.attribute))
  declarator: (function_declarator
    declarator: (identifier) @lifecycle.method)
  (#match? @lifecycle.attribute "^(constructor|destructor|init|cleanup)$")) @lifecycle.relationship.annotated.method

; 静态初始化
(declaration
  (storage_class_specifier) @static.specifier
  declarator: (init_declarator
    declarator: (identifier) @static.variable
    value: (identifier) @init.value)) @lifecycle.relationship.static.initialization

; 动态内存分配
(new_expression
  type: (type_identifier) @allocated.type) @lifecycle.relationship.dynamic.allocation

; 动态内存释放
(delete_expression
  argument: (identifier) @deleted.pointer) @lifecycle.relationship.dynamic.deallocation

; 数组动态分配
(new_expression
  type: (array_type
    type: (type_identifier) @array.type
    dimension: (identifier) @array.size)) @lifecycle.relationship.array.allocation

; 数组动态释放
(delete_expression
  argument: (identifier) @deleted.array.pointer)
  (#match? @deleted.array.pointer "delete\\[\\]") @lifecycle.relationship.array.deallocation

; 容器生命周期管理
(call_expression
  function: (field_expression
    object: (identifier) @container.object
    field: (field_identifier) @container.method)
  arguments: (argument_list
    (identifier) @container.element))
  (#match? @container.method "^(push_back|push_front|insert|emplace)$") @lifecycle.relationship.container.element.creation

; 容器元素销毁
(call_expression
  function: (field_expression
    object: (identifier) @container.object
    field: (field_identifier) @container.method)
  arguments: (argument_list))
  (#match? @container.method "^(pop_back|pop_front|erase|clear)$") @lifecycle.relationship.container.element.destruction

; 文件资源管理
(call_expression
  function: (field_expression
    object: (identifier) @file.object
    field: (field_identifier) @file.method)
  arguments: (argument_list
    (string_literal) @file.name))
  (#match? @file.method "^(open|create)$") @lifecycle.relationship.file.open

(call_expression
  function: (field_expression
    object: (identifier) @file.object
    field: (field_identifier) @file.method)
  arguments: (argument_list))
  (#match? @file.method "^(close|flush)$") @lifecycle.relationship.file.close

; 线程生命周期管理
(call_expression
  function: (field_expression
    object: (identifier) @thread.object
    field: (field_identifier) @thread.method))
  (#match? @thread.method "^(join|detach)$") @lifecycle.relationship.thread.management

; 锁生命周期管理
(call_expression
  function: (field_expression
    object: (identifier) @lock.object
    field: (field_identifier) @lock.method))
  (#match? @lock.method "^(lock|unlock|try_lock)$") @lifecycle.relationship.lock.management

; 作用域守卫
(class_specifier
  name: (type_identifier) @scope.guard.class
  body: (field_declaration_list
    (function_definition
      declarator: (function_declarator
        declarator: (identifier) @guard.constructor))
    (function_definition
      declarator: (function_declarator
        declarator: (destructor_name) @guard.destructor)))) @lifecycle.relationship.scope.guard

; 延迟初始化
(class_specifier
  name: (type_identifier) @lazy.class
  body: (field_declaration_list
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @lazy.field))
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @get.method)))) @lifecycle.relationship.lazy.initialization

; 单例生命周期
(class_specifier
  name: (type_identifier) @singleton.class
  body: (field_declaration_list
    (field_declaration
      (storage_class_specifier) @static.specifier
      declarator: (field_declarator
        declarator: (field_identifier) @instance.field))
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @get.instance.method)))) @lifecycle.relationship.singleton.lifecycle

; 对象池模式
(class_specifier
  name: (type_identifier) @object.pool.class
  body: (field_declaration_list
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @acquire.method))
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @release.method)))) @lifecycle.relationship.object.pool

; 引用计数生命周期
(class_specifier
  name: (type_identifier) @reference.counted.class
  body: (field_declaration_list
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @ref.count.field))
    (function_definition
      declarator: (function_declarator
        declarator: (identifier) @add.ref.method))
    (function_definition
      declarator: (function_declarator
        declarator: (identifier) @release.ref.method)))) @lifecycle.relationship.reference.counting

; 观察者生命周期
(class_specifier
  name: (type_identifier) @observable.class
  body: (field_declaration_list
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @add.observer.method))
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @remove.observer.method)))) @lifecycle.relationship.observer.lifecycle

; 状态机生命周期
(class_specifier
  name: (type_identifier) @state.machine.class
  body: (field_declaration_list
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @current.state.field))
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @transition.method)))) @lifecycle.relationship.state.machine
`;