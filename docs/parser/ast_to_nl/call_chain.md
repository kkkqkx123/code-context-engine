# AST to Natural Language 完整调用链分析

## 顶层处理流程

```
EntityGroup[] (来自 Grouper)
    │
    ▼
AstToNlConverter::convert_entity_groups()
    │
    ├── [有插件匹配] ──► convert_entity_groups_batch()
    │       │               └── 插件 BM25/Embedding 生成器
    │       └── 插件处理失败 ──► 回退到内置处理
    │
    └── [无插件] ──► convert_entity_group() 逐个处理
            │
            ├── [插件优先] ──► try_plugin_generation()
            │
            └── [内置处理] ──► 按 GroupType 分发
```

## 实体组转换调用链

```
convert_entity_group(group, file_path, request)
    │
    ├── GroupType::ClassWithMethods
    │   │
    │   ├── pattern_dispatch!(pattern_info, ...)
    │   │   ├── PatternInfo::Builder        → convert_builder_group()
    │   │   ├── PatternInfo::GetterSetter   → convert_property_group()
    │   │   ├── PatternInfo::Factory        → convert_factory_group()
    │   │   ├── PatternInfo::Singleton      → convert_singleton_group()
    │   │   ├── PatternInfo::Strategy       → convert_strategy_group()
    │   │   ├── PatternInfo::Observer       → convert_observer_group()
    │   │   ├── PatternInfo::Adapter        → convert_adapter_group()
    │   │   ├── PatternInfo::Decorator      → convert_decorator_group()
    │   │   ├── PatternInfo::Composite      → convert_composite_group()
    │   │   ├── PatternInfo::TemplateMethod → convert_template_method_group()
    │   │   ├── PatternInfo::Dto            → convert_dto_group()
    │   │   ├── PatternInfo::Repository     → convert_repository_group()
    │   │   ├── PatternInfo::OrmEntity      → convert_orm_entity_group()
    │   │   ├── PatternInfo::Service        → convert_service_group()
    │   │   ├── PatternInfo::Config         → convert_config_group()
    │   │   ├── PatternInfo::Validator      → convert_validator_group()
    │   │   ├── PatternInfo::EventHandler   → convert_event_handler_group()
    │   │   └── PatternInfo::GuiCallback    → convert_gui_callback_group()
    │   │
    │   └── [无模式匹配] ──► convert_regular_class_group()
    │
    ├── GroupType::Standalone              → 单实体转换
    ├── GroupType::InterfaceWithImpls      → 接口+实现
    ├── GroupType::TraitWithImpls          → Trait+实现
    ├── GroupType::RelatedFunctions        → 相关函数组
    ├── GroupType::ModuleWithContents      → 模块+内容
    └── 其他 GroupType                     → 默认转换
```

## 模板分发调用链（内置转换）

```
convert_builder_group(group, summary, file_path, request)
    │
    ├── [BM25 模式] ──► GroupTemplateDispatcher::dispatch()
    │   │                   → BuilderTemplate.generate(group)
    │   │                   → String (关键词优化文本)
    │   │
    ├── [Embedding 模式] ──► GroupTemplateDispatcher::dispatch()
    │   │                       → BuilderTemplate.generate(group)
    │   │                       → Vec<String> (语义描述列表)
    │   │
    └── [Both 模式] ──► 同时生成 BM25 + Embedding
```

## 插件生成调用链

```
try_plugin_generation(group, file_path, request, registry)
    │
    ├── registry.get_bm25_generators(file_path, language)
    │       └── for each generator → generate_bm25_batch([group])
    │
    ├── registry.get_embedding_generators(file_path, language)
    │       └── for each generator → generate_embedding_batch([group])
    │
    └── 结果合并为 ConversionResult
```

## 分块器调用链

```
GroupChunker::chunk_groups(group_conversions, file_path)
    │
    ├── 计算总 token 数
    ├── 选择分块策略 (按 GroupType)
    │   ├── ClassWithMethods → ByMembers 策略
    │   ├── Standalone → BySentences 策略
    │   ├── ModuleWithContents → ByParagraphs 策略
    │   └── 其他 → 自动选择
    │
    ├── 重复组头部描述到每个分块（上下文保持）
    ├── 按 token 预算分组成员
    ├── 生成重叠区域
    ├── 记录分块关系
    │
    └── 返回 Vec<ChunkedResult>
```

## 分词回退链

```
TextSplitter::split(text, strategy, options)
    │
    ├── [ByMembers] ──► 按实体边界分割
    │       └── 单个成员超长 ──► BySentences 回退
    │
    ├── [BySentences] ──► 按句子边界分割
    │       └── 单句超长 ──► ByParagraphs 回退
    │
    ├── [ByParagraphs] ──► 按段落边界分割
    │       └── 段落超长 ──► ByLines 回退
    │
    └── [ByLines/ByTokens] ──► 按行/token 强制分割
            └── 最终回退
```
