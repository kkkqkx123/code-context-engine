/*
C++ Function Annotation Tree-Sitter Query Patterns
Shared queries for function annotations used across entities and relationships
*/
export default `
; 函数注解 - 属性说明符在函数定义中
(function_definition
  (attribute_declaration)
  declarator: (function_declarator
    declarator: (identifier) @function.name)
  body: (compound_statement) @function.body) @function.definition.with.annotation

; 虚函数和重写注解
(function_definition
  (virtual_specifier) @virtual.specifier
 declarator: (function_declarator
    declarator: (field_identifier) @virtual.function)
  (#match? @virtual.specifier "override|final")) @function.virtual.annotation

; constexpr/static/inline 注解
(function_definition
  (storage_class_specifier) @storage.specifier
  declarator: (function_declarator
    declarator: (identifier) @storage.function)
  body: (compound_statement) @storage.body
  (#match? @storage.specifier "constexpr|consteval|static|inline")) @function.storage.annotation

; 模板函数注解
(template_declaration
  parameters: (template_parameter_list)
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @template.function.name))) @function.template.annotation
`;