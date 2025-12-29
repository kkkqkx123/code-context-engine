/*
Rust Semantic Relationships-specific Tree-Sitter Query Patterns
用于识别方法重写、重载、委托、观察者模式等语义关系
*/
export default `
; Trait实现关系 - 使用锚点确保精确匹配
(impl_item
  trait: (type_identifier) @implemented.trait
  type: (type_identifier) @implementing.type
  body: (declaration_list
    (function_item
      name: (identifier) @implemented.method)+)) @semantic.trait.implementation

; 泛型约束关系 - 使用量词操作符
(impl_item
  type_parameters: (type_parameters
    (type_parameter
      name: (type_identifier) @generic.param)+)
  (where_clause
    (where_predicate
      left: (type_identifier) @constrained.type
      bounds: (trait_bounds
        (type_identifier) @constraint.trait)+)+)) @semantic.generic.constraint

; Deref继承关系 - 使用谓词过滤
(impl_item
  trait: (type_identifier) @deref.trait
  type: (type_identifier) @deref.type
  body: (declaration_list
    (function_item
      name: (identifier) @deref.method))
  (#match? @deref.trait "^Deref$")) @semantic.deref.inheritance

; 观察者模式关系 - 使用谓词过滤
(function_item
  name: (identifier) @observer.method
  parameters: (parameters
    (parameter
      name: (identifier) @event.parameter)+)
  (#match? @observer.method "^(on_|watch_|observe_|subscribe_|notify_|emit_|publish_|broadcast_)$")) @semantic.observer.pattern

; 发布订阅模式关系 - 使用谓词过滤
(function_item
  name: (identifier) @publisher.method
  parameters: (parameters
    (parameter
      name: (identifier) @event.parameter)+)
  (#match? @publisher.method "^(emit_|publish_|notify_|broadcast_|fire_|trigger_)$")) @semantic.publisher.pattern

; 工厂模式关系 - 使用谓词过滤
(function_item
  name: (identifier) @factory.method
  return_type: (type_identifier) @created.type
  (#match? @factory.method "^(new|create|build|make|from|parse|of|value_of)$")) @semantic.factory.pattern

; 建造者模式关系 - 使用谓词过滤
(struct_item
  name: (type_identifier) @builder.struct
  body: (field_declaration_list
    (field_declaration
      name: (field_identifier) @builder.field)+)) @semantic.builder.pattern

; 单例模式关系 - 使用谓词过滤
(const_item
  name: (identifier) @singleton.instance
  type: (type_identifier) @singleton.type
  value: (call_expression
    function: (identifier) @constructor.call)) @semantic.singleton.pattern

; 策略模式关系 - 使用谓词过滤
(trait_item
  name: (type_identifier) @strategy.trait
  body: (declaration_list
    (function_item
      name: (identifier) @strategy.method)+)) @semantic.strategy.pattern

; 命令模式关系 - 使用谓词过滤
(struct_item
  name: (type_identifier) @command.struct
  body: (field_declaration_list
    (field_declaration
      name: (field_identifier) @command.field)+)) @semantic.command.pattern

; 适配器模式关系 - 使用谓词过滤
(impl_item
  trait: (type_identifier) @adapter.trait
  type: (type_identifier) @adapter.type
  body: (declaration_list
    (function_item
      name: (identifier) @adapter.method)+)) @semantic.adapter.pattern

; 装饰器模式关系 - 使用谓词过滤
(attribute_item
  attribute: (attribute
    name: (identifier) @decorator.macro
    (#match? @decorator.macro "^(derive|attribute)$")
    arguments: (token_tree
      (identifier) @decorated.trait)+)) @semantic.decorator.pattern

; 代理模式关系 - 使用谓词过滤
(struct_item
  name: (type_identifier) @proxy.struct
  body: (field_declaration_list
    (field_declaration
      name: (field_identifier) @proxy.target)+)) @semantic.proxy.pattern

; 组合模式关系 - 使用谓词过滤
(enum_item
  name: (type_identifier) @composite.enum
  body: (enum_variant_list
    (enum_variant
      name: (identifier) @composite.variant
      (field_declaration_list
        (field_declaration
          name: (field_identifier) @composite.field)+)*)) @semantic.composite.pattern

; 模板方法模式关系 - 使用谓词过滤
(trait_item
  name: (type_identifier) @template.trait
  body: (declaration_list
    (function_item
      name: (identifier) @template.method
      body: (block
        (call_expression
          function: (field_expression
            value: (self)
            field: (field_identifier) @abstract.method)))))) @semantic.template.pattern

; 迭代器模式关系 - 使用谓词过滤
(impl_item
  trait: (type_identifier) @iterator.trait
  type: (type_identifier) @iterator.type
  (#match? @iterator.trait "^(Iterator|IntoIterator|ExactSizeIterator|DoubleEndedIterator)$")) @semantic.iterator.pattern

; 访问者模式关系 - 使用谓词过滤
(trait_item
  name: (type_identifier) @visitor.trait
  body: (declaration_list
    (function_item
      name: (identifier) @visitor.method
      parameters: (parameters
        (parameter
          name: (identifier) @visited.type)+)+)) @semantic.visitor.pattern

; 状态模式关系 - 使用谓词过滤
(enum_item
  name: (type_identifier) @state.enum
  body: (enum_variant_list
    (enum_variant
      name: (identifier) @state.variant)+)) @semantic.state.pattern

; 责任链模式关系 - 使用谓词过滤
(function_item
  name: (identifier) @chain.method
  parameters: (parameters
    (parameter
      name: (identifier) @chain.handler)+)
  return_type: (type_identifier) @chain.result) @semantic.chain.pattern
`;