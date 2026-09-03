# Grouper 实体组压缩描述分析

## 概述

本文档分析 Grouper 模块产生的实体组（EntityGroup）结构，以及 AstToNl 如何处理这些组以实现描述压缩。

## 1. Grouper 提取的组结构

### 1.1 EntityGroup 结构

**位置**：`crates/cce_grouper/` 或 `crates/cce_parser/src/grouper/`

```rust
pub struct EntityGroup {
    pub group_id: String,           // 唯一组ID
    pub group_type: GroupType,      // 组类型
    pub name: String,               // 组名称
    pub header_id: Option<EntityId>,// 头部实体ID（如类）
    pub members: Vec<GroupedEntity>,// 成员实体
    pub pattern_info: PatternInfo,  // 模式信息
    pub member_roles: SmallVec<[(EntityId, MemberRole)]>, // 成员角色
    pub is_stdlib_group: bool,      // 是否为标准库组
    pub stdlib_category: Option<StdlibCategory>, // 标准库分类
    pub language: String,           // 编程语言
    pub context_hints: Vec<String>, // 上下文提示
    pub span: Span,                 // 源跨度
    pub combined_source: Arc<str>,  // 组合的源代码
}
```

### 1.2 GroupType

| 类型 | 说明 | 压缩策略 |
|------|------|---------|
| `ClassWithMethods` | 类+方法 | 按模式分发，压缩样板方法 |
| `InterfaceWithImpls` | 接口+实现 | 压缩实现细节 |
| `TraitWithImpls` | Trait+实现 | 压缩默认实现 |
| `RelatedFunctions` | 相关函数 | 函数级描述 |
| `Standalone` | 独立实体 | 单实体描述 |
| `ModuleWithContents` | 模块+内容 | 文件级描述 |
| `TestSuiteWithCases` | 测试套件 | 测试级描述 |

### 1.3 GroupedEntity（轻量实体）

```rust
pub struct GroupedEntity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub name: String,
    pub signature: String,
    pub parameters: SmallVec<[(String, Option<String>)]>,
    pub return_type: Option<String>,
    pub doc_comment: Option<String>,
    pub is_stdlib: bool,
    pub stdlib_category: Option<StdlibCategory>,
    pub metadata: EntityMetadata,
}
```

## 2. AST to NL 阶段的组处理

### 2.1 转换入口

`AstToNlConverter::convert_entity_groups()` 接收 `&[EntityGroup]`，返回 `Vec<GroupConversions>`。

### 2.2 压缩机制

#### 角色过滤

只有 `CoreMethod` 和 `SignificantMethod` 角色的成员生成独立描述：
- 核心方法 → 独立描述（如 `execute()`, `build()`, `validate()`）
- 重要方法 → 独立描述（如 `processPayment()`, `sendEmail()`）
- 样板方法 → 压缩到组描述（如 getter/setter、辅助方法）

#### 数量减少

| 场景 | 旧方案描述数 | 新方案描述数 |
|------|------------|------------|
| 10个方法的DTO类 | 1 + 10 = 11 | 1 + 0（全部压缩）= 1 |
| Builder类（build + 5个setter） | 1 + 6 = 7 | 1 + 1（仅build）= 2 |
| 服务类（3核心 + 5辅助） | 1 + 8 = 9 | 1 + 3 = 4 |
| 事件处理器（4个handle方法） | 1 + 4 = 5 | 1 + 4 = 5 |

## 3. 描述生成流程

```
EntityGroup
    │
    ├── pattern_info 非空 ──► 模式模板
    │       │
    │       ├── BM25: 生成1个字符串（原始名 + 规范化名 + 关键词 + 描述）
    │       └── Embedding: 生成 Vec<String>（组描述 + 重要成员描述）
    │
    ├── is_stdlib_group ──► StdlibGroupTemplate
    │       └── 精简描述（标准库分类 + 功能摘要）
    │
    └── 常规组 ──► RegularGroupTemplate
            └── 通用描述（类型 + 功能 + 重要成员）
```

## 4. 压缩效果

### 4.1 向量数量减少

假设每个描述对应一个向量：
- 10个方法的大型类：从 11 个向量减少到 1-4 个向量
- 平均减少率：60-80%

### 4.2 语义质量提升

- 组级描述包含上下文，向量表示更丰富
- 样板代码不再产生噪声向量
- 关键方法获得独立的、有上下文的描述

### 4.3 搜索质量提升

- 减少低质量向量对检索的干扰
- 类级搜索更精准（一个向量描述整个类）
- 方法级搜索更聚焦（仅有核心方法向量）
