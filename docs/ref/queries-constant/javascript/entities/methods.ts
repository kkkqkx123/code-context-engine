/*
JavaScript Method-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的方法定义查询 - 使用交替模式合并重复查询
[
  (method_definition
    name: (property_identifier) @method.name)
  (generator_method_definition
    name: (property_identifier) @generator.method.name)
] @definition.method

; 特殊方法查询
[
  (method_definition
    name: (property_identifier) @constructor.name
    (#eq? @constructor.name "constructor"))
  (method_definition
    name: (property_identifier) @getter.name
    (#match? @getter.name "^get"))
  (method_definition
    name: (property_identifier) @setter.name
    (#match? @setter.name "^set"))
  (method_definition
    name: (property_identifier) @static.name
    (#match? @static.name "^static"))
  (method_definition
    "async"
    name: (property_identifier) @async.method.name)
  (method_definition
    "*"
    name: (property_identifier) @generator.method.name)
] @definition.special_method

; 私有方法查询
(private_property_identifier) @name.definition.private_method

; Computed method names
(method_definition
  name: (computed_property_name) @computed.method.name) @definition.computed_method

; 带参数的方法查询 - 提供更多上下文信息
(method_definition
  name: (property_identifier) @method.name
  parameters: (formal_parameters
    (required_parameter
      name: (identifier) @param.name)*)?
  body: (statement_block) @method.body) @definition.method.with_params
`;