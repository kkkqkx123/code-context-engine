/*
Java Method-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的方法声明查询 - 使用交替模式
(method_declaration
  name: (identifier) @method.name) @definition.method

; 带参数的方法查询 - 使用量词操作符
(method_declaration
  name: (identifier) @method.name
  parameters: (formal_parameters
    (formal_parameter
      name: (identifier) @param.name
      type: (_) @param.type)*)
  body: (block) @method.body) @definition.method.with_params

; 构造函数声明查询
(constructor_declaration
  name: (identifier) @constructor.name) @definition.constructor

; 带参数的构造函数查询
(constructor_declaration
  name: (identifier) @constructor.name
  parameters: (formal_parameters
    (formal_parameter
      name: (identifier) @constructor.param
      type: (_) @constructor.type)*)*
  body: (block) @constructor.body) @definition.constructor.with_params

; Lambda表达式查询 - 使用锚点确保精确匹配
(lambda_expression
  parameters: (lambda_parameters
    (identifier) @lambda.param)*
  body: (_) @lambda.body) @definition.lambda

; 类型参数查询 - 使用量词操作符
(type_parameters
  (type_parameter
    name: (identifier) @type.param)*) @definition.type.parameters

; 注解查询 - 使用交替模式
[
  (annotation
    name: (identifier) @annotation.name)
  (marker_annotation
    name: (identifier) @marker.annotation)
] @definition.annotation

; 形式参数查询 - 使用量词操作符
(formal_parameters
  (formal_parameter
    name: (identifier) @param.name
    type: (_) @param.type)*) @definition.parameters

; 修饰符查询
(modifiers
  (modifier) @modifier.name) @definition.modifiers

; 返回类型查询
(method_declaration
  type: (_) @return.type) @definition.return.type
`;