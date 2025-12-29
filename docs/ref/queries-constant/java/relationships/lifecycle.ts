/*
Java Lifecycle Relationships-specific Tree-Sitter Query Patterns
用于识别对象实例化、初始化、销毁、生命周期管理等关系
*/
export default `
; 对象实例化关系 - 使用参数化查询
(object_creation_expression
  type: (type_identifier) @instantiated.class
  arguments: (argument_list
    (identifier) @constructor.param)*
  type_arguments: (type_arguments
    (type_identifier) @type.arg)*) @lifecycle.instantiation

; 构造函数定义关系 - 使用锚点确保精确匹配
(constructor_declaration
  name: (identifier) @constructor.method
  parameters: (formal_parameters
    (formal_parameter
      name: (identifier) @constructor.param)*)*) @lifecycle.constructor.definition

; 初始化块关系 - 使用交替模式
[
  (class_declaration
    name: (identifier) @initialized.class
    body: (class_body
      (instance_initializer
        (block) @init.block)))
  (class_declaration
    name: (identifier) @static.initialized.class
    body: (class_body
      (static_initializer
        (block) @static.init.block)))
] @lifecycle.initialization.block

; 析构方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @destructor.method
  parameters: (formal_parameters)
  (#match? @destructor.method "finalize$")) @lifecycle.destructor.definition

; AutoCloseable接口实现关系 - 使用谓词过滤
(class_declaration
  name: (identifier) @closeable.class
  super_interfaces: (super_interfaces
    (type_list
      (type_identifier) @implemented.interface))
  (#match? @implemented.interface "AutoCloseable$")) @lifecycle.autocloseable.implementation

; Close方法实现关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @close.method
  parameters: (formal_parameters)
  (#match? @close.method "close$")) @lifecycle.close.method

; 生命周期注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @lifecycle.method
  (annotation
    name: (identifier) @lifecycle.annotation
    (#match? @lifecycle.annotation "^(PostConstruct|PreDestroy|Initialized|Destroyed)$"))) @lifecycle.annotated.method

; Spring生命周期注解关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @spring.lifecycle.method
  (annotation
    name: (identifier) @spring.annotation
    (#match? @spring.annotation "^(PostConstruct|PreDestroy|Bean|Component)$"))) @lifecycle.spring.lifecycle

; 事件监听器方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @event.listener.method
  (annotation
    name: (identifier) @event.annotation
    (#match? @event.annotation "^(EventListener|Subscribe|Handle)$"))) @lifecycle.event.listener

; 资源获取方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @acquire.method
  return_type: (type_identifier) @resource.type
  (#match? @acquire.method "^(acquire|open|start|connect|init)$")) @lifecycle.resource.acquisition

; 资源释放方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @release.method
  (#match? @release.method "^(release|close|stop|disconnect|cleanup|destroy)$")) @lifecycle.resource.release

; 线程生命周期关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @thread.lifecycle.method
  (#match? @thread.lifecycle.method "^(start|run|stop|interrupt|join)$")) @lifecycle.thread.lifecycle

; 事务生命周期关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @transaction.method
  (annotation
    name: (identifier) @transaction.annotation
    (#match? @transaction.annotation "^(Transactional|Begin|Commit|Rollback)$"))) @lifecycle.transaction.lifecycle

; 缓存生命周期关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @cache.method
  (annotation
    name: (identifier) @cache.annotation
    (#match? @cache.annotation "^(CacheEvict|CachePut|Cacheable)$"))) @lifecycle.cache.lifecycle

; 文件操作生命周期 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @file.object
    field: (field_identifier) @file.method)
  arguments: (argument_list
    (identifier) @file.param)*)
  (#match? @file.method "^(open|create|delete|close|flush)$")) @lifecycle.file.operation

; 网络连接生命周期 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @network.object
    field: (field_identifier) @network.method)
  arguments: (argument_list
    (identifier) @network.param)*)
  (#match? @network.method "^(connect|disconnect|close|send|receive)$")) @lifecycle.network.operation

; 数据库连接生命周期 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @database.object
    field: (field_identifier) @database.method)
  arguments: (argument_list
    (identifier) @database.param)*)
  (#match? @database.method "^(getConnection|closeConnection|commit|rollback)$")) @lifecycle.database.operation

; HTTP服务器生命周期 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @http.server
    field: (field_identifier) @http.method)
  arguments: (argument_list
    (identifier) @http.param)*)
  (#match? @http.method "^(ListenAndServe|Serve|Shutdown|Close)$")) @lifecycle.http.operation

; 定时器生命周期 - 使用谓词过滤
(method_invocation
  function: [
    (identifier) @timer.function
    (field_access
      object: (identifier) @timer.object
      field: (field_identifier) @timer.method)
  ]
  arguments: (argument_list
    (identifier) @timer.param)*)
  (#match? @timer.function "^(schedule|cancel|start|stop)$")) @lifecycle.timer.operation

; 集合初始化生命周期 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @collection.object
    field: (field_identifier) @collection.method)
  arguments: (argument_list
    (identifier) @collection.param)*)
  (#match? @collection.method "^(add|addAll|clear|remove|removeAll)$")) @lifecycle.collection.lifecycle

; 流操作生命周期 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @stream.object
    field: (field_identifier) @stream.method)
  arguments: (argument_list
    (identifier) @stream.param)*)
  (#match? @stream.method "^(collect|forEach|close|onClose)$")) @lifecycle.stream.lifecycle

; 异步操作生命周期 - 使用谓词过滤
(method_invocation
  function: (field_access
    object: (identifier) @async.object
    field: (field_identifier) @async.method)
  arguments: (argument_list
    (identifier) @async.param)*)
  (#match? @async.method "^(submit|execute|get|cancel|complete)$")) @lifecycle.async.operation
`;