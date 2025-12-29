/*
TypeScript Class-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的类声明查询 - 使用交替模式合并重复查询
[
  (class_declaration
    name: (type_identifier) @class.name)
  (abstract_class_declaration
    name: (type_identifier) @abstract.class.name)
] @definition.class

; Generic class declarations - important for type understanding
(class_declaration
  type_parameters: (type_parameters)
  name: (type_identifier) @generic.class.name) @definition.generic_class

; Constructor - important class entry point
(method_definition
  name: (property_identifier) @constructor.name
  (#eq? @constructor.name "constructor")) @definition.constructor

; Static initialization blocks - important for class setup
(class_static_block) @definition.static_block

; 带继承的类查询 - 使用锚点操作符
(class_declaration
  name: (type_identifier) @class.name
  heritage: .
    (heritage_clause
      (type_identifier) @base.class)
  body: (class_body) @class.body) @definition.class.with_inheritance

; 类成员查询 - 提供更多上下文信息
(class_declaration
  name: (type_identifier) @class.name
  body: (class_body
    (property_definition
      name: (property_identifier) @property.name
      type: (type_annotation)? @property.type)*
    (method_definition
      name: (property_identifier) @method.name
      parameters: (formal_parameters)? @method.params
      return_type: (type_annotation)? @method.return.type)*)) @definition.class.with_members
`;