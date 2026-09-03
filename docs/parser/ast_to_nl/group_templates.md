# 实体组模板设计

## 概述

实体组模板（GroupTemplate）是 AstToNl 转换的核心抽象。每个模板负责将一种特定类型的 `EntityGroup` 转换为自然语言描述。

## 模板层次结构

```
GroupTemplateBase (公共基类)
    ├── BM25 GroupTemplate (关键词优化，返回 String)
    │   ├── PatternGroupTemplate (带模式信息，泛型)
    │   │   ├── BuilderTemplate
    │   │   ├── FactoryTemplate
    │   │   ├── GetterSetterTemplate
    │   │   ├── SingletonTemplate
    │   │   ├── StrategyTemplate
    │   │   ├── ObserverTemplate
    │   │   ├── AdapterTemplate
    │   │   ├── DecoratorTemplate
    │   │   ├── CompositeTemplate
    │   │   ├── TemplateMethodTemplate
    │   │   ├── DtoTemplate
    │   │   ├── RepositoryTemplate
    │   │   ├── OrmEntityTemplate
    │   │   ├── ServiceTemplate
    │   │   ├── ConfigTemplate
    │   │   ├── ValidatorTemplate
    │   │   ├── EventHandlerTemplate
    │   │   └── GuiCallbackTemplate
    │   ├── StdlibGroupTemplate (标准库)
    │   └── RegularGroupTemplate (常规)
    │
    └── Embedding GroupTemplate (语义优化，返回 Vec<String>)
        ├── PatternGroupTemplate (带模式信息，泛型)
        │   ├── (同上18种模式模板)
        ├── StdlibGroupTemplate
        └── RegularGroupTemplate
```

## 核心 Trait 定义

### GroupTemplateBase

```rust
pub trait GroupTemplateBase {
    /// 检查成员是否需要独立描述
    fn should_generate_member_description(&self, role: &MemberRole) -> bool;

    /// 过滤需要独立描述的成员（使用 HashMap O(1) 查找）
    fn filter_significant_members<'a>(&self, group: &'a EntityGroup) -> Vec<&'a GroupedEntity>;

    /// 获取成员角色
    fn get_member_role(&self, group: &EntityGroup, member_id: EntityId) -> Option<MemberRole>;

    /// 统计重要成员数量
    fn count_significant_members(&self, group: &EntityGroup) -> usize;
}
```

### BM25 GroupTemplate

```rust
pub trait GroupTemplate: GroupTemplateBase {
    /// 生成关键词优化的文本
    fn generate(&self, group: &EntityGroup) -> String;
}

pub trait PatternGroupTemplate<Summary>: GroupTemplate {
    /// 生成带模式信息的文本
    fn generate_with_pattern(&self, group: &EntityGroup, summary: &Summary) -> String;
}
```

### Embedding GroupTemplate

```rust
pub trait GroupTemplate: GroupTemplateBase {
    /// 生成语义描述列表
    fn generate(&self, group: &EntityGroup) -> Vec<String>;
}

pub trait PatternGroupTemplate<Summary>: GroupTemplate {
    /// 生成带模式信息的语义描述列表
    fn generate_with_pattern(&self, group: &EntityGroup, summary: &Summary) -> Vec<String>;
}
```

## 模板分发器

### BM25 GroupTemplateDispatcher

```rust
pub struct GroupTemplateDispatcher {
    // 设计模式 (10个)
    builder_template: BuilderTemplate,
    factory_template: FactoryTemplate,
    getter_setter_template: GetterSetterTemplate,
    singleton_template: SingletonTemplate,
    strategy_template: StrategyTemplate,
    observer_template: ObserverTemplate,
    adapter_template: AdapterTemplate,
    decorator_template: DecoratorTemplate,
    composite_template: CompositeTemplate,
    template_method_template: TemplateMethodTemplate,

    // 样板模式 (8个)
    dto_template: DtoTemplate,
    repository_template: RepositoryTemplate,
    orm_entity_template: OrmEntityTemplate,
    service_template: ServiceTemplate,
    config_template: ConfigTemplate,
    validator_template: ValidatorTemplate,
    event_handler_template: EventHandlerTemplate,
    gui_callback_template: GuiCallbackTemplate,

    // 标准库 + 常规
    stdlib_template: StdlibGroupTemplate,
    regular_template: RegularGroupTemplate,
}
```

## 模板辅助函数

**位置**：`common/templates/helpers.rs`

```rust
/// 规范化名称（PascalCase/snake_case → 小写加下划线）
fn normalize_name(name: &str) -> String;

/// 提取名称中的关键词
fn extract_keywords(name: &str) -> Vec<String>;

/// 组合文本（去重、小写）
fn combine_text<'a>(parts: impl IntoIterator<Item = &'a str>) -> String;
```

## 常规实体模板（RegularGroupTemplate）

当实体组没有检测到任何模式时，使用常规模板生成通用描述。

**BM25 输出示例**：
```
"user authentication login validate user authentication login validate credentials session"
```

**Embedding 输出示例**：
```
["User class with authentication methods for login and credential validation.",
 "Validates user credentials and returns authentication result.",
 "Manages user session lifecycle."]
```

## 标准库模板（StdlibGroupTemplate）

为标准库实体组生成精简描述。

**BM25 输出示例**：
```
"vec collection vector vec dynamic array memory contiguous"
```

**Embedding 输出示例**：
```
["Standard library collection: Vec. Provides dynamic array with contiguous memory storage.",
 "Creates a new, empty Vec."]
```
