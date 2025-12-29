/*
JavaScript Semantic Relationships-specific Tree-Sitter Query Patterns
用于识别方法重写、重载、委托、观察者模式等语义关系
*/
export default `
; 原型链继承关系（方法重写）
(assignment_expression
  left: (member_expression
    object: (identifier) @subclass.object
    property: (property_identifier) @overridden.method)
  right: (function_expression)) @semantic.relationship.prototype.override

; 类继承关系（方法重写）
(class_declaration
  name: (identifier) @subclass.class
  heritage: (class_heritage
    (identifier) @superclass.class)
  body: (class_body
    (method_definition
      name: (property_identifier) @overridden.method))) @semantic.relationship.class.override

; 混入模式（委托关系）
(call_expression
  function: (member_expression
    object: (identifier) @mixin.object
    property: (property_identifier) @mixin.method)
  arguments: (argument_list
    (identifier) @target.object)) @semantic.relationship.mixin.delegation

; 观察者模式（事件监听）
(call_expression
  function: (member_expression
    object: (identifier) @observer.target
    property: (property_identifier) @observer.method
    (#match? @observer.method "^(addEventListener|on|watch|subscribe)$"))
  arguments: (argument_list
    (string) @event.name
    (function_expression) @handler.function)) @semantic.relationship.observer.pattern

; 发布订阅模式
(call_expression
  function: (member_expression
    object: (identifier) @publisher.object
    property: (property_identifier) @publisher.method
    (#match? @publisher.method "^(emit|publish|notify)$"))
  arguments: (argument_list
    (string) @event.name
    (identifier) @event.data)) @semantic.relationship.publisher.pattern

; 配置对象模式
(call_expression
  function: (identifier) @configurable.function
  arguments: (argument_list
    (object
      (pair
        key: (property_identifier) @config.key
        value: (identifier) @config.value)))) @semantic.relationship.configuration

; 工厂模式
(call_expression
  function: (identifier) @factory.function
  arguments: (argument_list
    (identifier) @factory.parameter)) @semantic.relationship.factory.pattern

; 单例模式
(assignment_expression
  left: (member_expression
    object: (identifier) @singleton.object
    property: (property_identifier) @singleton.instance)
  right: (call_expression
    function: (identifier) @constructor.function)) @semantic.relationship.singleton.pattern

; 装饰器模式（高阶函数）
(call_expression
  function: (identifier) @decorator.function
  arguments: (argument_list
    (function_expression) @decorated.function)) @semantic.relationship.decorator.pattern

; 策略模式
(call_expression
  function: (member_expression
    object: (identifier) @context.object
    property: (property_identifier) @strategy.setter
    (#match? @strategy.setter "^(setStrategy|setAlgorithm)$"))
  arguments: (argument_list
    (identifier) @strategy.object)) @semantic.relationship.strategy.pattern

; 命令模式
(call_expression
  function: (member_expression
    object: (identifier) @invoker.object
    property: (property_identifier) @invoker.method
    (#match? @invoker.method "^(execute|invoke|run)$"))
  arguments: (argument_list
    (identifier) @command.object)) @semantic.relationship.command.pattern
`;