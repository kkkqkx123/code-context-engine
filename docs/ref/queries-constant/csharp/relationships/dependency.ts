/*
C# Dependency Relationships Tree-Sitter Query Patterns
用于识别和分析代码中的依赖关系
包含C#特有的using指令、程序集引用、NuGet包依赖等
*/
export default `
; using指令依赖关系
(using_directive
  name: (identifier) @dependency.using.namespace) @dependency.relationship.using.namespace

; using静态指令依赖关系
(using_directive
  static: (identifier) @dependency.using.static.class
  name: (identifier) @dependency.using.static.member) @dependency.relationship.using.static

; using别名指令依赖关系
(using_directive
  name: (identifier) @dependency.using.alias.name
  equality: "="
  type: (identifier) @dependency.using.alias.type) @dependency.relationship.using.alias

; 程序集引用依赖关系
(attribute_list
  (attribute
    name: (identifier) @dependency.assembly.reference
    arguments: (attribute_argument_list
      (attribute_argument
        (string_literal) @dependency.assembly.name)))) @dependency.relationship.assembly.reference

; 类型引用依赖关系
(type_identifier) @dependency.type.reference

; 基类依赖关系
(base_class_clause
  (type_identifier) @dependency.base.class) @dependency.relationship.base.class

; 接口实现依赖关系
(base_class_clause
  (type_identifier) @dependency.implemented.interface) @dependency.relationship.implemented.interface

; 泛型类型约束依赖关系
(type_parameter_constraints_clause
 (type_identifier) @dependency.constrained.type
  (type_parameter_constraint
    type: (type_identifier) @dependency.constraint.type)) @dependency.relationship.generic.constraint

; 方法参数类型依赖关系
(parameter
  type: (identifier) @dependency.parameter.type
  name: (identifier) @dependency.parameter.name) @dependency.relationship.parameter.type

; 返回类型依赖关系
(method_declaration
  type: (identifier) @dependency.return.type
 name: (identifier) @dependency.method.name) @dependency.relationship.return.type

; 构造函数参数类型依赖关系
(constructor_declaration
  parameters: (parameter_list
    (parameter
      type: (identifier) @dependency.constructor.parameter.type
      name: (identifier) @dependency.constructor.parameter.name))) @dependency.relationship.constructor.parameter.type

; 方法调用依赖关系
(invocation_expression
  function: (member_access_expression
    expression: (identifier) @dependency.object
    name: (identifier) @dependency.method.name)
  arguments: (argument_list
    (argument
      (identifier) @dependency.argument.value)*)) @dependency.relationship.method.call

; 属性访问依赖关系
(member_access_expression
  expression: (identifier) @dependency.object
  name: (identifier) @dependency.accessed.property) @dependency.relationship.property.access

; 事件订阅依赖关系
(assignment_expression
  left: (member_access_expression
    expression: (identifier) @dependency.event.source
    name: (identifier) @dependency.event.name)
  right: (identifier) @dependency.event.handler) @dependency.relationship.event.subscription

; 扩展方法依赖关系
(method_declaration
  (modifier) @extension.this.modifier
 parameters: (parameter_list
    (parameter
      (modifier) @extension.this.modifier
      type: (identifier) @dependency.extended.type
      name: (identifier) @extension.parameter.name))
  name: (identifier) @dependency.extension.method) @dependency.relationship.extension.method

; LINQ表达式依赖关系
(query_expression
  (from_clause
    identifier: (identifier) @dependency.linq.variable
    expression: (identifier) @dependency.linq.source)) @dependency.relationship.linq.query

; 特性依赖关系
(attribute
  name: (identifier) @dependency.attribute.name
  arguments: (attribute_argument_list
    (attribute_argument) @dependency.attribute.argument)*) @dependency.relationship.attribute.usage

; 委托依赖关系
(delegate_declaration
  name: (identifier) @dependency.delegate.name
  parameters: (parameter_list
    (parameter
      type: (identifier) @dependency.delegate.parameter.type)*)
  return_type: (identifier) @dependency.delegate.return.type) @dependency.relationship.delegate.definition

; 异步方法依赖关系
(method_declaration
  (modifier) @async.modifier
  name: (identifier) @async.method.name
  type: (generic_type
    name: (identifier) @dependency.async.return.type
    type_arguments: (type_argument_list
      (identifier) @dependency.async.type.argument)))) @dependency.relationship.async.method

; await表达式依赖关系
(await_expression
  expression: (identifier) @dependency.awaited.expression) @dependency.relationship.await.expression
`;