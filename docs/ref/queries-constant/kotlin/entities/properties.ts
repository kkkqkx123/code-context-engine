/*
Kotlin Property-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 属性声明查询 - 使用参数化模式
(property_declaration
  (variable_declaration
    name: (simple_identifier) @property.name)
  type: (_) @property.type?
  (expression) @property.initializer?
  (getter) @property.getter?
  (setter) @property.setter?
  (property_delegate) @property.delegate?) @definition.property

; 属性修饰符查询 - 使用谓词过滤
(property_declaration
  (modifiers
    (property_modifier) @property.modifier)
  (variable_declaration
    name: (simple_identifier) @property.name))
  (#match? @property.modifier "^(val|var|lateinit|const)$")) @definition.property.with.modifier

; 变量声明查询 - 使用交替模式
[
  (variable_declaration
    name: (simple_identifier) @var.name
    type: (_) @var.type?
    value: (_) @var.value?)
  (multi_variable_declaration
    (variable_declaration
      name: (simple_identifier) @multi.var.name)+)
] @definition.variable

; Getter和Setter查询 - 使用交替模式
[
  (getter
    (function_body) @getter.body)
  (setter
    (function_body) @setter.body)
] @definition.property.accessor

; 属性委托查询
(property_delegate
  (expression) @delegate.expression) @definition.property.delegate

; 修饰符查询 - 使用量词操作符
(modifiers
  [
    (property_modifier) @property.modifier
    (visibility_modifier) @visibility.modifier
    (inheritance_modifier) @inheritance.modifier
  ]+) @definition.modifiers

; 注解查询
(annotation
  (user_type
    (simple_identifier) @annotation.name)
  (arguments
    (argument
      (simple_identifier) @annotation.arg
      (_)? @annotation.value)*)?) @definition.annotation
`;