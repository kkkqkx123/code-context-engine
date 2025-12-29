/*
Kotlin Semantic Relationships-specific Tree-Sitter Query Patterns
用于识别方法重写、重载、委托、观察者模式等语义关系
*/
export default `
; 类继承关系 - 使用锚点确保精确匹配
(class_declaration
  name: (simple_identifier) @subclass.class
  (delegation_specifiers
    (user_type
      (simple_identifier) @superclass.class))
  (#not-eq? @subclass.class @superclass.class)) @semantic.class.inheritance

; 接口实现关系 - 使用量词操作符
(class_declaration
  name: (simple_identifier) @implementing.class
  (delegation_specifiers
    (user_type
      (simple_identifier) @implemented.interface)+)) @semantic.interface.implementation

; 接口继承关系
(interface_declaration
  name: (simple_identifier) @subinterface.interface
  (delegation_specifiers
    (user_type
      (simple_identifier) @superinterface.interface)+)) @semantic.interface.inheritance

; 泛型约束关系 - 使用量词操作符
(class_declaration
  name: (simple_identifier) @constrained.class
  (type_constraints
    (type_constraint
      (simple_identifier) @constrained.type
      (type) @constraint.type)+)) @semantic.generic.constraint

; 委托模式关系 - 使用谓词过滤
(class_declaration
  name: (simple_identifier) @delegating.class
  (delegation_specifiers
    (user_type
      (simple_identifier) @delegate.interface)
    (delegation_specifier
      (simple_identifier) @delegate.object
      (simple_identifier) @by.keyword
      (#match? @by.keyword "^by$")))) @semantic.delegation.pattern

; 观察者模式关系 - 使用谓词过滤
(property_declaration
  name: (simple_identifier) @observed.property
  (modifiers
    (annotation
      name: (simple_identifier) @delegate.annotation
      (#match? @delegate.annotation "^delegate$")))
  (property_delegate
    (simple_identifier) @observer.delegate)) @semantic.observer.pattern

; 发布订阅模式关系 - 使用谓词过滤
(function_declaration
  name: (simple_identifier) @publisher.method
  (#match? @publisher.method "^(emit|publish|notify|broadcast)$")
  parameters: (function_value_parameters
    (function_value_parameter
      name: (simple_identifier) @event.parameter)+)) @semantic.publisher.pattern

; 工厂模式关系 - 使用谓词过滤
(companion_object
  name: (simple_identifier) @companion.name
  body: (class_body
    (function_declaration
      name: (simple_identifier) @factory.method
      return_type: (user_type
        (simple_identifier) @created.type)
      (#match? @factory.method "^(create|build|make|new|getInstance)$")))) @semantic.factory.pattern

; 单例模式关系 - 使用谓词过滤
(object_declaration
  name: (simple_identifier) @singleton.object) @semantic.singleton.pattern

; 策略模式关系 - 使用谓词过滤
(interface_declaration
  name: (simple_identifier) @strategy.interface
  body: (interface_body
    (function_declaration
      name: (simple_identifier) @strategy.method)+)) @semantic.strategy.pattern

; 命令模式关系 - 使用谓词过滤
(class_declaration
  name: (simple_identifier) @command.class
  body: (class_body
    (primary_constructor
      (class_parameters
        (class_parameter
          name: (simple_identifier) @command.parameter)+)))) @semantic.command.pattern

; 适配器模式关系 - 使用谓词过滤
(class_declaration
  name: (simple_identifier) @adapter.class
  (delegation_specifiers
    (user_type
      (simple_identifier) @adapter.interface))
  body: (class_body
    (function_declaration
      name: (simple_identifier) @adapter.method)+)) @semantic.adapter.pattern

; 装饰器模式关系 - 使用谓词过滤
(function_declaration
  receiver: (user_type
    (simple_identifier) @decorated.type)
  name: (simple_identifier) @decorator.method
  parameters: (function_value_parameters
    (function_value_parameter
      name: (simple_identifier) @decorator.parameter)+)) @semantic.decorator.pattern

; 代理模式关系 - 使用谓词过滤
(class_declaration
  name: (simple_identifier) @proxy.class
  body: (class_body
    (property_declaration
      name: (simple_identifier) @proxy.target))) @semantic.proxy.pattern

; 组合模式关系 - 使用谓词过滤
(class_declaration
  name: (simple_identifier) @composite.class
  body: (class_body
    (property_declaration
      name: (simple_identifier) @composite.children
      type: (user_type
        (simple_identifier) @collection.type)))) @semantic.composite.pattern

; 模板方法模式关系 - 使用谓词过滤
(class_declaration
  name: (simple_identifier) @template.class
  body: (class_body
    (function_declaration
      name: (simple_identifier) @template.method
      body: (block
        (call_expression
          function: (navigation_expression
            left: (this_expression)
            right: (simple_identifier) @abstract.method)))))) @semantic.template.pattern

; 迭代器模式关系 - 使用谓词过滤
(function_declaration
  name: (simple_identifier) @iterator.method
  return_type: (user_type
    (simple_identifier) @iterator.type)
  (#match? @iterator.method "^(iterator|asIterable|asSequence)$")) @semantic.iterator.pattern

; 访问者模式关系 - 使用谓词过滤
(interface_declaration
  name: (simple_identifier) @visitor.interface
  body: (interface_body
    (function_declaration
      name: (simple_identifier) @visitor.method
      parameters: (function_value_parameters
        (function_value_parameter
          name: (simple_identifier) @visited.type)+)))) @semantic.visitor.pattern

; 状态模式关系 - 使用谓词过滤
(class_declaration
  (modifiers
    (class_modifier) @_modifier (#eq? @_modifier "sealed"))
  name: (simple_identifier) @state.class
  body: (class_body
    (class_declaration
      name: (simple_identifier) @state.variant)+)) @semantic.state.pattern

; 责任链模式关系 - 使用谓词过滤
(function_declaration
  name: (simple_identifier) @chain.method
  parameters: (function_value_parameters
    (function_value_parameter
      name: (simple_identifier) @chain.handler)+)
  return_type: (user_type
    (simple_identifier) @chain.result)) @semantic.chain.pattern

; 数据类模式关系 - 使用谓词过滤
(class_declaration
  (modifiers
    (annotation
      name: (simple_identifier) @dataclass.annotation
      (#match? @dataclass.annotation "^data$")))
  name: (simple_identifier) @dataclass.name
  body: (class_body
    (primary_constructor
      (class_parameters
        (class_parameter
          name: (simple_identifier) @dataclass.parameter)+)))) @semantic.dataclass.pattern

; 枚举类模式关系 - 使用谓词过滤
(class_declaration
  (modifiers
    (class_modifier) @_modifier (#eq? @_modifier "enum"))
  name: (simple_identifier) @enum.class
  body: (enum_class_body
    (enum_entry
      name: (simple_identifier) @enum.variant)+)) @semantic.enum.pattern

; 内联类模式关系 - 使用谓词过滤
(class_declaration
  (modifiers
    (class_modifier) @_modifier (#eq? @_modifier "inline"))
  name: (simple_identifier) @inline.class
  primary_constructor: (primary_constructor
    (class_parameters
      (class_parameter
        name: (simple_identifier) @inline.value)))) @semantic.inline.pattern

; 伴生对象工厂模式关系 - 使用谓词过滤
(companion_object
  name: (simple_identifier) @companion.factory
  body: (class_body
    (function_declaration
      name: (simple_identifier) @factory.method
      return_type: (user_type
        (simple_identifier) @product.type)
      (#match? @factory.method "^(of|from|valueOf|parse)$")))) @semantic.companion.factory
`;