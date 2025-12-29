/*
C++ and C# Shared Inheritance Relationships Tree-Sitter Query Patterns
用于识别和分析代码中的继承关系
包含C++特有的类继承、虚函数、纯虚函数、多重继承、模板继承等
包含C#特有的类继承、接口实现、泛型继承、显式接口实现等
*/
export default `
; ===== C++ 特定继承关系 =====

; 类继承关系 - 基本模式
(class_specifier
  name: (type_identifier) @inheritance.derived.class
  base_class_clause: (base_class_clause
    (type_identifier) @inheritance.base.class)) @inheritance.relationship.class.inheritance

; 多重继承关系
(class_specifier
  name: (type_identifier) @inheritance.derived.class
  base_class_clause: (base_class_clause
    (type_identifier) @inheritance.base.class.1
    (type_identifier) @inheritance.base.class.2
    (type_identifier) @inheritance.base.class.3)*)) @inheritance.relationship.multiple.inheritance

; 虚函数重写关系
(class_specifier
  name: (type_identifier) @inheritance.derived.class
  body: (field_declaration_list
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @inheritance.override.method)
      (virtual_specifier) @inheritance.virtual.specifier))) @inheritance.relationship.virtual.override

; 纯虚函数定义
(class_specifier
  name: (type_identifier) @inheritance.abstract.class
  body: (field_declaration_list
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @inheritance.pure.virtual.method)
      (virtual_specifier) @inheritance.virtual.specifier
      body: (pure_virtual_clause)))) @inheritance.relationship.pure.virtual

; 虚继承关系
(class_specifier
  name: (type_identifier) @inheritance.derived.class
  base_class_clause: (base_class_clause
    (virtual_specifier) @inheritance.virtual.inheritance.specifier
    (type_identifier) @inheritance.virtual.base.class))) @inheritance.relationship.virtual.inheritance

; 模板继承关系
(class_specifier
  name: (type_identifier) @inheritance.template.derived.class
  base_class_clause: (base_class_clause
    (template_type
      name: (type_identifier) @inheritance.template.base.class
      arguments: (template_argument_list
        (type_identifier) @inheritance.template.argument)*))) @inheritance.relationship.template.inheritance

; CRTP (Curiously Recurring Template Pattern) 继承
(class_specifier
  name: (type_identifier) @inheritance.crtp.derived.class
  base_class_clause: (base_class_clause
    (template_type
      name: (type_identifier) @inheritance.crtp.base.class
      template_arguments: (template_argument_list
        (type_identifier) @inheritance.crtp.derived.type)))) @inheritance.relationship.crtp.pattern

; 结构体继承关系
(struct_specifier
  name: (type_identifier) @inheritance.derived.struct
  base_class_clause: (base_class_clause
    (type_identifier) @inheritance.base.struct)) @inheritance.relationship.struct.inheritance

; 友元类继承关系
(friend_declaration
  (class_specifier
    name: (type_identifier) @inheritance.friend.class)) @inheritance.relationship.friend.class

; ===== 共享继承关系 =====
; 方法重写关系 - 使用谓词过滤
[
  (function_definition
    declarator: (function_declarator
      declarator: (field_identifier) @inheritance.override.method)
    (virtual_specifier) @inheritance.virtual.specifier)
] @inheritance.relationship.method.override

; 虚方法定义
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @inheritance.virtual.method)
  (virtual_specifier) @inheritance.virtual.specifier) @inheritance.relationship.virtual.method

; 构造函数链式调用
[
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @inheritance.constructor.method)
    body: (compound_statement
      (constructor_initializer_list
        (constructor_initializer
          name: (identifier) @inheritance.base.constructor))))
] @inheritance.relationship.constructor.chaining

`;