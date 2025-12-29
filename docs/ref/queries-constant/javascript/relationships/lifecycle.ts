/*
JavaScript Lifecycle Relationships-specific Tree-Sitter Query Patterns
用于识别对象实例化、初始化、销毁、生命周期管理等关系
*/
export default `
; 对象实例化关系
(new_expression
  constructor: (identifier) @instantiated.class
  arguments: (argument_list
    (identifier) @constructor.parameter)) @lifecycle.relationship.instantiation

; 类实例化关系
(new_expression
  constructor: (member_expression
    object: (identifier) @module.object
    property: (identifier) @instantiated.class)
  arguments: (argument_list
    (identifier) @constructor.parameter)) @lifecycle.relationship.class.instantiation

; 构造函数调用关系
(call_expression
  function: (member_expression
    object: (this) @constructor.this
    property: (property_identifier) @constructor.method
    (#match? @constructor.method "constructor$"))) @lifecycle.relationship.constructor.call

; 原型方法初始化
(assignment_expression
  left: (member_expression
    object: (member_expression
      object: (identifier) @class.object
      property: (property_identifier) @prototype.property)
    property: (property_identifier) @init.method)
  right: (function_expression)) @lifecycle.relationship.prototype.initialization

; 对象初始化方法
(call_expression
  function: (member_expression
    object: (identifier) @initialized.object
    property: (property_identifier) @init.method
    (#match? @init.method "^(init|initialize|setup|configure)$"))
  arguments: (argument_list
    (identifier) @init.parameter)) @lifecycle.relationship.object.initialization

; React组件生命周期
(method_definition
  name: (property_identifier) @lifecycle.method
  (#match? @lifecycle.method "^(componentDidMount|componentDidUpdate|componentWillUnmount|useEffect|useLayoutEffect)$")) @lifecycle.relationship.react.lifecycle

; 销毁关系
(call_expression
  function: (member_expression
    object: (identifier) @destroyed.object
    property: (property_identifier) @destroy.method
    (#match? @destroy.method "^(destroy|dispose|cleanup|teardown|close)$"))) @lifecycle.relationship.destruction

; 事件监听器添加（生命周期管理）
(call_expression
  function: (member_expression
    object: (identifier) @event.target
    property: (property_identifier) @add.listener.method
    (#match? @add.listener.method "^(addEventListener|addListener|on)$"))
  arguments: (argument_list
    (string) @event.name
    (identifier) @handler.function)) @lifecycle.relationship.listener.addition

; 事件监听器移除（生命周期管理）
(call_expression
  function: (member_expression
    object: (identifier) @event.target
    property: (property_identifier) @remove.listener.method
    (#match? @remove.listener.method "^(removeEventListener|removeListener|off)$"))
  arguments: (argument_list
    (string) @event.name
    (identifier) @handler.function)) @lifecycle.relationship.listener.removal

; 定时器创建（生命周期管理）
(call_expression
  function: (identifier) @timer.function
  (#match? @timer.function "^(setTimeout|setInterval)$")
  arguments: (argument_list
    (function_expression) @timer.handler
    (identifier) @timer.delay)) @lifecycle.relationship.timer.creation

; 定时器清除（生命周期管理）
(call_expression
  function: (identifier) @clear.timer.function
  (#match? @clear.timer.function "^(clearTimeout|clearInterval)$")
  arguments: (argument_list
    (identifier) @timer.id)) @lifecycle.relationship.timer.clearance

; Promise创建（异步生命周期）
(call_expression
  function: (identifier) @promise.constructor
  (#match? @promise.constructor "Promise$")
  arguments: (argument_list
    (function_expression) @promise.executor)) @lifecycle.relationship.promise.creation

; 异步资源管理
(call_expression
  function: (member_expression
    object: (identifier) @async.resource
    property: (property_identifier) @async.method
    (#match? @async.method "^(acquire|release|open|close|start|stop)$"))) @lifecycle.relationship.async.resource.management
`;