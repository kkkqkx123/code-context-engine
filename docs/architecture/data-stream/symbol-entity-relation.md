# Symbol 与 Entity 关系分析

## 概述

本系统中有两个核心概念：**Entity**（语义实体）和 **Symbol**（符号）。Entity 是跨语言统一的语义抽象，而 Symbol 是语言级别的代码符号。两者服务于不同的目的，但在关系索引构建过程中存在转换映射。

## 核心概念

### Entity（语义实体）

Entity 是系统的核心抽象，代表源代码中的语义概念。

```rust
// cce_core/src/types/entity/full.rs
pub struct Entity {
    pub id: EntityId,                  // 文件局部 ID
    pub kind: EntityKind,              // 跨语言统一的类型
    pub name: String,                  // 实体名称
    pub signature: String,             // 签名
    pub parameters: Vec<(String, Option<String>)>,
    pub return_type: Option<String>,
    pub span: Span,
    pub depth: usize,
    pub parent: Option<EntityId>,
    pub children: Vec<EntityId>,
    pub doc_comment: Option<String>,
    pub modifiers: Vec<String>,
    pub attributes: HashMap<String, String>,
    pub metadata: HashMap<String, String>,
    pub is_stdlib: bool,
    pub stdlib_category: Option<StdlibCategory>,
}
```

**特点**：
- 从 AST 提取后不再依赖原始树
- 跨语言统一 (Python `def` / Rust `fn` → `EntityKind::Function`)
- 自包含：一次提取包含所有下游所需信息

### Symbol（符号）

Symbol 是关系索引 (RelationIndex) 中使用的概念，用于跨文件的符号解析和引用追踪。

```
四层符号表架构（均在 RelationIndex 内部）:
  ├── LocalSymbolTable:  文件局部符号 (HashMap<EntityId, Entity>)
  ├── ImportTable:       文件级标准化导入
  ├── ExportTable:       文件级标准化导出
  └── FileIndex:         文件元数据索引
```

符号层面的核心类型：

```rust
// cce_core/src/types/import.rs
pub struct StandardizedImport {
    pub kind: ImportKind,              // ImportDefault / ImportNamed / ImportAll / ...
    pub source: String,                // 导入源
    pub target: ImportTarget,          // 导入目标 { local_name, kind }
    pub alias: Option<String>,
    pub is_wildcard: bool,
    pub is_default: bool,
    pub is_system_header: bool,        // C/C++ 系统头文件
    pub is_relative: bool,
    pub span: Option<Span>,
}

pub struct StandardizedExport {
    pub kind: ExportKind,              // ExportNamed / ExportDefault / ExportAll
    pub target: ExportTarget,          // { name, kind, source_module }
    pub is_reexport: bool,
    pub span: Option<Span>,
}
```

## Entity → Symbol 的映射

### 映射点1：ParsedFile → IndexBuilder

```
ParsedFile
  ├── entities (Vec<Entity>)
  ├── local_symbols (HashMap<String, Vec<EntityId>>)
  ├── raw_relations (Vec<RawRelationData>)
  ├── import_table (Option<ImportTable>)
  └── file_doc_comment (Option<String>)
        │
        ▼
IndexBuilder (cce_parser::relation::IndexBuilder)
  ├── Entity → 函数索引 (function_index)
  ├── Entity → 文件映射 (entity_file_index)
  ├── RawRelationData → ResolvedRelation (通过跨文件解析)
  ├── 推导 Imports → StandardizedImport (来自 import_table 缓存或 raw_relations 派生)
  └── 推导 Exports → StandardizedExport
```

> **注**：由于解析器修复，Imports 在 AST 解析期间已预缓存到 `import_table`，避免 IndexBuilder 重复解析 raw_relations。

### 映射点2：Entity → SymbolMetadata (导出处理)

```rust
// index/builder.rs
fn process_exports(&self, file: &ParsedFile, exports: &mut Vec<StandardizedExport>) {
    for entity in &file.entities {
        // 判断可见性 (public/private)
        let visibility = infer_visibility(entity, &file.language);

        // 创建导出目标
        let target = ExportTarget::new(&entity.name, entity.kind.into())
            .with_source_module(file.path.clone());

        // 创建标准化导出
        let export = StandardizedExport::new(
            ExportKind::ExportNamed,
            target,
        );

        exports.push(export);
    }
}
```

### 映射点3：RawRelationData → StandardizedImport

