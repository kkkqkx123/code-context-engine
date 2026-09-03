# Relation 数据流图

## 1. 整体架构图

```
AST (tree-sitter)
    │
    ▼
┌────────────────────────────────────────────────────────────────────┐
│                    RelationExtractor                                │
│  - 从 AST 中提取关系 (函数调用、继承、引用等)                       │
│  - 生成 Vec<Relation>                                               │
└────────────────────────────────────────────────────────────────────┘
    │
    ▼
┌────────────────────────────────────────────────────────────────────┐
│                    ParsedFile (raw_relations)                       │
│  - 提取过程中发生的 Relation → RawRelationData                     │
│  - RawRelationData { src, dst_name, relation_type, span }          │
└────────────────────────────────────────────────────────────────────┘
    │
    ▼
┌────────────────────────────────────────────────────────────────────┐
│                    IndexBuilder (关系索引构建)                       │
│  - 接收 ParsedFile 的 raw_relations 和 import_table(可选)                │
│  - 解析 imports/exports/dependencies (imports 可能已由 ParseCoordinator 预解析并缓存于 import_table) │
│  - 标准化导入/导出信息                                              │
│  - 跨文件符号解析                                                   │
│  - 构建 RelationIndex (线程安全)                                    │
└────────────────────────────────────────────────────────────────────┘
    │
    ▼
┌────────────────────────────────────────────────────────────────────┐
│                    RelationIndex (DashMap 线程安全索引)              │
│  ├── resolved_relation_index — EntityId → Vec<ResolvedRelation>    │
│  ├── function_index       — EntityId → Entity                     │
│  ├── entity_file_index    — EntityId → file_path                  │
│  ├── import_index         — file_id → ImportTable                  │
│  ├── export_index         — file_id → Vec<ExportInfo>              │
│  └── file_index           — file_id → FileInfo                    │
└────────────────────────────────────────────────────────────────────┘
    │
    ▼
┌────────────────────────────────────────────────────────────────────┐
│                    Query & 工具层                                    │
│  ├── RelationSearcher  — 调用链查询 (caller/callee)                │
│  ├── RelationBridge    — 关系桥接 (组装调用上下文)                  │
│  ├── SymbolLookup      — 符号查找 (跳转到定义/查找引用)             │
│  └── RelationBoost     — 查询结果调用关系权重提升                  │
└────────────────────────────────────────────────────────────────────┘
```

## 2. 详细数据流

### 2.1 解析阶段 (Parser)

```
AST (tree-sitter Tree)
    │
    ▼
RelationExtractor::extract(&self, tree, source, lang)
    │
    ├── 遍历 AST 节点
    ├── 识别调用表达式 (call_expression, method_invocation 等)
    ├── 识别导入语句 (import/use/include/require)
    ├── 识别继承/实现关系 (extends, implements, trait bounds)
    ├── 识别类型引用
    └── 识别模板关系 (元素包含、事件回调、属性绑定)
    │
    ▼
Vec<Relation>
  ├── DirectCall(src_id, dst_name, span)          — 直接函数调用
  ├── InstanceMethodCall(src_id, dst_name, span)  — 实例方法调用
  ├── StaticMethodCall(src_id, dst_name, span)    — 静态方法调用
  ├── ConstructorCall(src_id, dst_name, span)     — 构造函数调用
  ├── Inheritance(src_id, dst_name, span)         — 继承
  ├── Implementation(src_id, dst_name, span)      — 接口实现
  ├── ImportStatement(src_id, dst_name, span)     — 导入
  ├── TypeReference(src_id, dst_name, span)       — 类型引用
  └── ElementContains(src_id, dst_name, span)     — 模板包含关系
```

### 2.2 符号提取阶段 (Extraction)

```
符号提取器 (EntityExtractor + SymbolExtractor)
    │
    ├── 语言特定提取:
    │     ├── cce_parser/src/parser/extractor/symbol_extractor/rust.rs
    │     ├── cce_parser/src/parser/extractor/symbol_extractor/python.rs
    │     ├── cce_parser/src/parser/extractor/symbol_extractor/java.rs
    │     ├── cce_parser/src/parser/extractor/symbol_extractor/javascript.rs
    │     └── ... (14+ 种语言)
    │
    ├── 语言无关处理:
    │     ├── common/classifier.rs — 实体分类器
    │     ├── common/context.rs   — 上下文管理
    │     ├── common/helpers.rs   — 辅助函数
    │     └── traits.rs           — 提取器 trait 定义
    │
    └── 标准库检测:
          ├── stdlib/detector.rs — 标准库检测器
          └── stdlib/*.rs       — 各语言标准库配置
```

### 2.3 关系构建阶段 (IndexBuilder)

