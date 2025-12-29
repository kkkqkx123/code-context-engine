/*
C# Class and Struct-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
Optimized based on tree-sitter best practices
*/
export default `
; 命名空间声明查询 - 使用交替模式
[
  (namespace_declaration
    name: (qualified_name) @namespace.name) @definition.namespace
  (namespace_declaration
    name: (identifier) @namespace.name) @definition.namespace
  (file_scoped_namespace_declaration
    name: (qualified_name) @namespace.name) @definition.namespace
  (file_scoped_namespace_declaration
    name: (identifier) @namespace.name) @definition.namespace
] @definition.namespace

; 类型声明查询 - 使用交替模式
[
  (class_declaration
    name: (identifier) @class.name
    base_class_clause: (base_class_clause
      (identifier) @base.class)?
    body: (declaration_list) @class.body) @definition.class
  (struct_declaration
    name: (identifier) @struct.name
    base_class_clause: (base_class_clause
      (identifier) @base.interface)?
    body: (declaration_list) @struct.body) @definition.struct
  (interface_declaration
    name: (identifier) @interface.name
    base_class_clause: (base_class_clause
      (identifier) @base.interface)?
    body: (declaration_list) @interface.body) @definition.interface
] @definition.type

; 记录类型查询 - 使用交替模式
[
  (record_class_declaration
    name: (identifier) @record.class.name
    body: (declaration_list) @record.class.body) @definition.record.class
  (record_struct_declaration
    name: (identifier) @record.struct.name
    body: (declaration_list) @record.struct.body) @definition.record.struct
] @definition.record

; 枚举声明查询 - 简化模式
(enum_declaration
  name: (identifier) @enum.name
  body: (enum_member_declaration_list
    (enum_member_declaration
      name: (identifier) @enum.member)*)) @definition.enum

; 构造函数和析构函数查询 - 使用交替模式
[
  (constructor_declaration
    name: (identifier) @constructor.name
    parameters: (parameter_list) @constructor.params
    body: (block) @constructor.body) @definition.constructor
  (destructor_declaration
    name: (identifier) @destructor.name
    body: (block) @destructor.body) @definition.destructor
] @definition.constructor_or_destructor

; 字段声明查询 - 使用锚点和量词操作符
(field_declaration
  type: (identifier) @field.type
  declarators: (variable_declarator_list
    (variable_declarator
      name: (identifier) @field.name))*) @definition.field

; 事件声明查询 - 使用交替模式
[
  (event_field_declaration
    type: (identifier) @event.type
    declarators: (variable_declarator_list
      (variable_declarator
        name: (identifier) @event.name))) @definition.event.field
  (event_declaration
    name: (identifier) @event.name
    type: (identifier) @event.type
    accessors: (accessor_list) @event.accessors) @definition.event.declaration
] @definition.event

; 特性声明查询 - 使用交替模式
[
  (attribute
    name: (identifier) @attribute.name
    arguments: (attribute_argument_list
      (attribute_argument) @attribute.argument)*) @definition.attribute
  (attribute_list
    (attribute) @attribute.list*) @definition.attribute.list
] @definition.attribute

; 泛型类型参数查询 - 使用量词操作符
(type_parameter_list
  (type_parameter
    name: (identifier) @type.parameter)*) @definition.type.parameter

; 嵌套类型查询 - 使用锚点确保精确匹配
(class_declaration
  name: (identifier) @nested.class.name
  body: (declaration_list
    (class_declaration
      name: (identifier) @inner.class.name))) @definition.nested.class

; 静态类查询 - 使用谓词过滤
(class_declaration
  (modifier) @static.modifier
  name: (identifier) @static.class.name
  (#match? @static.modifier "static")) @definition.static.class

; 抽象类和接口查询 - 使用谓词过滤
[
  (class_declaration
    (modifier) @abstract.modifier
    name: (identifier) @abstract.class.name
    (#match? @abstract.modifier "abstract")) @definition.abstract.class
  (interface_declaration
    name: (identifier) @interface.name) @definition.interface
] @definition.abstract.type

; 分部类查询 - 使用谓词过滤
(class_declaration
  (modifier) @partial.modifier
  name: (identifier) @partial.class.name
  (#match? @partial.modifier "partial")) @definition.partial.class
`;