/*
Rust Lifecycle Relationships-specific Tree-Sitter Query Patterns
用于识别对象实例化、初始化、销毁、生命周期管理等关系
*/
export default `
; 结构体实例化关系 - 使用参数化查询
(struct_expression
  type: (type_identifier) @instantiated.struct
  (field_initializer_list
    (field_initializer
      name: (field_identifier) @field.name
      value: (identifier) @field.value)+)) @lifecycle.struct.instantiation

; 构造函数定义关系 - 使用谓词过滤
(function_item
  name: (identifier) @constructor.method
  parameters: (parameters
    (parameter
      name: (identifier) @constructor.param)+)
  return_type: (type_identifier) @constructed.type
  (#match? @constructor.method "^new$")) @lifecycle.constructor.definition

; 默认构造函数关系 - 使用谓词过滤
(function_item
  name: (identifier) @default.constructor
  return_type: (type_identifier) @default.type
  (#match? @default.constructor "^default$")) @lifecycle.default.constructor

; 复制构造函数关系 - 使用谓词过滤
(function_item
  name: (identifier) @copy.constructor
  parameters: (parameters
    (parameter
      name: (identifier) @source.object)+)
  return_type: (type_identifier) @copied.type
  (#match? @copy.constructor "^clone$")) @lifecycle.copy.constructor

; Drop trait实现关系 - 使用谓词过滤
(impl_item
  trait: (type_identifier) @drop.trait
  type: (type_identifier) @drop.type
  body: (declaration_list
    (function_item
      name: (identifier) @drop.method
      (#match? @drop.method "^drop$")))) @lifecycle.drop.implementation

; 资源获取方法关系 - 使用谓词过滤
(function_item
  name: (identifier) @acquire.method
  return_type: (type_identifier) @resource.type
  (#match? @acquire.method "^(acquire|open|start|connect|lock|borrow)$")) @lifecycle.resource.acquisition

; 资源释放方法关系 - 使用谓词过滤
(function_item
  name: (identifier) @release.method
  (#match? @release.method "^(release|close|stop|disconnect|unlock|drop)$")) @lifecycle.resource.release

; 内存分配关系 - 使用谓词过滤
(call_expression
  function: (identifier) @allocator.function
  arguments: (arguments
    (identifier) @allocation.param)*
  (#match? @allocator.function "^(alloc|Box::new|Vec::new|String::new)$")) @lifecycle.memory.allocation

; 内存释放关系 - 使用谓词过滤
(call_expression
  function: (identifier) @deallocator.function
  arguments: (arguments
    (identifier) @deallocation.target)*
  (#match? @deallocator.function "^(free|drop|clear|reset)$")) @lifecycle.memory.deallocation

; 智能指针创建关系 - 使用谓词过滤
(call_expression
  function: (scoped_identifier
    path: (identifier) @smart_ptr.namespace
    name: (identifier) @smart_ptr.constructor)
  arguments: (arguments
    (identifier) @smart_ptr.param)*
  (#match? @smart_ptr.constructor "^(Box|Rc|Arc|RefCell|Mutex|RwLock)$")) @lifecycle.smart_ptr.creation

; 智能指针销毁关系 - 使用谓词过滤
(call_expression
  function: (field_expression
    value: (identifier) @smart_ptr.object
    field: (field_identifier) @smart_ptr.method)
  (#match? @smart_ptr.method "^(drop|into_raw|into_inner)$")) @lifecycle.smart_ptr.destruction

; 迭代器生命周期 - 使用谓词过滤
(function_item
  name: (identifier) @iterator.method
  return_type: (type_identifier) @iterator.type
  (#match? @iterator.method "^(iter|into_iter|drain)$")) @lifecycle.iterator.creation

; 迭代器消耗关系 - 使用谓词过滤
(call_expression
  function: (field_expression
    value: (identifier) @iterator.object
    field: (field_identifier) @iterator.method)
  (#match? @iterator.method "^(next|collect|fold|for_each)$")) @lifecycle.iterator.consumption

; 异步生命周期 - 使用锚点确保精确匹配
(async_block
  body: (block
    (await_expression
      (call_expression) @async.operation))) @lifecycle.async.creation

; 线程生命周期 - 使用谓词过滤
(call_expression
  function: (scoped_identifier
    path: (identifier) @thread.namespace
    name: (identifier) @thread.method)
  arguments: (arguments
    (closure_expression) @thread.handler)*
  (#match? @thread.method "^(spawn|current|sleep|yield_now)$")) @lifecycle.thread.lifecycle

; 通道生命周期 - 使用谓词过滤
(call_expression
  function: (scoped_identifier
    path: (identifier) @channel.namespace
    name: (identifier) @channel.method)
  arguments: (arguments
    (identifier) @channel.param)*
  (#match? @channel.method "^(channel|Sender|Receiver)$")) @lifecycle.channel.creation

; 生命周期参数关系
(lifetime
  (identifier) @lifetime.parameter) @lifecycle.lifetime.parameter

; 生命周期约束关系 - 使用锚点确保精确匹配
(where_predicate
  left: (lifetime
    (identifier) @constrained.lifetime)
  bounds: (trait_bounds
    (lifetime
      (identifier) @lifetime.bound))) @lifecycle.lifetime.constraint

; 静态生命周期关系 - 使用谓词过滤
(lifetime
  (identifier) @static.lifetime
  (#match? @static.lifetime "^'static$")) @lifecycle.static.lifetime
`;