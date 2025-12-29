/*
C++ and C# Shared Dependency Relationships Tree-Sitter Query Patterns
用于识别和分析代码中的依赖关系
包含C++特有的头文件包含、using声明、模板依赖等
包含C#特有的using指令、程序集引用、NuGet包依赖等
*/
export default `
; ===== C++ 特定依赖关系 =====

; 头文件包含依赖关系
(preproc_include
  path: (string_literal) @dependency.include.path) @dependency.relationship.include

; 系统库包含依赖关系
(preproc_include
  path: (system_lib_string) @dependency.system.path) @dependency.relationship.system

; using声明依赖关系
(using_declaration
  scope: (nested_name_specifier) @dependency.using.scope
  name: (identifier) @dependency.using.name) @dependency.relationship.using.declaration

; using指令依赖关系
(using_directive
  scope: (nested_name_specifier) @dependency.using.directive.scope
  name: (identifier) @dependency.using.directive.name) @dependency.relationship.using.directive

; 模板类型依赖关系
(template_type
  name: (type_identifier) @dependency.template.type
  arguments: (template_argument_list
    (type_identifier) @dependency.template.argument)*) @dependency.relationship.template.type

; 模板函数依赖关系
(call_expression
  function: (template_function
    name: (identifier) @dependency.template.function
    arguments: (template_argument_list
      (type_identifier) @dependency.template.function.argument)*)) @dependency.relationship.template.function

; 命名空间依赖关系
(namespace_definition
  name: (nested_name_specifier) @dependency.namespace.scope) @dependency.relationship.namespace

; 友元声明依赖关系
(friend_declaration
  (type_identifier) @dependency.friend.type) @dependency.relationship.friend.type

(friend_declaration
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @dependency.friend.function))) @dependency.relationship.friend.function

; ===== 共享依赖关系 =====

; 类型引用依赖关系
(declaration
  type: (type_identifier) @dependency.type.reference
  declarator: (identifier) @dependency.variable.name) @dependency.relationship.type

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

; 函数参数类型依赖关系
(parameter_declaration
  type: (type_identifier) @dependency.parameter.type
  declarator: (identifier) @dependency.parameter.name) @dependency.relationship.parameter.type

; 返回类型依赖关系
(function_definition
  declarator: (function_declarator
    type: (type_identifier) @dependency.return.type
    declarator: (identifier) @dependency.function.name)) @dependency.relationship.return.type

; 方法调用依赖关系
(call_expression
  function: (identifier) @dependency.called.function
  arguments: (argument_list
    (identifier) @dependency.call.argument)*) @dependency.relationship.function.call

; 字段访问依赖关系
(field_expression
  argument: (identifier) @dependency.object
  field: (field_identifier) @dependency.accessed.field) @dependency.relationship.field.access

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
  (parameter_list
    (parameter
      (modifier) @extension.this.modifier
      type: (type_identifier) @dependency.extended.type
      name: (identifier) @extension.parameter.name))
  name: (identifier) @dependency.extension.method) @dependency.relationship.extension.method

; LINQ表达式依赖关系
(query_expression
  from_clause: (from_clause
    identifier: (identifier) @dependency.linq.variable
    expression: (identifier) @dependency.linq.source))) @dependency.relationship.linq.query
`;