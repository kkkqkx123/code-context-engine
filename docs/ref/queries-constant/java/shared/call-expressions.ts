/*
Java Call Expression Tree-Sitter Query Patterns
Shared queries for call expressions used across entities and relationships
*/
export default `
; 方法调用查询 - 使用参数化模式
(method_invocation
  name: [
    (identifier) @method.name
    (field_access
      field: (identifier) @method.field)
  ]
  arguments: (argument_list
    (identifier) @method.arg)*
  type_arguments: (type_arguments
    (type_identifier) @type.arg)*) @definition.method.call

; 对象创建查询 - 使用参数化模式
(object_creation_expression
  type: (type_identifier) @created.type
  arguments: (argument_list
    (identifier) @constructor.arg)*
  type_arguments: (type_arguments
    (type_identifier) @type.arg)*) @definition.object.creation

; 静态方法调用数据流 - 使用谓词过滤
(method_invocation
  function: (scoped_identifier
    scope: (identifier) @target.class
    name: (identifier) @target.method)
  arguments: (argument_list
    (identifier) @source.parameter)+) @definition.static.method.call

; 泛型方法调用数据流 - 使用量词操作符
(method_invocation
  name: (identifier) @target.method
  type_arguments: (type_arguments
    (type_identifier) @type.arg)+
  arguments: (argument_list
    (identifier) @source.parameter)+) @definition.generic.method.call

; 对象方法调用数据流 - 使用锚点确保精确匹配
(method_invocation
  function: (field_access
    object: (identifier) @target.object
    field: (identifier) @target.method)
  arguments: (argument_list
    (identifier) @source.parameter)+) @definition.object.method.call

; 链式方法调用查询
(method_invocation
  function: (field_access
    object: (method_invocation
      function: (field_access
        object: (identifier) @chained.object
        field: (identifier) @chained.method))
    field: (identifier) @chained.next.method)
  arguments: (argument_list
    (identifier) @chained.arg)*) @definition.chained.method.call

; 集合操作调用查询 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @target.collection
    field: (identifier) @collection.method)
  arguments: (argument_list
    (identifier) @collection.arg)*)
  (#match? @collection.method "^(add|put|set|remove|get|contains|addAll)$")) @definition.collection.method.call

; 字符串操作调用查询 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @target.string
    field: (identifier) @string.method)
  arguments: (argument_list
    (identifier) @string.arg)*)
  (#match? @string.method "^(concat|substring|replace|format|valueOf)$")) @definition.string.method.call

; 流操作调用查询 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @target.stream
    field: (identifier) @stream.method)
  arguments: (argument_list
    (identifier) @stream.arg)*)
  (#match? @stream.method "^(map|filter|flatMap|collect|reduce|forEach)$")) @definition.stream.method.call

; 反射调用查询 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @reflection.object
    field: (identifier) @reflection.method)
  arguments: (argument_list
    (identifier) @reflection.arg)*)
  (#match? @reflection.method "^(getMethod|invoke|getField|setField)$")) @definition.reflection.method.call
`;