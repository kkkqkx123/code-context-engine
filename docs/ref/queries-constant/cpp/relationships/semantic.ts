/*
C++ Semantic Relationships-specific Tree-Sitter Query Patterns
用于识别方法重写、重载、委托、观察者模式等语义关系
Optimized based on tree-sitter best practices
*/
export default `
; 函数重载查询 - 使用量词操作符
(class_specifier
  name: (type_identifier) @class.name
  body: (field_declaration_list
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @overloaded.method)
      parameters: (parameter_list) @method.params)+)) @semantic.function.overload
; 模板特化查询 - 简化模式
(explicit_specialization
  (template_declaration
    (function_definition
      declarator: (function_declarator
        declarator: (identifier) @specialized.function)))) @semantic.template.specialization

; 友元关系查询 - 使用交替模式
[
  (friend_declaration
    (function_definition
      declarator: (function_declarator
        declarator: (identifier) @friend.function)))
  (friend_declaration
    (class_specifier
      name: (type_identifier) @friend.class))
] @semantic.friend.relationship

; 参数化的设计模式查询 - 减少重复
(class_specifier
  name: (type_identifier) @pattern.class
  body: (field_declaration_list
    (field_declaration
      declarator: (field_declarator
        declarator: (field_identifier) @pattern.field)
      type: (type_identifier) @pattern.type)*
    (function_definition
      declarator: (function_declarator
        declarator: (field_identifier) @pattern.method))*)) @semantic.design.pattern
  (#match? @pattern.class "^(Observer|Subject|Strategy|Factory|Singleton)$")

; 运算符重载关系 - 使用谓词过滤
(function_definition
  declarator: (function_declarator
    declarator: (operator_name) @operator.name)
  parameters: (parameter_list) @operator.params) @semantic.operator.overload
  (#match? @operator.name "^(operator\\+|operator-|operator\\*|operator/|operator==|operator!=)$")

; 纯虚函数关系 - 使用锚点确保精确匹配
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @pure.virtual.method)
  (virtual_specifier) @virtual.specifier
  body: (pure_virtual_clause)) @semantic.pure.virtual

; 模板参数关系 - 使用量词操作符
(template_declaration
  parameters: (template_parameter_list
    (type_parameter
      name: (type_identifier) @template.param)*)) @semantic.template.parameter

; 概念约束关系（C++20） - 使用锚点确保精确匹配
(concept_definition
  name: (identifier) @constraint.concept
  parameters: (template_parameter_list)
  (requires_clause) @constraint.requires) @semantic.constraint.concept

; CRTP模式查询 - 使用锚点和谓词
(class_specifier
  name: (type_identifier) @crtp.derived.class
  base_class_clause: (base_class_clause
    (template_type
      (type_identifier) @crtp.base.class
      template_arguments: (template_argument_list
        (type_identifier) @crtp.derived.type)))) @semantic.crtp.pattern

; RAII模式查询 - 使用锚点和量词操作符
(class_specifier
  name: (type_identifier) @raii.class
  body: (field_declaration_list
    (field_declaration
      type: (type_identifier) @resource.type
      declarator: (field_declarator
        declarator: (field_identifier) @resource.field))
    (function_definition
      declarator: (function_declarator
        declarator: (identifier) @constructor.method))
    (function_definition
      declarator: (function_declarator
        declarator: (destructor_name) @destructor.method)))) @semantic.raii.pattern

; 委托模式查询 - 简化模式
(class_specifier
  name: (type_identifier) @delegator.class
  body: (field_declaration_list
    (field_declaration
      type: (type_identifier) @delegate.type
      declarator: (field_declarator
        declarator: (field_identifier) @delegate.field)))) @semantic.delegation.pattern

; 异常安全模式查询 - 使用谓词过滤
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @exception.safe.function)
  body: (compound_statement
    (try_statement
      body: (compound_statement) @try.body
      (catch_clause
        parameter: (parameter_declaration
          declarator: (identifier) @catch.param)
        body: (compound_statement) @catch.body)))) @semantic.exception.safety
`;