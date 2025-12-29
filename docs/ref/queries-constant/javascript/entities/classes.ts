/*
JavaScript Class-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的类声明查询 - 使用交替模式合并重复查询
(class_declaration
  name: (identifier) @class.name) @definition.class

; Constructor - important class entry point
(method_definition
  name: (property_identifier) @constructor.name
  (#eq? @constructor.name "constructor")) @definition.constructor

; Static initialization blocks - important for class setup
(class_static_block) @definition.static_block

; 带继承的类查询 - 使用锚点操作符
(class_declaration
  name: (identifier) @class.name
  heritage: .
    (class_heritage
      (identifier) @base.class)
  body: (class_body) @class.body) @definition.class.with_inheritance

; 类成员查询 - 提供更多上下文信息
(class_declaration
  name: (identifier) @class.name
  body: (class_body
    (property_definition
      name: (property_identifier) @property.name)*
    (method_definition
      name: (property_identifier) @method.name
      parameters: (formal_parameters)? @method.params
      body: (statement_block)? @method.body)*)) @definition.class.with_members
`;