/*
Python Function-specific Tree-Sitter Query Patterns
Extracted from the main python.ts file for better maintainability and performance
*/
export default `
; Basic function definitions
(function_definition
  name: (identifier) @function.name) @definition.function

; Lambda expressions with anchor for precise matching
(lambda
  parameters: (parameters)? @lambda.params
  body: (_) @lambda.body) @definition.lambda

; Method definitions within classes
(class_definition
  body: (block
    (function_definition
      name: (identifier) @method.name))) @definition.method
`;