```
IndexBuilder (cce_parser::relation::IndexBuilder)
    │
    ├── add_parsed_files(files: &[&ParsedFile])
    │     ├── 遍历 ParsedFile.raw_relations
    │     ├── 尝试解析 dst_name 为具体的 EntityId
    │     ├── 推导 ImportTable (标准化导入)
    │     ├── 推导 ExportTable (标准化导出)
    │     └── 构建局部符号表
    │
    ├── 跨文件符号解析:
    │     ├── 根据 import 找到文件级别导出
    │     ├── 根据包名解析外部依赖
    │     └── 处理别名和重导出
    │
    └── index() → ThreadSafeIndex (RelationIndex)
```

### 2.4 关系索引结构 (RelationIndex)

```
RelationIndex (cce_parser::relation::index)
    │
    ├── resolved_relation_index: DashMap<EntityId, Vec<ResolvedRelation>>
    │     └── 实体到关系的映射 (调用者 → 所有出边)
    │
    ├── function_index: DashMap<EntityId, Entity>
    │     └── 实体元数据缓存
    │
    ├── entity_file_index: DashMap<EntityId, String>
    │     └── 实体到文件的映射
    │
    ├── import_index: DashMap<String, Vec<StandardizedImport>>
    │     └── 文件导入信息
    │
    ├── export_index: DashMap<String, Vec<StandardizedExport>>
    │     └── 文件导出信息
    │
    ├── file_index: DashMap<String, FileInfo>
    │     └── 文件元数据
    │
    └── resolved_relation_count() → 已解析的关系数量
```

### 2.5 查询阶段 (Query)

#### 2.5.1 调用链查询

```
RelationSearcher (cce_orchestrator::query::relation_searcher)
    │
    ├── get_callers(entity_id) → Vec<ResolvedRelation>
    │     └── 查询调用此实体的所有调用者
    │
    ├── get_callees(entity_id) → Vec<ResolvedRelation>
    │     └── 查询此实体调用的所有被调用者
    │
    ├── query_forward(entity_id, depth) → 前向调用链
    │     └── BFS/DFS 遍历 callee 链
    │
    ├── query_backward(entity_id, depth) → 后向调用链
    │     └── BFS/DFS 遍历 caller 链
    │
    └── find_path(from_id, to_id) → 调用路径
          └── 查找两个实体之间的最短调用路径
```

#### 2.5.2 继承层次查询

```
RelationSearcher
    │
    ├── get_base_classes(entity_id) → Vec<ResolvedRelation>
    │     └── 查询直接父类
    │
    ├── get_derived_classes(entity_id) → Vec<ResolvedRelation>
    │     └── 查询直接子类
    │
    ├── get_implemented_interfaces(entity_id) → Vec<ResolvedRelation>
    │     └── 查询实现的接口
    │
    ├── get_implementing_classes(entity_id) → Vec<ResolvedRelation>
    │     └── 查询实现此接口的类
    │
    └── get_inheritance_hierarchy(entity_id) → 完整继承树
```

#### 2.5.3 关系增强 (RelationBoost)

```
RelationBoost (cce_orchestrator::query::boost::relation)
    │
    ├── 在搜索结果中识别调用关系
    ├── 对调用频繁的实体进行分数提升
    └── 提升系数: score = score * (1 + boost_factor * call_count)
```

#### 2.5.4 SPSR-Graph 组装

```
SPSRGraphAssembler (cce_orchestrator::query::assembly)
    │
    ├── 接收基础搜索结果
    ├── 通过调用链扩展结果集
    │     ├── 前向传播: 找 callee
    │     └── 后向传播: 找 caller
    ├── 聚合上下文信息
    └── 返回丰富后的搜索结果
```

## 3. 核心数据结构

### 3.1 RelationType 枚举

**位置**：`cce_core/src/types/relation.rs`

```rust
pub enum RelationType {
    // === 调用关系 ===
    DirectCall,           // 直接函数调用
    InstanceMethodCall,   // 实例方法调用 (obj.method())
    StaticMethodCall,     // 静态方法调用 (Class::method())
    ConstructorCall,      // 构造函数调用
    GenericCall,          // 泛型函数调用
    MethodCall,           // 通用方法调用
    MacroCall,            // 宏调用

    // === 依赖关系 ===
    ImportStandard,       // import/use 语句
    ImportDynamic,        // 动态导入
    ImportType,           // 类型导入
    Export,               // 导出
    Require,              // require/include (PHP)
    Use,                  // use (Rust/PHP)
    Include,              // include (C/C++)

    // === 结构关系 ===
    Inheritance,          // 继承 (extends)
    Implementation,       // 接口实现 (implements)
    TraitBound,           // Trait 约束
    TypeReference,        // 类型引用
    FieldAccess,          // 字段访问

    // === 模板关系 ===
    ElementContains,      // 元素包含
    TemplateReference,    // 模板引用
    ParameterBinding,     // 参数绑定
    EventCallback,        // 事件回调
    TypeDefinition,       // 类型定义
}
```