```rust
// index/builder.rs
fn process_imports(&self, file: &ParsedFile, imports: &mut StandardizedImportTable) {
    for relation in &file.raw_relations {
        if relation.relation_type.is_import() {
            // 解析导入路径
            let import_info = self.resolve_import_path(&relation.dst_name, &file.path);

            // 创建标准化导入
            let import = StandardizedImport::new(
                import_info.kind,
                import_info.source,
            )
            .with_target(ImportTarget::new(
                import_info.local_name,
                import_info.target_kind,
            ));

            imports.add_import(import);
        }
    }
}
```

## 数据流

```
               AST Parser (tree-sitter)
                     │
                     ▼
            EntityExtractor
            (cce_parser::parser::extractor)
                     │
                     ▼
          ┌─────────────────────┐
          │      Entity         │  ↔ 跨语言统一的语义抽象
          │  (kind, signature,  │     查询时使用 Entity 信息
          │   doc_comment, ...) │
          └─────────┬───────────┘
                    │
                    ▼
          ┌─────────────────────┐
          │    ParsedFile       │
          │  - entities         │
          │  - local_symbols    │
          │  - raw_relations    │
          │  - embedded_blocks  │
          └─────────┬───────────┘
                    │
         ┌─────────┴──────────┐
         ▼                    ▼
  ┌──────────────┐   ┌──────────────┐
  │  FileProcessor│   │ IndexBuilder │
  │  (cce_orch..) │   │ (cce_parser) │
  │  - 分组      │   │ - 解析 imports│
  │  - NL 转换   │   │ - 解析 exports│
  │  - 分块      │   │ - 构建关系索引│
  │  → 存储实体  │   │ - 符号解析    │
  └──────────────┘   └──────┬───────┘
                            │
                            ▼
                  ┌─────────────────────┐
                  │   RelationIndex     │
                  │  - function_index   │ ← Entity 元数据
                  │  - import_index     │ ← 标准化导入
                  │  - export_index     │ ← 标准化导出
                  │  - file_index       │ ← 文件元数据
                  │  - resolved_relations│ ← 解析后的关系
                  └─────────────────────┘
```

## Entity 和 Symbol 的数据重复分析

### 重复的字段

| 字段 | Entity | Symbol/Import/Export |
|------|--------|---------------------|
| name | Entity::name | ImportTarget::local_name / ExportTarget::name |
| kind | Entity::kind | ImportTarget::kind / TargetKind |
| file path | ParsedFile::path | FileInfo::path / ImportTable::file_id |
| span | Entity::span | 部分 Import/Export 也含 span |

### 设计意图：为何允许重复

#### 1. 职责分离

- **Entity**：供存储 (Qdrant/SQLite)、查询和展示使用，是"语义层"概念
- **Symbol/Import/Export**：供关系索引和跨文件解析使用，是"符号层"概念

#### 2. 不同的生命周期

- **Entity**：解析后立即存在，可独立存储和查询
- **RelationIndex**：在整个索引完成后才完整，支持跨文件的符号解析

#### 3. 不同的用途

- Entity 用于：语义搜索、代码理解、NL 生成
- Symbol 用于：跳转到定义、查找引用、调用链分析

#### 4. 避免重复的设计

- **EntityId** 在两层间保持一致 (u64)，是连接 Entity 和 Symbol 的桥梁
- 实体级别的数据 (signature, parameters, doc_comment) 仅 Entity 持有
- SymbolMetadata 只存储 Entity 的子集 (name, kind, location)

## 架构优势

### 1. 清晰的职责划分

Entity 系统专注于"代码是什么"，Symbol 系统专注于"代码在哪里被引用"。两者互不干扰。

### 2. 灵活的扩展性

新增 EntityKind 只需修改 `cce_core/src/types/entity/kind.rs`，不影响符号解析逻辑。新增关系类型只需扩展 RelationType，不影响 Entity 本身。

### 3. 高效的查询性能

Entity 查询走 Qdrant 向量检索 + BM25 全文检索；Symbol 查询走 RelationIndex (DashMap 内存索引)，两者独立优化。

### 4. 完整的类型安全

EntityKind 和 RelationType 都是 Rust 枚举，所有 match 都强制穷尽，新增变体时编译器会提示所有需要修改的位置。

## 相关源文件

| 文件 | 作用 |
|------|------|
| `cce_core/src/types/entity/full.rs` | Entity 结构体 |
| `cce_core/src/types/entity/file.rs` | ParsedFile |
| `cce_core/src/types/relation.rs` | Relation 类型 |
| `cce_core/src/types/import.rs` | 标准化导入/导出类型 |
| `cce_parser/src/parser/extractor/entity_extractor.rs` | 实体提取器 |
| `cce_parser/src/relation/` | 关系索引构建 |
| `cce_orchestrator/src/index/file_processor.rs` | 文件处理管道 |
| `cce_orchestrator/src/tools/symbol_lookup/` | 符号查找工具 |
