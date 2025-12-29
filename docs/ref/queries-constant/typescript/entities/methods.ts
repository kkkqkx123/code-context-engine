/*
TypeScript Method-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的方法定义查询 - 使用交替模式合并重复查询
[
  (method_definition
    name: (property_identifier) @method.name)
  (method_signature
    name: (property_identifier) @method.signature.name)
  (abstract_method_signature
    name: (property_identifier) @abstract.method.signature.name)
] @definition.method

; Getter/Setter methods - important for property access
(method_definition
  name: (property_identifier) @accessor.name
  (#match? @accessor.name "^(get|set).*")) @definition.accessor

; 异步方法查询
(method_definition
  "async"
  name: (property_identifier) @async.method.name) @definition.async_method

; 静态方法查询
(method_definition
  "static"
  name: (property_identifier) @static.method.name) @definition.static_method

; 私有方法查询
(method_definition
  name: (private_property_identifier) @private.method.name) @definition.private_method

; 带参数的方法查询 - 提供更多上下文信息
(method_definition
  name: (property_identifier) @method.name
  parameters: (formal_parameters
    (required_parameter
      name: (identifier) @param.name
      type: (type_annotation)? @param.type)*)
  return_type: (type_annotation)? @return.type) @definition.method.with_params
`;