### 3.2 Relation 结构

```rust
pub struct Relation {
    pub relation_type: RelationType,   // 关系类型
    pub src: EntityId,                 // 源实体 ID (调用者)
    pub dst: RelationTarget,           // 目标 (已解析/未解析)
    pub span: Span,                    // 源代码位置
    pub metadata: HashMap<String, String>,  // 扩展元数据
}

pub enum RelationTarget {
    Resolved {
        entity_id: Option<EntityId>,   // 已解析的实体 ID
        name: String,
    },
    Unresolved {
        name: String,                  // 未解析的目标名称
    },
}
```

### 3.3 ResolvedRelation 结构

```rust
pub struct ResolvedRelation {
    pub relation_type: RelationType,
    pub caller: EntityId,              // 调用者 (已解析)
    pub callee: Option<EntityId>,      // 被调用者 (已解析)
    pub callee_name: String,           // 被调用者名称
    pub callee_file: String,           // 被调用者所在文件
    pub span: Span,
    pub is_external: bool,             // 是否是外部调用
    pub is_external_inferred: bool,    // 是否是推断的外部调用
}
```

### 3.4 RawRelationData (ParsedFile 中)

```rust
pub struct RawRelationData {
    pub src: EntityId,
    pub dst_name: String,
    pub relation_type: RelationType,
    pub span: Span,
}
```

### 3.5 标准化导入/导出

```rust
pub struct StandardizedImport {
    pub kind: ImportKind,              // 导入类型
    pub source: String,                // 导入源路径
    pub target: ImportTarget,          // 导入目标
    pub alias: Option<String>,         // 别名
    pub is_wildcard: bool,             // 通配符导入
    pub is_default: bool,              // 默认导入
    pub is_system_header: bool,        // 系统头文件
    pub is_relative: bool,             // 相对路径
    pub span: Option<Span>,
}

pub struct StandardizedExport {
    pub kind: ExportKind,              // 导出类型
    pub target: ExportTarget,          // 导出目标
    pub is_reexport: bool,             // 重导出
    pub span: Option<Span>,
}
```

## 4. 关系域划分

```rust
impl RelationType {
    pub fn domain(&self) -> &'static str {
        // call: DirectCall, InstanceMethodCall, StaticMethodCall, ...
        // dependency: ImportStandard, ImportDynamic, Use, Include, ...
        // structural: Inheritance, Implementation, TraitBound, ...
        // template: ElementContains, TemplateReference, EventCallback, ...
        // reference: TypeReference, FieldAccess, ...
    }
}
```

## 5. 错误处理流程

### 5.1 解析错误

- 语法错误 → 跳过该实体/关系提取
- 不支持的语法 → 记录 warning，继续处理
- 文件编码问题 → 尝试自动检测编码，失败则跳过

### 5.2 符号解析错误

- 符号未找到 → 标记为 Unresolved，保留名称字符串
- 包解析失败 → 记录到错误列表，继续处理
- 循环依赖 → 检测并中断，记录 warning

### 5.3 查询错误

- 索引不存在 → 返回空结果
- 查询超时 → 返回已获取的部分结果
- 实体不存在 → 返回空关系列表

## 6. 性能优化

### 6.1 标准库过滤

```rust
// 可选过滤: 配置 filter_stdlib_calls = true 时
// 所有标准库调用不会被解析为实体关系
// 减少索引大小，聚焦项目内部关系
```

### 6.2 去重优化

- IndexBuilder 内部维护去重集合
- 同一 caller → callee 对只保留一条
- 同文件内的重复关系自动合并

### 6.3 批量处理

- 解析阶段: 批量处理文件组
- 索引构建: 逐文件添加解析结果
- 线程安全: DashMap 无锁并发读写

### 6.4 内存优化

- ImportTable 和 ExportTable 存储标准化后的轻量结构
- Entity 引用使用 EntityId (u64) 而非指针
- 关系索引内部使用 Arc 减少克隆

## 7. 相关源文件

| 文件 | 作用 |
|------|------|
| `cce_core/src/types/relation.rs` | 关系核心类型定义 |
| `cce_core/src/types/import.rs` | 导入/导出类型定义 |
| `cce_parser/src/parser/extractor/relation_extractor.rs` | 关系提取器 |
| `cce_parser/src/relation/` | 关系索引构建模块 |
| `cce_orchestrator/src/query/relation_searcher.rs` | 关系查询 |
| `cce_orchestrator/src/query/relation_bridge.rs` | 关系桥接 |
| `cce_orchestrator/src/query/boost/relation.rs` | 关系增强 |
| `cce_orchestrator/src/query/assembly/` | SPSR-Graph 组装 |
| `cce_orchestrator/src/tools/symbol_lookup/` | 符号查找工具 |
