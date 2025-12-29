/*
Java Function Annotation Tree-Sitter Query Patterns
Shared queries for function annotations used across entities and relationships
*/
export default `
; 方法注解查询 - 使用量词操作符
(method_declaration
  (modifiers
    (annotation) @method.annotation)*
  name: (identifier) @method.name
  body: (block) @method.body) @method.annotation

; 构造函数注解查询
(constructor_declaration
  (modifiers
    (annotation) @constructor.annotation)*
  name: (identifier) @constructor.name
  body: (block) @constructor.body) @constructor.annotation

; 重写方法注解 - 使用谓词过滤
(method_declaration
  name: (identifier) @overridden.method
  (annotation
    name: (identifier) @override.annotation
    (#match? @override.annotation "Override$"))) @override.method.annotation

; 生命周期注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @lifecycle.method
  (annotation
    name: (identifier) @lifecycle.annotation
    (#match? @lifecycle.annotation "^(PostConstruct|PreDestroy|Initialized|Destroyed)$"))) @lifecycle.method.annotation

; Spring注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @spring.method
  (annotation
    name: (identifier) @spring.annotation
    (#match? @spring.annotation "^(PostConstruct|PreDestroy|Bean|Component|Service|Repository|Controller)$"))) @spring.method.annotation

; 事务注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @transaction.method
  (annotation
    name: (identifier) @transaction.annotation
    (#match? @transaction.annotation "^(Transactional|Begin|Commit|Rollback)$"))) @transaction.method.annotation

; 缓存注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @cache.method
  (annotation
    name: (identifier) @cache.annotation
    (#match? @cache.annotation "^(CacheEvict|CachePut|Cacheable)$"))) @cache.method.annotation

; 事件监听器注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @event.listener.method
  (annotation
    name: (identifier) @event.annotation
    (#match? @event.annotation "^(EventListener|Subscribe|Handle)$"))) @event.listener.annotation

; 依赖注入注解方法关系 - 使用谓词过滤
(method_declaration
  parameters: (formal_parameters
    (formal_parameter
      name: (identifier) @injected.param
      (annotation
        name: (identifier) @inject.annotation
        (#match? @inject.annotation "^(Inject|Autowired|Value)$")))+)) @dependency.injection.annotation

; 工厂方法注解关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @factory.method
  (annotation
    name: (identifier) @factory.annotation
    (#match? @factory.annotation "^(Bean|Factory|Producer|Builder)$"))
  return_type: (type_identifier) @product.type) @factory.method.annotation

; 测试注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @test.method
  (annotation
    name: (identifier) @test.annotation
    (#match? @test.annotation "^(Test|Before|After|BeforeClass|AfterClass)$"))) @test.method.annotation

; 异步注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @async.method
  (annotation
    name: (identifier) @async.annotation
    (#match? @async.annotation "^(Async|EnableAsync)$"))) @async.method.annotation

; 调度注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @scheduled.method
  (annotation
    name: (identifier) @scheduled.annotation
    (#match? @scheduled.annotation "^(Scheduled|EnableScheduling)$"))) @scheduled.method.annotation

; 安全注解方法关系 - 使用谓词过滤
(method_declaration
  name: (identifier) @security.method
  (annotation
    name: (identifier) @security.annotation
    (#match? @security.annotation "^(PreAuthorize|PostAuthorize|Secured|RolesAllowed)$"))) @security.method.annotation

; 字段注解查询 - 使用量词操作符
(field_declaration
  (modifiers
    (annotation) @field.annotation)*
  declarator: (variable_declarator
    name: (identifier) @field.name)
  type: (_) @field.type) @field.annotation

; 配置属性注解关系 - 使用谓词过滤
(field_declaration
  declarator: (variable_declarator
    name: (identifier) @config.property)
  (annotation
    name: (identifier) @config.annotation
    (#match? @config.annotation "^(Value|Property|Configuration)$"))) @configuration.property.annotation
`;