/*
C# Semantic Relationships-specific Tree-Sitter Query Patterns
用于识别方法重写、重载、委托、观察者模式等语义关系
Optimized based on tree-sitter best practices
*/
export default `
; 泛型关系查询 - 使用锚点和量词操作符
[
  (class_declaration
    name: (identifier) @generic.class
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @type.parameter))*) @semantic.generic.class
  (method_declaration
    name: (identifier) @generic.method
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @method.type.parameter))*) @semantic.generic.method
  (type_parameter_constraints_clause
    (identifier) @constrained.type.parameter
    (type_parameter_constraint) @constraint.type) @semantic.generic.constraint
] @semantic.generic.relationship

; 观察者模式查询 - 参数化模式
[
  (event_declaration
    name: (identifier) @event.name
    type: (identifier) @event.type) @semantic.observer.event
  (assignment_expression
    left: (member_access_expression
      expression: (identifier) @subscriber.object
      name: (identifier) @subscriber.event)
    right: (identifier) @event.handler) @semantic.observer.subscription
  (invocation_expression
    function: (member_access_expression
      expression: (identifier) @publisher.object
      name: (identifier) @publisher.event)) @semantic.observer.trigger
] @semantic.observer.pattern

; 委托关系查询 - 使用交替模式
[
  (delegate_declaration
    name: (identifier) @delegate.type
    parameters: (parameter_list) @delegate.params) @semantic.delegate.definition
  (assignment_expression
    left: (identifier) @delegate.instance
    right: (identifier) @delegate.method) @semantic.delegate.assignment
] @semantic.delegate.relationship

; 设计模式查询 - 参数化模式
(class_declaration
 name: (identifier) @pattern.class
  body: (declaration_list
    (field_declaration
      type: (identifier) @pattern.type
      declarators: (variable_declarator_list
        (variable_declarator
          name: (identifier) @pattern.field)))*
    (method_declaration
      name: (identifier) @pattern.method)*)) @semantic.design.pattern

; 依赖注入关系查询 - 使用锚点和谓词
[
  (constructor_declaration
    parameters: (parameter_list
      (parameter
        type: (identifier) @injected.type
        name: (identifier) @injected.parameter))*) @semantic.dependency.injection.constructor
  (property_declaration
    name: (identifier) @injected.property
    type: (identifier) @injected.type
    (attribute
      name: (identifier) @inject.attribute)) @semantic.dependency.injection.property
] @semantic.dependency.injection

; 异步模式关系查询 - 使用谓词过滤
[
  (method_declaration
    (modifier) @async.modifier
    name: (identifier) @async.method
    type: (identifier) @async.return.type
    (#match? @async.modifier "async")) @semantic.async.method
  (await_expression
    expression: (identifier) @await.expression) @semantic.async.await
] @semantic.async.pattern

; LINQ关系查询 - 使用锚点和字段名
(query_expression
  (from_clause
    identifier: (identifier) @linq.variable
    expression: (identifier) @linq.source)
  (query_body
    (select_clause
      expression: (identifier) @linq.result)
    (where_clause
      condition: (identifier) @linq.condition)?
    (orderby_clause
      (ordering) @linq.ordering)*)) @semantic.linq.query

; 特性关系查询 - 使用交替模式
[
  (attribute
    name: (identifier) @attribute.name
    arguments: (attribute_argument_list
      (attribute_argument) @attribute.argument)*) @semantic.attribute.usage
  (class_declaration
    (attribute_list
      (attribute) @class.attribute)*) @semantic.attribute.class
] @semantic.attribute.relationship

; 分部类型关系查询 - 使用谓词过滤
[
  (class_declaration
    (modifier) @partial.modifier
    name: (identifier) @partial.class
    (#match? @partial.modifier "partial")) @semantic.partial.class
  (method_declaration
    (modifier) @partial.modifier
    name: (identifier) @partial.method
    (#match? @partial.modifier "partial")) @semantic.partial.method
] @semantic.partial.relationship

; 接口显式实现查询 - 使用锚点确保精确匹配
(method_declaration
  (explicit_interface_specifier
    (identifier) @interface.name)
  name: (identifier) @implemented.method) @semantic.explicit.interface.implementation

; 属性和索引器重写查询 - 使用谓词过滤
[
  (property_declaration
    (modifier) @override.modifier
    name: (identifier) @overridden.property
    (#match? @override.modifier "override")) @semantic.override.property
  (indexer_declaration
    (modifier) @override.modifier
    type: (identifier) @overridden.indexer.type
    (#match? @override.modifier "override")) @semantic.override.indexer
] @semantic.override.member
`;