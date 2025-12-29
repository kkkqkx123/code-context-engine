/*
Python Function Annotation Tree-Sitter Query Patterns
Shared queries for function annotations used across entities and relationships
*/
export default `
; 装饰器函数定义
(decorated_definition
  (decorator
    (identifier) @decorator.name)
  definition: (function_definition
    name: (identifier) @function.name)) @function.definition.with.decorator

; 带参数的装饰器函数定义
(decorated_definition
  (decorator
    (call
      function: (identifier) @decorator.function
      arguments: (argument_list
        (_) @decorator.argument)*))
  definition: (function_definition
    name: (identifier) @function.name)) @function.definition.with.parameterized.decorator

; 异步函数定义
(function_definition
  "async"
  name: (identifier) @async.function.name) @async.function.definition

; 带类型注解的函数定义
(function_definition
  name: (identifier) @typed.function.name
  return_type: (type) @return.type) @function.definition.with.type.annotation

; 异步带类型注解的函数定义
(function_definition
  "async"
  name: (identifier) @typed.async.function.name
  return_type: (type) @return.type) @async.function.definition.with.type.annotation

; 生成器函数定义
(function_definition
  name: (identifier) @generator.name
  body: (block
    (expression_statement
      (yield) .))) @generator.function.definition

; 异步生成器函数定义
(function_definition
  "async"
  name: (identifier) @async.generator.name
  body: (block
    (expression_statement
      (yield) .))) @async.generator.function.definition

; 特殊方法定义（魔术方法）
(function_definition
  name: (identifier) @special.method.name
  (#match? @special.method.name "^__(.*)__$")) @special.method.definition

; 私有方法定义
(function_definition
  name: (identifier) @private.method.name
  (#match? @private.method.name "^_")) @private.method.definition

; 测试函数定义
(function_definition
  name: (identifier) @test.function.name
  (#match? @test.function.name "^(test_|.*_test)$")) @test.function.definition

; 抽象方法定义
(decorated_definition
  (decorator
    (identifier) @abstractmethod.decorator
    (#match? @abstractmethod.decorator "^(abstractmethod)$"))
  definition: (function_definition
    name: (identifier) @abstract.method.name)) @abstract.method.definition

; 属性方法定义
(decorated_definition
  (decorator
    (identifier) @property.decorator
    (#match? @property.decorator "^(property|setter|getter)$"))
  definition: (function_definition
    name: (identifier) @property.method.name)) @property.method.definition

; 静态方法定义
(decorated_definition
  (decorator
    (identifier) @staticmethod.decorator
    (#match? @staticmethod.decorator "^(staticmethod)$"))
  definition: (function_definition
    name: (identifier) @staticmethod.method.name)) @staticmethod.definition

; 类方法定义
(decorated_definition
  (decorator
    (identifier) @classmethod.decorator
    (#match? @classmethod.decorator "^(classmethod)$"))
  definition: (function_definition
    name: (identifier) @classmethod.method.name)) @classmethod.definition
`;