/*
C++ Function-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
Optimized based on tree-sitter best practices
*/
export default `
; 统一的函数查询 - 使用交替模式合并重复查询
[
  (function_definition
    type: (_)
    declarator: (function_declarator
      declarator: (identifier) @function.name)
    body: (compound_statement) @function.body) @definition.function
  (declaration
    type: (_)
    declarator: (function_declarator
      declarator: (identifier) @function.name
      parameters: (parameter_list))) @definition.function.prototype
] @definition.function

; 模板函数查询 - 使用谓词过滤
(template_declaration
  parameters: (template_parameter_list)
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @template.function.name)
    body: (compound_statement) @template.function.body)) @definition.template.function

; 运算符重载查询 - 使用交替模式
[
  (function_definition
    declarator: (function_declarator
      declarator: (operator_name) @operator.name)) @definition.operator.overload
  (function_definition
    declarator: (function_declarator
      declarator: (operator_name) @operator.new.name))
  (#match? @operator.new.name "^(new|delete)$") @definition.operator.new.delete
] @definition.operator

; 特殊函数修饰符查询 - 使用谓词过滤
[
  (function_definition
    (storage_class_specifier) @constexpr.specifier
    declarator: (function_declarator
      declarator: (identifier) @constexpr.function)
    (#match? @constexpr.specifier "^(constexpr|consteval)$")) @definition.constexpr.function
  (function_definition
    (explicit_specifier) @explicit.specifier
    declarator: (function_declarator
      declarator: (identifier) @explicit.function)) @definition.explicit.function
  (function_definition
    (virtual_specifier) @virtual.specifier
    declarator: (function_declarator
      declarator: (field_identifier) @virtual.method)) @definition.virtual.method
] @definition.special.function



; 虚函数重写查询 - 使用谓词过滤
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @override.method)
  (virtual_specifier) @virtual.specifier
  (override_specifier) @override.specifier) @definition.virtual.override
`;