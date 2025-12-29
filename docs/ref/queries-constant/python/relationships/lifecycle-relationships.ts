/*
Python Lifecycle Relationships-specific Tree-Sitter Query Patterns
用于识别对象实例化、初始化、销毁、生命周期管理等关系
*/
export default `
; Constructor and destructor definitions using alternation
[
  (class_definition
    name: (identifier) @constructed.class
    body: (block
      (function_definition
        name: (identifier) @constructor.method
        (#match? @constructor.method "^__init__$"))))
  (class_definition
    name: (identifier) @destructed.class
    body: (block
      (function_definition
        name: (identifier) @destructor.method
        (#match? @destructor.method "^__del__$"))))
] @lifecycle.relationship.constructor_or_destructor

; Context manager protocol methods using alternation for similar patterns
[
  (class_definition
    name: (identifier) @context.manager.class
    body: (block
      (function_definition
        name: (identifier) @entry.method
        (#match? @entry.method "^__(a)?enter__$"))))
  (class_definition
    name: (identifier) @context.manager.class
    body: (block
      (function_definition
        name: (identifier) @exit.method
        (#match? @exit.method "^__(a)?exit__$"))))
] @lifecycle.relationship.context_manager

; Iterator protocol methods using alternation
[
  (class_definition
    name: (identifier) @iterator.class
    body: (block
      (function_definition
        name: (identifier) @iterator.method
        (#match? @iterator.method "^__(a)?(iter|next)__$"))))
] @lifecycle.relationship.iterator
`;