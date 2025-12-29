/*
Kotlin Function-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的函数声明查询 - 使用交替模式
[
  (function_declaration
    name: (simple_identifier) @function.name
    (modifiers
      (function_modifier) @function.modifier)*
    (type_parameters
      (type_parameter
        name: (simple_identifier) @type.param)*)?
    (function_value_parameters
      (function_value_parameter
        name: (simple_identifier) @param.name)*)?
    (type) @return.type?
    (function_body) @function.body?)
  (function_declaration
    (type) @receiver.type
    name: (simple_identifier) @extension.function)
] @definition.function

; 扩展函数查询 - 使用锚点确保精确匹配
(function_declaration
  (type) @receiver.type
  name: (simple_identifier) @extension.function) @definition.extension.function

; 类型参数查询 - 使用量词操作符
(type_parameters
  (type_parameter
    name: (simple_identifier) @type.param)*) @definition.type.parameters

; 类型约束查询 - 使用量词操作符
(type_constraints
  (type_constraint
    (simple_identifier) @constrained.type
    (type) @constraint.type)*) @definition.type.constraints

; 注解查询 - 使用量词操作符
(annotation
  (user_type
    (simple_identifier) @annotation.name)
  (arguments
    (argument
      (simple_identifier) @annotation.arg
      (_)? @annotation.value)*)?) @definition.annotation

; 修饰符查询 - 使用量词操作符
(modifiers
  [
    (function_modifier) @function.modifier
    (visibility_modifier) @visibility.modifier
    (inheritance_modifier) @inheritance.modifier
    (parameter_modifier) @parameter.modifier
    (type_modifier) @type.modifier
  ]+) @definition.modifiers
`;