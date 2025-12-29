/*
Java Class-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; 统一的类声明查询 - 使用交替模式
(class_declaration
  name: (identifier) @class.name) @definition.class

; 继承关系查询 - 使用锚点和谓词过滤
(class_declaration
  name: (identifier) @derived.class
  superclass: (superclass
    (type_identifier) @base.class)
  (#not-eq? @derived.class @base.class)) @inheritance.relationship

; 接口实现关系查询 - 使用量词操作符
(class_declaration
  name: (identifier) @implementing.class
  super_interfaces: (super_interfaces
    (type_list
      (type_identifier) @implemented.interface)+)) @interface.implementation

; 泛型类型参数查询 - 使用量词操作符
(class_declaration
  name: (identifier) @generic.class
  type_parameters: (type_parameters
    (type_parameter
      name: (identifier) @type.param)+)) @generic.type.parameter

; 方法声明查询 - 使用量词操作符
(class_declaration
  body: (class_body
    (method_declaration
      name: (identifier) @method.name
      parameters: (formal_parameters
        (formal_parameter
          name: (identifier) @method.param)*)*))) @class.method

; 构造函数声明查询
(class_declaration
  body: (class_body
    (constructor_declaration
      name: (identifier) @constructor.name
      parameters: (formal_parameters
        (formal_parameter
          name: (identifier) @constructor.param)*)*))) @class.constructor

; 静态成员查询 - 使用谓词过滤
(class_declaration
  body: (class_body
    [
      (field_declaration
        (modifiers
          (modifier) @static.modifier)
        declarator: (variable_declarator
          name: (identifier) @static.field))
      (method_declaration
        (modifiers
          (modifier) @static.modifier)
        name: (identifier) @static.method))
    ]
    (#match? @static.modifier "static"))) @class.static.member

; 注解成员查询 - 使用谓词过滤
(class_declaration
  body: (class_body
    [
      (field_declaration
        (modifiers
          (annotation) @field.annotation)
        declarator: (variable_declarator
          name: (identifier) @annotated.field))
      (method_declaration
        (modifiers
          (annotation) @method.annotation)
        name: (identifier) @annotated.method))
    ])) @class.annotated.member

; 内部类查询 - 使用锚点确保精确匹配
(class_declaration
  body: (class_body
    .
    (class_declaration
      name: (identifier) @inner.class)))) @class.inner.type

; 类型体查询
(class_body) @body.class

; 修饰符查询
(modifiers
  (modifier) @modifier.name) @definition.modifiers
`;