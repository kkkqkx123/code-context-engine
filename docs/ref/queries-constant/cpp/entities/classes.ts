/*
C++ Class and Struct-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
Optimized based on tree-sitter best practices
*/
export default `
; 统一的类型声明查询 - 使用交替模式
[
  (class_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list) @type.body) @definition.class
  (struct_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list) @type.body) @definition.struct
  (union_specifier
    name: (type_identifier) @type.name
    body: (field_declaration_list) @type.body) @definition.union
] @definition.type

; 模板类型查询 - 使用锚点和谓词
(template_declaration
  parameters: (template_parameter_list)
  [
    (class_specifier
      name: (type_identifier) @template.class.name
      body: (field_declaration_list) @template.class.body)
    (struct_specifier
      name: (type_identifier) @template.struct.name
      body: (field_declaration_list) @template.struct.body)
  ]) @definition.template.type

; 带继承的类查询 - 使用锚点和谓词
(class_specifier
  name: (type_identifier) @class.name
  (base_class_clause
    (type_identifier) @base.class)
  body: (field_declaration_list) @class.body) @definition.class.with_inheritance


; 友元声明查询 - 使用交替模式
[
  (friend_declaration
    (declaration
      declarator: (function_declarator
        declarator: (identifier) @friend.function))) @definition.friend.function
  (friend_declaration
    (type_identifier) @friend.class) @definition.friend.class
] @definition.friend

; 成员字段查询 - 使用锚点和量词操作符
(field_declaration
  type: (_) @field.type
  declarator: (field_identifier) @field.name)* @definition.field

; 成员方法查询 - 使用锚点和谓词过滤
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @method.name)
  body: (compound_statement) @method.body) @definition.method

; 静态成员查询 - 使用谓词过滤
[
  (field_declaration
    (storage_class_specifier) @static.specifier
    type: (_) @static.field.type
    declarator: (field_identifier) @static.field.name
    (#match? @static.specifier "static")) @definition.static.field
  (function_definition
    (storage_class_specifier) @static.specifier
    declarator: (function_declarator
      declarator: (field_identifier) @static.method.name)
    (#match? @static.specifier "static")) @definition.static.method
] @definition.static.member

; 虚函数查询 - 修正为更准确的匹配
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @virtual.method.name)
  (virtual_specifier) @virtual.specifier) @definition.virtual.method

; 纯虚函数查询 - 修正为更准确的匹配
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @pure.virtual.method.name)
  body: (compound_statement
    (expression_statement
      (call_expression
        function: (identifier) @function.name
        arguments: (argument_list
          (identifier) @argument.name))))
  (#match? @function.name "^0|nullptr$")) @definition.pure.virtual.method

; 模板成员查询 - 使用锚点和谓词
(template_declaration
  parameters: (template_parameter_list)
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @template.method.name))) @definition.template.method
`;