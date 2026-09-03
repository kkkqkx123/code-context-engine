# 模式模板详细设计

## 概述

本文档详细描述每种模式模板的生成策略。系统支持 18 种模式模板，分为设计模式（10种）和样板模式（8种）。

## 设计模式模板（Design Pattern Templates）

### 1. BuilderTemplate

**模式特征**：构建器类，包含 `build()` 方法和 `set_*` 方法

**BM25 输出**：
```
"user builder user build name email set_name set_email construct chain"
```

**Embedding 输出**：
```rust
["Builder for constructing User instances. Supports step-by-step configuration of user properties.",
 "Constructs and returns a fully configured User instance with all specified properties.",
 "Sets the name property for the User being built.",
 "Sets the email property for the User being built."]
```

### 2. FactoryTemplate

**模式特征**：工厂类，包含 `create_*` / `make_*` 方法

**BM25 输出**：
```
"user factory user create make factory method product"
```

**Embedding 输出**：
```rust
["Factory for creating User instances. Encapsulates object creation logic and manages product lifecycle.",
 "Creates and returns a new User instance with specific configuration.",
 "Registers a product type for factory production."]
```

### 3. SingletonTemplate

**模式特征**：单例类，包含 `get_instance()` 静态方法和私有构造函数

**BM25 输出**：
```
"config manager singleton config get_instance instance global"
```

**Embedding 输出**：
```rust
["Singleton class ConfigManager. Provides a single, globally accessible instance for configuration management."]
```

### 4. StrategyTemplate

**模式特征**：策略接口 + 多个实现类，包含 `execute()` / `apply()` 方法

**BM25 输出**：
```
"payment strategy payment execute pay credit_card paypal strategy algorithm"
```

**Embedding 输出**：
```rust
["Strategy pattern for payment processing. Defines interchangeable payment algorithms.",
 "Executes the payment strategy. Processes payment according to selected method."]
```

### 5. ObserverTemplate

**模式特征**：Subject 类通知 Observer 接口的实现

**BM25 输出**：
```
"event manager observer event notify subscribe unsubscribe notification"
```

**Embedding 输出**：
```rust
["Observer pattern with EventManager subject and 3 observers. Enables event-driven notification system.",
 "Notifies all registered observers of state changes.",
 "Receives notification from subject. Updates observer state."]
```

### 6. AdapterTemplate

**模式特征**：适配器类，实现目标接口并包装被适配者

**BM25 输出**：
```
"json adapter xml json adapt convert transform interface"
```

**Embedding 输出**：
```rust
["Adapter that converts XML data to JSON format. Bridges incompatible interfaces.",
 "Converts XML input to JSON output."]
```

### 7. DecoratorTemplate

**模式特征**：装饰器类，实现相同接口并包装另一个实现

**BM25 输出**：
```
"logging decorator service logging log decorator wrapper intercept"
```

**Embedding 输出**：
```rust
["Decorator that adds logging functionality to service. Wraps and extends behavior.",
 "Executes the decorated service with logging."]
```

### 8. CompositeTemplate

**模式特征**：组合类，包含子组件集合

**BM25 输出**：
```
"ui container composite ui component add remove child container hierarchy"
```

**Embedding 输出**：
```rust
["Composite container for UI components. Manages a hierarchy of child components.",
 "Adds a child component to the container.",
 "Renders all child components recursively."]
```

### 9. TemplateMethodTemplate

**模式特征**：抽象模板类，定义算法骨架 + 子类实现步骤

**BM25 输出**：
```
"data processor template data process parse validate export template method"
```

**Embedding 输出**：
```rust
["Template method pattern for data processing. Defines algorithm skeleton with overridable steps.",
 "Executes the complete data processing pipeline."]
```

## 样板模式模板（Boilerplate Pattern Templates）

### 10. DtoTemplate

**模式特征**：数据传输对象，仅包含字段和 Getter/Setter

**BM25 输出**：
```
"user dto user get_name set_name get_email set_email data transfer"
```

