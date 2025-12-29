/*
C# Function Annotation Tree-Sitter Query Patterns
Shared queries for function annotations used across entities and relationships
*/
export default `
; 方法注解 - 属性在方法定义中
(method_declaration
  attribute: (attribute_list)
  name: (identifier) @method.name
  body: (block) @method.body) @method.definition.with.annotation

; 虚方法注解
(method_declaration
  (modifier) @virtual.modifier
  name: (identifier) @virtual.method
  (#match? @virtual.modifier "virtual")) @method.virtual.annotation

; 重写方法注解
(method_declaration
  (modifier) @override.modifier
 name: (identifier) @override.method
  (#match? @override.modifier "override")) @method.override.annotation

; 访问修饰符注解
(method_declaration
  (modifier) @access.modifier
  name: (identifier) @access.method
  (#match? @access.modifier "public|private|protected|internal")) @method.access.annotation

; 特殊修饰符注解
(method_declaration
  (modifier) @special.modifier
 name: (identifier) @special.method
  (#match? @special.modifier "static|async|extern|unsafe")) @method.special.annotation

; 泛型方法注解
(method_declaration
  type_parameters: (type_parameter_list)
  name: (identifier) @generic.method.name) @method.generic.annotation
`;