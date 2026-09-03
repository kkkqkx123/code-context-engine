# AST to Natural Language 模板系统

## 概述

模板系统是 AstToNl 转换的核心，负责将 `EntityGroup` 转换为自然语言描述。系统采用 **双路径模板架构**：

- **BM25 模板路径**：生成保留代码符号的混合文本，优化关键词匹配
- **Embedding 模板路径**：生成纯语义描述，优化向量嵌入

## 模板层次结构

```
common/templates/
├── group_trait_base.rs    # GroupTemplateBase trait（基类）
└── helpers.rs             # 模板辅助函数（名称规范化、关键词提取）

bm25/templates/
├── group_trait.rs         # GroupTemplate trait (BM25) → fn generate() -> String
├── dispatcher.rs          # BM25 GroupTemplateDispatcher
├── design_patterns.rs     # 设计模式模板
├── boilerplate_patterns.rs # 样板模式模板
├── regular.rs             # 常规实体模板
└── stdlib.rs              # 标准库模板

embedding/templates/
├── group_trait.rs         # GroupTemplate trait (Embedding) → fn generate() -> Vec<String>
├── dispatcher.rs          # Embedding GroupTemplateDispatcher
├── design_patterns.rs     # 设计模式模板
├── boilerplate_patterns.rs # 样板模式模板
├── regular.rs             # 常规实体模板
└── stdlib.rs              # 标准库模板
```

## 核心 Trait 定义

### GroupTemplateBase（公共基类）

**位置**: `common/templates/group_trait_base.rs`

```rust
pub trait GroupTemplateBase {
    /// 判断成员是否需要独立描述
    fn should_generate_member_description(&self, role: &MemberRole) -> bool {
        role.has_independent_description()
    }

    /// 过滤需要独立描述的成员
    fn filter_significant_members<'a>(&self, group: &'a EntityGroup) -> Vec<&'a GroupedEntity>;

    /// 获取成员角色
    fn get_member_role(&self, group: &EntityGroup, member_id: EntityId) -> Option<MemberRole>;

    /// 统计重要成员数量
    fn count_significant_members(&self, group: &EntityGroup) -> usize;
}
```

### BM25 GroupTemplate

**位置**: `bm25/templates/group_trait.rs`

```rust
pub trait GroupTemplate: GroupTemplateBase {
    /// 生成关键词优化的文本
    /// 返回单个字符串，包含：
    /// - 原始名称（精确匹配）
    /// - 规范化名称（模糊匹配）
    /// - 关键词（关键词搜索）
    /// - 组描述
    fn generate(&self, group: &EntityGroup) -> String;
}

pub trait PatternGroupTemplate<Summary>: GroupTemplate {
    /// 生成带模式信息的文本
    fn generate_with_pattern(&self, group: &EntityGroup, summary: &Summary) -> String;
}
```

### Embedding GroupTemplate

**位置**: `embedding/templates/group_trait.rs`

```rust
pub trait GroupTemplate: GroupTemplateBase {
    /// 生成语义描述
    /// 返回 Vec<String>：
    /// - 第一个元素：组整体描述
    /// - 后续元素：重要成员的独立描述
    fn generate(&self, group: &EntityGroup) -> Vec<String>;
}
```

## BM25 模板生成策略

BM25 模板生成包含以下信息的混合文本：

| 信息类型 | 示例 | 目的 |
|---------|------|------|
| 原始名称 | `UserBuilder` | 精确匹配 |
| 规范化名称 | `user builder` | 模糊匹配 |
| 关键词 | `build`, `construct`, `create` | 关键词搜索 |
| 组描述 | "Builds User instances with name and email" | 语义理解 |

## Embedding 模板生成策略

Embedding 模板生成纯语义描述：

| 信息类型 | 包含/排除 | 说明 |
|---------|-----------|------|
| 原始名称 | ✗ 排除 | 防止过拟合代码符号 |
| 规范化名称 | ✗ 排除 | 仅用于角色判断 |
| 参数名 | ✗ 排除 | 仅保留数量 |
| 参数类型 | ✗ 排除 | 使用语义描述 |
| 返回类型 | ✗ 排除 | 使用语义描述 |
| 文档注释 | ✓ 保留（核心意图） | 语义信息 |
| 意图推断 | ✓ 使用 | 模式感知 |

## 模板分发逻辑

`GroupTemplateDispatcher` 的分发逻辑：

```
1. PatternInfo::Builder       → BuilderTemplate
2. PatternInfo::Factory       → FactoryTemplate
3. PatternInfo::GetterSetter  → GetterSetterTemplate
4. PatternInfo::Singleton     → SingletonTemplate
5. PatternInfo::Strategy      → StrategyTemplate
6. PatternInfo::Observer      → ObserverTemplate
7. PatternInfo::Adapter       → AdapterTemplate
8. PatternInfo::Decorator     → DecoratorTemplate
9. PatternInfo::Composite     → CompositeTemplate
10. PatternInfo::TemplateMethod → TemplateMethodTemplate
11. PatternInfo::Dto           → DtoTemplate
12. PatternInfo::Repository    → RepositoryTemplate
13. PatternInfo::OrmEntity     → OrmEntityTemplate
14. PatternInfo::Service       → ServiceTemplate
15. PatternInfo::Config        → ConfigTemplate
16. PatternInfo::Validator     → ValidatorTemplate
17. PatternInfo::EventHandler  → EventHandlerTemplate
18. PatternInfo::GuiCallback   → GuiCallbackTemplate
19. PatternInfo::None + is_stdlib_group → StdlibGroupTemplate
20. PatternInfo::None + 其他    → RegularGroupTemplate
```