**Embedding 输出**：
```rust
["Data Transfer Object for User data. Carries user information between application layers.",
 "Gets the name of the user.",
 "Gets the email of the user."]
```

### 11. RepositoryTemplate

**模式特征**：数据仓储，包含 CRUD 方法

**BM25 输出**：
```
"user repository user find_by_id save delete find_all crud data access"
```

**Embedding 输出**：
```rust
["Repository for User entity. Provides data access and CRUD operations.",
 "Finds a user by their unique identifier.",
 "Saves a user entity to the data store."]
```

### 12. OrmEntityTemplate

**模式特征**：ORM 实体类，包含注解/属性和关联

**BM25 输出**：
```
"user orm entity user table column id name email database entity"
```

**Embedding 输出**：
```rust
["ORM entity representing a user. Maps to the users table in the database."]
```

### 13. ServiceTemplate

**模式特征**：业务服务类，包含业务逻辑方法

**BM25 输出**：
```
"user service user create_user update_user delete_user business logic"
```

**Embedding 输出**：
```rust
["Service layer for user operations. Contains business logic for user management.",
 "Creates a new user with specified details.",
 "Updates an existing user's information."]
```

### 14. ConfigTemplate

**模式特征**：配置类，包含静态配置属性和加载方法

**BM25 输出**：
```
"app config config load get_property set_property settings configuration"
```

**Embedding 输出**：
```rust
["Application configuration class. Manages application settings and properties.",
 "Loads configuration from specified source.",
 "Gets a configuration property value."]
```

### 15. ValidatorTemplate

**模式特征**：验证器类，包含 `validate()` 和验证规则

**BM25 输出**：
```
"user validator user validate check verify rule constraint validation"
```

**Embedding 输出**：
```rust
["Validator for User objects. Enforces validation rules and constraints.",
 "Validates a user object and returns validation results."]
```

### 16. EventHandlerTemplate

**模式特征**：事件处理器，包含 `handle_*` 事件处理方法

**BM25 输出**：
```
"user event handler user handle_user_created handle_user_updated event event-driven"
```

**Embedding 输出**：
```rust
["Event handler for user-related events. Publishes: UserCreated, UserUpdated. Type: Async.",
 "Handles UserCreated event. Sends welcome email and initializes user profile.",
 "Handles UserUpdated event. Updates search index and invalidates cache."]
```

### 17. GuiCallbackTemplate

**模式特征**：GUI 回调/监听器，包含事件响应方法

**BM25 输出**：
```
"button click handler button on_click on_hover ui callback event"
```

**Embedding 输出**：
```rust
["GUI callback handler for button events. Responds to user interface interactions.",
 "Handles button click event. Triggers the associated action."]
```

## 模板选择逻辑

### PatternInfo 到模板的映射

| PatternInfo | 模板类 |
|-------------|--------|
| `Builder` | `BuilderTemplate` |
| `Factory` | `FactoryTemplate` |
| `GetterSetter` | `GetterSetterTemplate` |
| `Singleton` | `SingletonTemplate` |
| `Strategy` | `StrategyTemplate` |
| `Observer` | `ObserverTemplate` |
| `Adapter` | `AdapterTemplate` |
| `Decorator` | `DecoratorTemplate` |
| `Composite` | `CompositeTemplate` |
| `TemplateMethod` | `TemplateMethodTemplate` |
| `Dto` | `DtoTemplate` |
| `Repository` | `RepositoryTemplate` |
| `OrmEntity` | `OrmEntityTemplate` |
| `Service` | `ServiceTemplate` |
| `Config` | `ConfigTemplate` |
| `Validator` | `ValidatorTemplate` |
| `EventHandler` | `EventHandlerTemplate` |
| `GuiCallback` | `GuiCallbackTemplate` |
| `None` + `is_stdlib_group` | `StdlibGroupTemplate` |
| `None` + 其他 | `RegularGroupTemplate` |
