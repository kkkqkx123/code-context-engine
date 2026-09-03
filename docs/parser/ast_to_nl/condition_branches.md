# AST to Natural Language 条件分支分析

## 1. 实体组类型分支

**位置**：`crates/cce_parser/src/ast_to_nl/converter/group_converter.rs:178-230`

```rust
match group.group_type {
    GroupType::ClassWithMethods => {
        pattern_dispatch!(self, &group.pattern_info, group, file_path, request, [
            Builder => convert_builder_group,
            GetterSetter => convert_property_group,
            Factory => convert_factory_group,
            Singleton => convert_singleton_group,
            Strategy => convert_strategy_group,
            Observer => convert_observer_group,
            Adapter => convert_adapter_group,
            Decorator => convert_decorator_group,
            Composite => convert_composite_group,
            TemplateMethod => convert_template_method_group,
            Dto => convert_dto_group,
            Repository => convert_repository_group,
            OrmEntity => convert_orm_entity_group,
            Service => convert_service_group,
            Config => convert_config_group,
            Validator => convert_validator_group,
            EventHandler => convert_event_handler_group,
            GuiCallback => convert_gui_callback_group,
        ]);
        self.convert_regular_class_group(group, file_path, request)
    }
    GroupType::Standalone | GroupType::InterfaceWithImpls | ... => {
        // 默认处理
    }
}
```

**分支条件**：

| GroupType | 模式检查 | 处理方法 |
|-----------|---------|----------|
| `ClassWithMethods` | 18种模式 | 模式分发 → 默认类转换回退 |
| `InterfaceWithImpls` | 否 | 直接转换 |
| `TraitWithImpls` | 否 | 直接转换 |
| `RelatedFunctions` | 否 | 直接转换 |
| `Standalone` | 否 | 单实体转换 |
| `ModuleWithContents` | 否 | 模块转换 |
| `TestSuiteWithCases` | 否 | 测试套件转换 |

## 2. 插件处理分支

**位置**：`group_converter.rs:170-175`

```rust
// Step 1: Try plugin-based generation first
if let Some(ref registry) = self.plugin_registry {
    if let Some(result) = self.try_plugin_generation(group, file_path, request, registry) {
        return result;
    }
}
// Step 2: Fallback to built-in patterns
```

**优先级**：插件优先 → 内置模式回退

## 3. 模板分发分支

**位置**：
- BM25: `bm25/templates/dispatcher.rs:82-179`
- Embedding: `embedding/templates/dispatcher.rs:108-205`

```rust
match &group.pattern_info {
    // 设计模式 (10种)
    PatternInfo::Builder => BuilderTemplate,
    PatternInfo::Factory => FactoryTemplate,
    // ... 共10种设计模式
    
    // 样板模式 (8种)
    PatternInfo::Dto => DtoTemplate,
    PatternInfo::Repository => RepositoryTemplate,
    // ... 共8种样板模式
    
    // 无模式
    PatternInfo::None => {
        if group.is_stdlib_group {
            StdlibGroupTemplate
        } else {
            RegularGroupTemplate
        }
    }
}
```

## 4. 输出模式分支

**位置**：`group_converter.rs`

| 模式 | BM25 文本 | Embedding 文本 |
|------|-----------|---------------|
| `Bm25` | ✅ 生成 | ❌ 空 |
| `Embedding` | ❌ 空 | ✅ 生成 |
| `Both` | ✅ 生成 | ✅ 生成 |

## 5. 成员角色分支

**位置**：`common/templates/group_trait_base.rs`

| MemberRole | 是否生成独立描述 |
|------------|----------------|
| `CoreMethod` | ✅ 是 |
| `SignificantMethod` | ✅ 是 |
| `BoilerplateMethod` | ❌ 否（压缩到组描述） |
| `RegularMember` | ❌ 否 |

## 6. 分块策略分支

**位置**：`chunker/chunker.rs`

| 场景 | 分块策略 |
|------|---------|
| 类+方法组 | ByMembers（按实体边界） |
| 独立实体 | BySentences（按句子边界） |
| 模块组 | ByParagraphs（按段落边界） |
| 嵌套类组 | ByNestedGroups |
| 文本超长 | 多级回退（成员→句子→段落→行→token） |
