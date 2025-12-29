/*
Kotlin Class-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的类声明查询 - 使用交替模式和谓词过滤
(class_declaration
  name: (simple_identifier) @class.name
  (modifiers
    (class_modifier) @class.modifier)?
  (type_parameters
    (type_parameter
      name: (simple_identifier) @type.param)*)?
  (primary_constructor
    (class_parameters
      (class_parameter
        name: (simple_identifier) @constructor.param)*)?)?
  body: (class_body) @class.body) @definition.class

; 接口和对象声明查询 - 使用交替模式
[
  (class_declaration
    (modifiers
      (class_modifier) @_modifier (#eq? @_modifier "interface"))
    name: (simple_identifier) @interface.name)
  (object_declaration
    name: (simple_identifier) @object.name)
  (companion_object
    name: (simple_identifier)? @companion.name)
] @definition.interface.or.object

; 枚举和注解类查询 - 使用交替模式
[
  (class_declaration
    (modifiers
      (class_modifier) @_modifier (#eq? @_modifier "enum"))
    name: (simple_identifier) @enum.name)
  (class_declaration
    (modifiers
      (class_modifier) @_modifier (#eq? @_modifier "annotation"))
    name: (simple_identifier) @annotation.name)
] @definition.enum.or.annotation

; 类型别名查询
(type_alias
  name: (simple_identifier) @type.alias.name
  type: (type) @type.alias.type) @definition.type.alias

; 泛型约束查询 - 使用量词操作符
(type_constraints
  (type_constraint
    (simple_identifier) @constrained.type
    (type) @constraint.type)*) @definition.type.constraints

; 委托说明符查询
(delegation_specifiers
  (user_type
    (simple_identifier) @delegate.type)
  (delegation_specifier
    (simple_identifier) @delegate.by)?) @definition.delegation
`;