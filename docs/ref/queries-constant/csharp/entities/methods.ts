/*
C# Method-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
Optimized based on tree-sitter best practices
*/
export default `
; 方法声明查询 - 使用锚点和字段名
(method_declaration
  name: (identifier) @method.name
  type: (identifier) @method.return.type
  parameters: (parameter_list
    (parameter
      name: (identifier) @param.name
      type: (identifier) @param.type)*)?
  body: (block) @method.body) @definition.method

; 运算符声明查询 - 使用交替模式
[
  (operator_declaration
    name: (identifier) @operator.name
    type: (identifier) @operator.return.type
    parameters: (parameter_list
      (parameter
        name: (identifier) @param.name
        type: (identifier) @param.type))*
    body: (block) @operator.body) @definition.operator
  (conversion_operator_declaration
    name: (identifier) @conversion.operator.name
    type: (identifier) @conversion.operator.type
    parameters: (parameter_list
      (parameter
        name: (identifier) @param.name
        type: (identifier) @param.type))
    body: (block) @conversion.operator.body) @definition.conversion.operator
] @definition.operator.declaration

; Lambda表达式查询 - 使用交替模式
[
  (lambda_expression
    parameters: (parameter_list
      (parameter
        name: (identifier) @lambda.param)*)?
    body: (_) @lambda.body) @definition.lambda
  (anonymous_method_expression
    parameters: (parameter_list
      (parameter
        name: (identifier) @anonymous.param)*)?
    body: (block) @anonymous.body) @definition.anonymous.method
] @definition.function.expression

; 属性访问器查询 - 使用交替模式和量词操作符
[
  (accessor_declaration
    name: (identifier) @accessor.name
    body: (block) @accessor.body) @definition.accessor
  (get_accessor_declaration
    body: (block) @get.body) @definition.get.accessor
  (set_accessor_declaration
    body: (block) @set.body) @definition.set.accessor
  (init_accessor_declaration
    body: (block) @init.body) @definition.init.accessor
  (add_accessor_declaration
    body: (block) @add.body) @definition.add.accessor
  (remove_accessor_declaration
    body: (block) @remove.body) @definition.remove.accessor
] @definition.property.accessor

; 异步方法查询 - 使用谓词过滤
(method_declaration
  (modifier) @async.modifier
  name: (identifier) @async.method.name
  type: (identifier) @async.return.type
  body: (block) @async.method.body
  (#match? @async.modifier "async")) @definition.async.method

; 泛型方法查询 - 使用锚点和量词操作符
(method_declaration
  name: (identifier) @generic.method.name
  type_parameters: (type_parameter_list
    (type_parameter
      name: (identifier) @type.parameter)*)?
  parameters: (parameter_list
    (parameter
      name: (identifier) @param.name
      type: (identifier) @param.type)*)?
  body: (block) @generic.method.body) @definition.generic.method

; 扩展方法查询 - 使用谓词过滤
(method_declaration
  (modifier) @this.modifier
  name: (identifier) @extension.method.name
  parameters: (parameter_list
    (parameter
      (modifier) @this.modifier
      type: (identifier) @extended.type
      name: (identifier) @extended.instance)
    (parameter
      name: (identifier) @param.name
      type: (identifier) @param.type)*)?
  body: (block) @extension.method.body
  (#match? @this.modifier "this")) @definition.extension.method

; 重写方法查询 - 使用谓词过滤
(method_declaration
  (modifier) @override.modifier
  name: (identifier) @override.method.name
  type: (identifier) @override.return.type
  parameters: (parameter_list
    (parameter
      name: (identifier) @param.name
      type: (identifier) @param.type)*)?
  body: (block) @override.method.body
  (#match? @override.modifier "override")) @definition.override.method

; 虚方法和抽象方法查询 - 使用谓词过滤
[
  (method_declaration
    (modifier) @virtual.modifier
    name: (identifier) @virtual.method.name
    type: (identifier) @virtual.return.type
    parameters: (parameter_list
      (parameter
        name: (identifier) @param.name
        type: (identifier) @param.type)*)?
    body: (block) @virtual.method.body
    (#match? @virtual.modifier "virtual")) @definition.virtual.method
  (method_declaration
    (modifier) @abstract.modifier
    name: (identifier) @abstract.method.name
    type: (identifier) @abstract.return.type
    parameters: (parameter_list
      (parameter
        name: (identifier) @param.name
        type: (identifier) @param.type)*)?
    (#match? @abstract.modifier "abstract")) @definition.abstract.method
] @definition.polymorphic.method

; 静态方法查询 - 使用谓词过滤
(method_declaration
  (modifier) @static.modifier
  name: (identifier) @static.method.name
  type: (identifier) @static.return.type
  parameters: (parameter_list
    (parameter
      name: (identifier) @param.name
      type: (identifier) @param.type)*)?
  body: (block) @static.method.body
  (#match? @static.modifier "static")) @definition.static.method

; 分部方法查询 - 使用谓词过滤
(method_declaration
  (modifier) @partial.modifier
  name: (identifier) @partial.method.name
  type: (identifier) @partial.return.type
  parameters: (parameter_list
    (parameter
      name: (identifier) @param.name
      type: (identifier) @param.type)*)?
  body: (block)? @partial.method.body
  (#match? @partial.modifier "partial")) @definition.partial.method
`;