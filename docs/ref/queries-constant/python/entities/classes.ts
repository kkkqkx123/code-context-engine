/*
Python Class-specific Tree-Sitter Query Patterns
*/
export default `
; Basic class definitions
(class_definition
  name: (identifier) @class.name) @definition.class

; Class inheritance with anchor operator for precise matching
(class_definition
  name: (identifier) @class.name
  superclasses: .
    (argument_list
      (identifier) @base.class)) @definition.class.with_inheritance

; Constructor and destructor methods with precise matching
(class_definition
  body: (block
    (function_definition
      name: (identifier) @constructor.name
      (#match? @constructor.name "^__init__$")))) @definition.constructor

(class_definition
  body: (block
    (function_definition
      name: (identifier) @destructor.name
      (#match? @destructor.name "^__del__$")))) @definition.destructor
`;