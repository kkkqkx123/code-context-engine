/*
Kotlin Function Annotation Tree-Sitter Query Patterns
Shared queries for function annotations used across entities and relationships
*/
export default `
; 函数修饰符注解
(function_declaration
  (modifiers
    (function_modifier) @function.modifier)*
  name: (simple_identifier) @function.name
  body: (function_body) @function.body) @function.modifier.annotation

; 扩展函数注解
(function_declaration
  (type) @receiver.type
  name: (simple_identifier) @extension.function
  body: (function_body) @extension.body) @extension.function.annotation

; 泛型函数注解
(function_declaration
  type_parameters: (type_parameters
    (type_parameter
      name: (simple_identifier) @type.param)*)?
  name: (simple_identifier) @generic.function
  body: (function_body) @generic.body) @generic.function.annotation

; 挂起函数注解
(function_declaration
  (modifiers
    (function_modifier) @suspend.modifier
    (#match? @suspend.modifier "^suspend$"))
  name: (simple_identifier) @suspend.function
  body: (function_body) @suspend.body) @suspend.function.annotation

; 内联函数注解
(function_declaration
  (modifiers
    (function_modifier) @inline.modifier
    (#match? @inline.modifier "^inline$"))
  name: (simple_identifier) @inline.function
  body: (function_body) @inline.body) @inline.function.annotation

; 尾递归函数注解
(function_declaration
  (modifiers
    (function_modifier) @tailrec.modifier
    (#match? @tailrec.modifier "^tailrec$"))
  name: (simple_identifier) @tailrec.function
  body: (function_body) @tailrec.body) @tailrec.function.annotation

; 运算符重载函数注解
(function_declaration
  (modifiers
    (function_modifier) @operator.modifier
    (#match? @operator.modifier "^operator$"))
  name: (simple_identifier) @operator.function
  body: (function_body) @operator.body) @operator.function.annotation

; 中缀函数注解
(function_declaration
  (modifiers
    (function_modifier) @infix.modifier
    (#match? @infix.modifier "^infix$"))
  name: (simple_identifier) @infix.function
  body: (function_body) @infix.body) @infix.function.annotation

; 外部函数注解
(function_declaration
  (modifiers
    (function_modifier) @external.modifier
    (#match? @external.modifier "^external$"))
  name: (simple_identifier) @external.function) @external.function.annotation

; 函数注解
(annotation
  (user_type
    (simple_identifier) @annotation.name)
  (arguments
    (argument
      (simple_identifier) @annotation.arg
      (_)? @annotation.value)*)?
  .
  (function_declaration
    name: (simple_identifier) @annotated.function)) @function.annotation

; 方法注解
(annotation
  (user_type
    (simple_identifier) @annotation.name)
  (arguments
    (argument
      (simple_identifier) @annotation.arg
      (_)? @annotation.value)*)?
  .
  (function_declaration
    name: (simple_identifier) @annotated.method)) @method.annotation

; 构造函数注解
(annotation
  (user_type
    (simple_identifier) @annotation.name)
  (arguments
    (argument
      (simple_identifier) @annotation.arg
      (_)? @annotation.value)*)?
  .
  (secondary_constructor
    (function_value_parameters) @constructor.params
    (block) @constructor.body)) @constructor.annotation

; Lambda注解
(lambda_literal
  (modifiers
    (function_modifier) @lambda.modifier)*
  parameters: (lambda_parameters
    (variable_declaration
      name: (simple_identifier) @lambda.param)*)?
  body: (_) @lambda.body) @lambda.annotation
`;