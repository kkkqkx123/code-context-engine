# 模板实现与 Grouper 类型约束分析

## 一、类型依赖关系

### 1.1 核心类型层次

```
grouper/types/
├── pattern.rs              # PatternInfo, MemberRole, MethodClassification
├── design_pattern.rs       # BuilderSummary, FactorySummary, etc.
├── boilerplate_pattern.rs  # DtoSummary, RepositorySummary, etc.
├── group.rs                # EntityGroup, GroupType, GroupRole
└── category.rs             # StdlibCategory, CompressionLevel

ast_to_nl/
├── common/
│   ├── normalizer.rs       # NameNormalizer
│   ├── utils.rs            # 工具函数
│   └── templates/
│       ├── group_trait_base.rs  # GroupTemplateBase trait
│       └── helpers.rs          # TemplateHelpers
├── bm25/templates/
│   ├── group_trait.rs     # GroupTemplate trait (BM25)
│   └── dispatcher.rs      # GroupTemplateDispatcher
└── embedding/templates/
    ├── group_trait.rs     # GroupTemplate trait (Embedding)
    └── dispatcher.rs      # GroupTemplateDispatcher
```

### 1.2 类型约束关系

```
┌─────────────────────────────────────────────────────────┐
│                  grouper/types                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ PatternInfo  │  │ EntityGroup  │  │ MemberRole   │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└──────────┬───────────────┬─────────────────┬────────────┘
           │               │                 │
           ▼               ▼                 ▼
┌─────────────────────────────────────────────────────────┐
│                 ast_to_nl/common                         │
│  ┌──────────────────┐  ┌──────────────────────────┐    │
│  │ GroupTemplateBase│  │ TemplateHelpers/Normalizer│    │
│  └──────────────────┘  └──────────────────────────┘    │
└──────────┬──────────────────────────────────────┬────────┘
           │                                      │
           ▼                                      ▼
┌──────────────────────┐  ┌──────────────────────────┐
│ bm25/templates       │  │ embedding/templates       │
│ ┌────────────────┐  │  │ ┌──────────────────────┐  │
│ │ GroupTemplate  │  │  │ │ GroupTemplate        │  │
│ │ (extends Base) │  │  │ │ (extends Base)       │  │
│ └────────────────┘  │  │ └──────────────────────┘  │
│ ┌────────────────┐  │  │ ┌──────────────────────┐  │
│ │PatternGroupTemp│  │  │ │PatternGroupTemplate  │  │
│ └────────────────┘  │  │ └──────────────────────┘  │
└──────────────────────┘  └──────────────────────────┘
```

## 二、类型约束分析

### 2.1 PatternInfo 约束

**定义位置**: `grouper/types/pattern.rs`

**约束关系**:
- `PatternInfo` 是一个枚举，包含所有模式类型
- 每个模式变体携带对应的 Summary 类型
- 模板调度器通过模式匹配分发到对应模板

**当前支持的模式**:

| 分类 | 模式 | Summary 类型 |
|------|------|-------------|
| 设计模式 | Builder | `BuilderSummary` |
| 设计模式 | Factory | `FactorySummary` |
| 设计模式 | GetterSetter | `GetterSetterSummary` |
| 设计模式 | Singleton | `SingletonSummary` |
| 设计模式 | Strategy | `StrategySummary` |
| 设计模式 | Observer | `ObserverSummary` |
| 设计模式 | Adapter | `AdapterSummary` |
| 设计模式 | Decorator | `DecoratorSummary` |
| 设计模式 | Composite | `CompositeSummary` |
| 设计模式 | TemplateMethod | `TemplateMethodSummary` |
| 样板模式 | Dto | `DtoSummary` |
| 样板模式 | Repository | `RepositorySummary` |
| 样板模式 | OrmEntity | `OrmEntitySummary` |
| 样板模式 | Service | `ServiceSummary` |
| 样板模式 | Config | `ConfigSummary` |
| 样板模式 | Validator | `ValidatorSummary` |
| 样板模式 | EventHandler | `EventHandlerSummary` |
| 样板模式 | GuiCallback | `GuiCallbackSummary` |

### 2.2 MemberRole 约束

**定义位置**: `grouper/types/pattern.rs`

| 角色 | 独立描述 | 说明 |
|------|---------|------|
| `CoreMethod` | ✅ 是 | 核心功能方法 |
| `SignificantMethod` | ✅ 是 | 重要方法 |
| `BoilerplateMethod` | ❌ 否 | 样板方法，压缩到组描述 |
| `Constructor` | ❌ 否 | 构造函数 |
| `EventHandler` | ✅ 是 | 事件处理方法 |
| `FieldAccessor` | ❌ 否 | 字段访问器 |
| `StaticUtility` | ❌ 否 | 静态工具方法 |
| `HelperMethod` | ❌ 否 | 辅助方法 |

## 三、约束问题分析

### 3.1 当前设计的优点

1. **编译时类型安全**: PatternInfo 枚举保证类型匹配
2. **内存效率**: SmallVec 减少堆分配
3. **代码复用**: GroupTemplateBase 提供公共方法
4. **清晰分层**: Grouper → Common → BM25/Embedding

### 3.2 已知约束问题

| 问题 | 影响 | 优先级 |
|------|------|--------|
| PatternInfo 扩展性差 | 新增模式需要修改多处 | 高 |
| member_roles 查找 O(n) | 使用 SmallVec 线性查找 | 中 |
| Grouper 与模板耦合 | Grouper 必须知道所有模板类型 | 中 |

### 3.3 缓解措施

当前实现已使用 `HashMap` 替代 `SmallVec` 进行 O(1) 角色查找：

```rust
// group_trait_base.rs - 使用 role_map
let role_map = group.build_role_map();
```

## 四、结论

当前设计在**类型安全**和**性能**方面表现良好，分层结构清晰。主要扩展性约束来自 `PatternInfo` 枚举的封闭性，但这对当前 18 种模式的规模是可接受的。
