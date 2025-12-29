/*
Kotlin Constructor-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的构造函数查询 - 使用交替模式
[
  (primary_constructor
    (class_parameters
      (class_parameter
        name: (simple_identifier) @param.name
        type: (_) @param.type)*)?)
  (secondary_constructor
    (function_value_parameters
      (function_value_parameter
        name: (simple_identifier) @param.name
        type: (_) @param.type)*)*)
] @definition.constructor

; 构造函数参数查询 - 使用谓词过滤
(class_parameter
  name: (simple_identifier) @param.name
  (modifiers
    (parameter_modifier) @param.modifier)?
  type: (_) @param.type
  value: (_) @param.value?) @definition.constructor.parameter

; 函数值参数查询 - 使用量词操作符
(function_value_parameters
  (function_value_parameter
    name: (simple_identifier) @param.name
    type: (_) @param.type
    value: (_) @param.value?)*) @definition.function.parameters

; Lambda参数查询
(lambda_parameter
  name: (simple_identifier) @lambda.param) @definition.lambda.parameter

; 修饰符查询 - 使用量词操作符
(modifiers
  [
    (parameter_modifier) @parameter.modifier
    (visibility_modifier) @visibility.modifier
  ]+) @definition.modifiers
`;