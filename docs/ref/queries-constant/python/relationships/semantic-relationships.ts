/*
Python Semantic Relationships-specific Tree-Sitter Query Patterns
用于识别方法重写、重载、委托、观察者模式等语义关系
*/
export default `
; Class inheritance relationships with anchor operator for precise matching
(class_definition
  name: (identifier) @derived.class
  superclasses: .
    (argument_list
      (identifier) @base.class)
  body: (block
    (function_definition
      name: (identifier) @overridden.method))) @semantic.relationship.class.inheritance

; Protocol patterns using alternation for similar patterns
[
  (class_definition
    body: (block
      (function_definition
        name: (identifier) @descriptor.method
        (#match? @descriptor.method "^(__get__|__set__|__delete__)$"))))) @semantic.relationship.descriptor.protocol
  (class_definition
    body: (block
      (function_definition
        name: (identifier) @iterator.method
        (#match? @iterator.method "^__(a)?(iter|next)__$"))))) @semantic.relationship.iterator.protocol
  (class_definition
    name: (identifier) @metaclass.class
    (argument_list
      (keyword_argument
        key: (identifier) @metakey
        value: (identifier) @metavalue
        (#match? @metakey "metaclass"))))) @semantic.relationship.metaclass.pattern
] @semantic.protocol.pattern
`;