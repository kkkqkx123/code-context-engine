/*
C# Expression and Type-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Using directives - important for namespace imports
(using_directive) @definition.using_directive

; Using statements - important for resource management
(using_statement) @definition.using_statement

; Type aliases - important for external aliases
(extern_alias_directive) @definition.extern_alias

; Type parameter constraints - important for generic constraints
(type_parameter_constraints_clause) @definition.type_constraint

; Conditional expressions - important for ternary operations
(conditional_expression) @definition.conditional

; Conditional access expressions - important for null-safe access
(conditional_access_expression) @definition.conditional_access

; Member binding expressions - important for deconstruction
(member_binding_expression) @definition.member_binding

; Element binding expressions - important for deconstruction
(element_binding_expression) @definition.element_binding

; Assignment and operators - important for operations
(assignment_expression) @definition.assignment
(binary_expression) @definition.binary_operation
(unary_expression) @definition.unary_operation
(update_expression) @definition.update_operation

; Type expressions - important for type operations
(cast_expression) @definition.cast
(as_expression) @definition.as_expression

; Null handling expressions - important for null safety
(null_coalescing_expression) @definition.null_coalescing

; Exception expressions - important for exception handling
(throw_expression) @definition.throw_expression

; Async expressions - important for asynchronous programming
(await_expression) @definition.await_expression

; Type information expressions - important for reflection
(sizeof_expression) @definition.sizeof_expression
(typeof_expression) @definition.typeof_expression
(nameof_expression) @definition.nameof_expression

; Default expressions - important for default values
(default_expression) @definition.default_expression

; String expressions - important for string manipulation
(interpolated_string_expression) @definition.interpolated_string
(interpolated_verbatim_string_expression) @definition.interpolated_verbatim_string
(interpolated_string_text) @definition.interpolated_text
(interpolation) @definition.interpolation

; Object creation expressions - important for instantiation
(with_expression) @definition.with_expression
(member_access_expression) @definition.member_access
(element_access_expression) @definition.element_access
(object_creation_expression) @definition.object_creation
(anonymous_object_creation_expression) @definition.anonymous_object_creation

; Array expressions - important for collections
(array_creation_expression) @definition.array_creation
(implicit_array_creation_expression) @definition.implicit_array_creation
(stack_alloc_array_creation_expression) @definition.stackalloc_array_creation

; Type expressions - important for type system
(array_type) @definition.array_type
(pointer_type) @definition.pointer_type
(nullable_type) @definition.nullable_type
(tuple_type) @definition.tuple_type
(tuple_element) @definition.tuple_element
(tuple_expression) @definition.tuple

; Basic expressions - important for fundamental operations
(parenthesized_expression) @definition.parenthesized
(this_expression) @definition.this
(base_expression) @definition.base

; Literals - important for constant values
(string_literal) @definition.string_literal
(character_literal) @definition.character_literal
(numeric_literal) @definition.numeric_literal
(boolean_literal) @definition.boolean_literal
(null_literal) @definition.null_literal
`;