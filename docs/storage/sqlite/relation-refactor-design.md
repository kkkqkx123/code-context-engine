# 关系存储重构设计文档

## 一、当前支持的关系类型

项目定义了 **33 种关系类型**，分为 5 个 Domain：

### 1. Call Domain（12种）
- `DirectCall` - 直接函数调用
- `InstanceMethodCall` - 实例方法调用
- `StaticMethodCall` - 静态方法调用
- `ChainedMethodCall` - 链式方法调用
- `ConstructorCall` - 构造函数调用
- `PointerCall` - 指针调用
- `CallbackCall` - 回调调用
- `GenericCall` - 泛型/模板调用
- `MacroCall` - 宏调用
- `GoroutineCall` - 协程调用
- `DeferredCall` - 延迟调用
- `AsyncCall` - 异步调用

### 2. Dependency Domain（11种）
- `IncludeSystem` - 系统头文件包含
- `IncludeLocal` - 本地头文件包含
- `ImportStandard` - 标准导入
- `ImportNamed` - 命名导入
- `ImportDefault` - 默认导入
- `ImportNamespace` - 命名空间导入
- `ImportDynamic` - 动态导入
- `Use` - Use 语句
- `Using` - Using 命名空间
- `MacroDependency` - 宏依赖
- `ModuleDependency` - 模块依赖

### 3. Structural Domain（4种）
- `Inheritance` - 继承关系
- `Implementation` - 实现接口
- `TraitBound` - Trait 约束
- `Contains` - 包含关系

### 4. Reference Domain（2种）
- `TypeReference` - 类型引用
- `FieldAccess` - 字段访问

### 5. Template Domain（4种）
- `ElementContains` - 元素包含
- `TemplateReference` - 模板引用
- `ParameterBinding` - 参数绑定
- `EventCallback` - 事件回调

## 二、当前存储架构分析

### 2.1 现有表结构

```sql
-- 1. relations 表（实际使用）
CREATE TABLE relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    caller_id INTEGER NOT NULL,
    callee_id INTEGER,
    callee_name TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    is_external INTEGER NOT NULL DEFAULT 0,
    span_start_row INTEGER,
    span_end_row INTEGER,
    file_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (caller_id) REFERENCES entities(id) ON DELETE CASCADE,
    FOREIGN KEY (callee_id) REFERENCES entities(id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE INDEX idx_relations_caller ON relations(caller_id);
CREATE INDEX idx_relations_callee ON relations(callee_id);
CREATE INDEX idx_relations_file ON relations(file_id);
CREATE INDEX idx_relations_type ON relations(relation_type);

-- 2. imports 表（已定义但未使用）
CREATE TABLE imports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    source TEXT NOT NULL,
    import_type TEXT NOT NULL,
    target_file_id INTEGER,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY (target_file_id) REFERENCES files(id) ON DELETE SET NULL
);

-- 3. exports 表（已定义但未使用）
CREATE TABLE exports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    function_id INTEGER,
    function_name TEXT NOT NULL,
    export_type TEXT NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);
```

### 2.2 使用情况分析

通过代码分析发现：
- `relations` 表：频繁使用，存储所有关系类型
- `imports` 表：表已创建，但 `ImportRepository` 方法从未被调用
- `exports` 表：表已创建，但 `ExportRepository` 方法从未被调用

**结论**：`imports` 和 `exports` 表是完全冗余的设计。

## 三、存在的问题

### 3.1 严重的冗余问题

**问题描述**：
- `imports` 和 `exports` 表完全未使用
- 所有 import/export 关系都通过 `RelationType` 存储在 `relations` 表中
- 代码库中存在 `import_repo.rs` 和 `export_repo.rs`，但没有任何调用

**影响**：
- 数据库空间浪费
- 代码维护成本增加
- 容易造成混淆

### 3.2 语义混淆问题

**问题描述**：文件级关系和实体级关系混在一起

| 关系类型 | 当前存储方式 | 应该的存储方式 |
|----------|--------------|----------------|
| `import "react"` | `caller_id = 实体ID` | `caller_id = 文件ID` |
| `function foo() {}` | `caller_id = 实体ID` | `caller_id = 实体ID` ✓ |
| `class A extends B` | `caller_id = 实体ID` | `caller_id = 实体ID` ✓ |

**当前实现的问题**：
```rust
// import 关系存储示例
Relation {
    src: EntityId(123),      // ← 问题：应该是 FileId(5)
    dst: RelationTarget::unresolved("react"),
    relation_type: RelationType::ImportStandard,
    span: Span { ... }
}
```

**问题分析**：
- import 语句属于文件级别，不属于任何实体
- 当前使用文件的第一个实体作为 caller，语义不清晰
- 查询"文件 A 导入了哪些模块"需要复杂的联表查询

### 3.3 查询性能问题

**当前查询方式**（复杂且低效）：
```rust
// 查询文件的导入
fn get_file_imports(conn: &Connection, file_id: i64) -> Result<Vec<Relation>> {
    // 1. 先找到文件的所有实体
    let entities = get_entities_by_file(conn, file_id)?;

    // 2. 对每个实体查询导入关系
    let mut imports = Vec::new();
    for entity in entities {
        let entity_imports = get_relations_by_caller(conn, entity.id)?
            .into_iter()
            .filter(|r| r.relation_type.is_dependency());
        imports.extend(entity_imports);
    }

    Ok(imports)
}
```

**理想查询方式**（简单且高效）：
```rust
// 直接查询
fn get_file_imports(conn: &Connection, file_id: i64) -> Result<Vec<Relation>> {
    get_relations_by_caller_and_level(conn, file_id, RelationLevel::File)
        .map(|relations| relations.into_iter()
            .filter(|r| r.relation_type.is_dependency())
            .collect())
}
```

## 四、重构方案

### 4.1 设计思路

将关系明确分为"文件级"和"实体级"：
- **文件级关系**：import、export、module dependency 等
- **实体级关系**：call、inheritance、implementation、reference 等

### 4.2 表结构优化

```sql
-- 优化后的 relations 表
CREATE TABLE relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    caller_level TEXT NOT NULL,          -- 'file' | 'entity'
    caller_id INTEGER NOT NULL,          -- 文件 ID 或实体 ID
    callee_id INTEGER,
    callee_name TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    is_external INTEGER NOT NULL DEFAULT 0,
    span_start_row INTEGER,
    span_end_row INTEGER,
    file_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (caller_id, caller_level) REFERENCES 
        (CASE caller_level 
            WHEN 'file' THEN files(id) 
            WHEN 'entity' THEN entities(id) 
        END) ON DELETE CASCADE,
    FOREIGN KEY (callee_id) REFERENCES entities(id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- 优化后的索引
CREATE INDEX idx_relations_caller ON relations(caller_level, caller_id);
CREATE INDEX idx_relations_callee ON relations(callee_id);
CREATE INDEX idx_relations_file ON relations(file_id);
CREATE INDEX idx_relations_type ON relations(relation_type);
CREATE INDEX idx_relations_level_type ON relations(caller_level, relation_type);
```

### 4.3 类型定义调整

```rust
// src/types/relation.rs

/// 关系级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationLevel {
    /// 文件级关系（import/export/module dependency）
    File,
    /// 实体级关系（call/inheritance/reference）
    Entity,
}

impl std::fmt::Display for RelationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationLevel::File => write!(f, "file"),
            RelationLevel::Entity => write!(f, "entity"),
        }
    }
}

impl std::str::FromStr for RelationLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "file" => Ok(RelationLevel::File),
            "entity" => Ok(RelationLevel::Entity),
            _ => Err(format!("Unknown RelationLevel: {}", s)),
        }
    }
}

/// 优化后的 Relation 结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    /// 关系级别
    pub caller_level: RelationLevel,

    /// 调用者 ID（文件 ID 或实体 ID）
    pub caller_id: i64,

    /// 目标
    pub dst: RelationTarget,

    /// 关系类型
    pub relation_type: RelationType,

    /// 代码位置
    pub span: Span,
}

impl Relation {
    /// 创建文件级关系
    pub fn file_relation(
        file_id: i64,
        dst: RelationTarget,
        relation_type: RelationType,
        span: Span,
    ) -> Self {
        Self {
            caller_level: RelationLevel::File,
            caller_id: file_id,
            dst,
            relation_type,
            span,
        }
    }

    /// 创建实体级关系
    pub fn entity_relation(
        entity_id: i64,
        dst: RelationTarget,
        relation_type: RelationType,
        span: Span,
    ) -> Self {
        Self {
            caller_level: RelationLevel::Entity,
            caller_id: entity_id,
            dst,
            relation_type,
            span,
        }
    }
}
```

### 4.4 存储类型调整

```rust
// src/storage/sqlite/types.rs

/// 优化后的关系记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationRecord {
    /// 唯一关系 ID
    pub id: i64,
    /// 关系级别
    pub caller_level: String,
    /// 调用者 ID
    pub caller_id: i64,
    /// 被调用者 ID
    pub callee_id: Option<i64>,
    /// 被调用者名称
    pub callee_name: String,
    /// 关系类型
    pub relation_type: String,
    /// 是否外部引用
    pub is_external: bool,
    /// 起始行
    pub span_start_row: Option<i64>,
    /// 结束行
    pub span_end_row: Option<i64>,
    /// 文件 ID
    pub file_id: i64,
    /// 创建时间
    pub created_at: i64,
    /// 更新时间
    pub updated_at: i64,
}
```

## 五、实施计划

### 5.1 第一阶段：清理冗余代码

**任务清单**：
1. 删除 `src/storage/sqlite/import_repo.rs`
2. 删除 `src/storage/sqlite/export_repo.rs`
3. 删除 `src/storage/sqlite/types.rs` 中的 `ImportRecord` 和 `ExportRecord`
4. 删除数据库表定义中的 `imports` 和 `exports` 表
5. 更新 `src/storage/sqlite/mod.rs` 移除相关导入

### 5.2 第二阶段：添加 RelationLevel

**任务清单**：
1. 在 `src/types/relation.rs` 中添加 `RelationLevel` 枚举
2. 更新 `Relation` 结构体，添加 `caller_level` 字段
3. 添加 `file_relation()` 和 `entity_relation()` 构造方法
4. 更新所有 `Relation` 创建代码，明确指定级别

### 5.3 第三阶段：数据库迁移

**SQL 迁移脚本**：
```sql
-- 1. 添加新字段
ALTER TABLE relations ADD COLUMN caller_level TEXT DEFAULT 'entity';

-- 2. 更新现有数据
UPDATE relations 
SET caller_level = 'file' 
WHERE relation_type IN (
    'dependency.include.system',
    'dependency.include.local',
    'dependency.import.standard',
    'dependency.import.named',
    'dependency.import.default',
    'dependency.import.namespace',
    'dependency.import.dynamic',
    'dependency.use',
    'dependency.using',
    'dependency.macro',
    'dependency.module'
);

-- 3. 更新索引
DROP INDEX IF EXISTS idx_relations_caller;
CREATE INDEX idx_relations_caller ON relations(caller_level, caller_id);
CREATE INDEX IF NOT EXISTS idx_relations_level_type ON relations(caller_level, relation_type);

-- 4. 删除未使用的表
DROP TABLE IF EXISTS imports;
DROP TABLE IF EXISTS exports;
```

### 5.4 第四阶段：更新 Repository

**更新 `src/storage/sqlite/relation_repo.rs`**：
```rust
impl RelationRepository {
    /// 按级别和 ID 获取关系
    pub fn get_by_caller_and_level(
        conn: &Connection,
        caller_id: i64,
        caller_level: &str,
    ) -> Result<Vec<RelationRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, caller_level, caller_id, callee_id, callee_name, relation_type, 
                        is_external, span_start_row, span_end_row, file_id, created_at, updated_at
                 FROM relations WHERE caller_id = ?1 AND caller_level = ?2",
            )
            .map_err(|e| StorageError::Query(format!("Failed to prepare: {}", e)))?;
        let relations = stmt
            .query_map(params![caller_id, caller_level], Self::from_row)
            .map_err(|e| StorageError::Query(format!("Failed to query: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::Query(format!("Failed to collect: {}", e)))?;
        Ok(relations)
    }

    /// 获取文件级关系
    pub fn get_file_relations(
        conn: &Connection,
        file_id: i64,
    ) -> Result<Vec<RelationRecord>, StorageError> {
        Self::get_by_caller_and_level(conn, file_id, "file")
    }

    /// 获取实体级关系
    pub fn get_entity_relations(
        conn: &Connection,
        entity_id: i64,
    ) -> Result<Vec<RelationRecord>, StorageError> {
        Self::get_by_caller_and_level(conn, entity_id, "entity")
    }

    /// 更新 from_row 方法
    fn from_row(row: &Row) -> Result<RelationRecord, rusqlite::Error> {
        Ok(RelationRecord {
            id: row.get(0)?,
            caller_level: row.get(1)?,
            caller_id: row.get(2)?,
            callee_id: row.get(3)?,
            callee_name: row.get(4)?,
            relation_type: row.get(5)?,
            is_external: row.get(6)?,
            span_start_row: row.get(7)?,
            span_end_row: row.get(8)?,
            file_id: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }
}
```

### 5.5 第五阶段：更新关系提取逻辑

**更新 `src/parser/extractor/relation_extractor.rs`**：
```rust
impl RelationExtractor {
    /// 提取文件级依赖关系
    fn extract_file_dependencies(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        file_id: i64,
    ) -> Result<Vec<Relation>, QueryError> {
        let matches = self.query_executor
            .execute_dependency_query(tree, source, language)?;

        let mut relations = Vec::new();
        for mat in &matches {
            if let Some(capture) = mat.get_capture("dependency") {
                let relation_type = self.determine_dependency_relation_type(&capture.name);
                let span = self.capture_to_span(&capture, source);

                // 使用 file_id 作为 caller_id
                relations.push(Relation::file_relation(
                    file_id,
                    RelationTarget::unresolved(capture.text.clone()),
                    relation_type,
                    span,
                ));
            }
        }

        Ok(relations)
    }
}
```

### 5.6 第六阶段：更新索引构建逻辑

**更新 `src/relation/index/builder.rs`**：
```rust
impl IndexBuilder {
    /// 构建反向索引
    pub fn build_reverse_index(&self) -> ReverseResolvedRelationIndex {
        let mut index = HashMap::new();

        for (caller_id, relations) in &self.resolved_relation_index {
            for relation in relations {
                if let Some(callee_id) = relation.callee_id {
                    let entry = ResolvedRelation {
                        caller: *caller_id,
                        callee_id,
                        relation_type: relation.relation_type,
                        caller_level: relation.caller_level,
                    };

                    index.entry(callee_id)
                        .or_insert_with(Vec::new)
                        .push(entry);
                }
            }
        }

        index
    }
}
```

## 六、验证和测试

### 6.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_level() {
        assert_eq!(format!("{}", RelationLevel::File), "file");
        assert_eq!(format!("{}", RelationLevel::Entity), "entity");

        assert_eq!(
            RelationLevel::from_str("file").unwrap(),
            RelationLevel::File
        );
        assert!(RelationLevel::from_str("unknown").is_err());
    }

    #[test]
    fn test_file_relation_creation() {
        let relation = Relation::file_relation(
            123,
            RelationTarget::unresolved("react".to_string()),
            RelationType::ImportStandard,
            Span::default(),
        );

        assert_eq!(relation.caller_level, RelationLevel::File);
        assert_eq!(relation.caller_id, 123);
        assert!(relation.relation_type.is_dependency());
    }

    #[test]
    fn test_entity_relation_creation() {
        let relation = Relation::entity_relation(
            456,
            RelationTarget::unresolved("foo".to_string()),
            RelationType::DirectCall,
            Span::default(),
        );

        assert_eq!(relation.caller_level, RelationLevel::Entity);
        assert_eq!(relation.caller_id, 456);
        assert!(relation.relation_type.is_call());
    }
}
```

### 6.2 集成测试

```rust
#[tokio::test]
async fn test_file_level_relations() {
    let client = SqliteClient::new_in_memory().await.unwrap();

    // 创建测试文件
    let file_id = client.create_file("test.ts", "TypeScript").await.unwrap();

    // 创建文件级导入关系
    let relation = Relation::file_relation(
        file_id,
        RelationTarget::unresolved("react".to_string()),
        RelationType::ImportNamed,
        Span::default(),
    );

    // 存储关系
    client.store_relation(&relation).await.unwrap();

    // 查询文件级关系
    let relations = client.get_file_relations(file_id).await.unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].caller_level, "file");
    assert_eq!(relations[0].relation_type, "dependency.import.named");
}
```

## 七、优势总结

### 7.1 语义清晰
- 明确区分文件级和实体级关系
- 避免混淆：import 的 caller 是文件，不是实体
- 数据模型更符合实际语义

### 7.2 查询高效
- 直接按 level 过滤，避免复杂的联表查询
- 索引优化：`caller_level + caller_id` 复合索引
- 减少 N+1 查询问题

### 7.3 避免冗余
- 删除未使用的 imports/exports 表
- 统一使用 relations 表存储所有关系
- 简化代码结构

### 7.4 保持灵活性
- 仍然支持所有 33 种关系类型
- 不影响现有查询逻辑
- 向后兼容（通过默认值）

## 八、风险评估

### 8.1 向后兼容性
- **风险**：现有数据需要迁移
- **缓解**：添加默认值，分阶段迁移
- **影响**：中

### 8.2 性能影响
- **风险**：表结构变更可能影响性能
- **缓解**：提前测试，优化索引
- **影响**：低

### 8.3 代码修改范围
- **风险**：需要修改多个文件
- **缓解**：分阶段实施，充分测试
- **影响**：中

## 九、时间估算

| 阶段 | 任务 | 预估时间 |
|------|------|----------|
| 1 | 清理冗余代码 | 2-3 小时 |
| 2 | 添加 RelationLevel | 1-2 小时 |
| 3 | 数据库迁移 | 1 小时 |
| 4 | 更新 Repository | 2-3 小时 |
| 5 | 更新提取逻辑 | 3-4 小时 |
| 6 | 更新索引逻辑 | 2-3 小时 |
| 7 | 编写测试 | 4-5 小时 |
| 8 | 集成测试 | 2-3 小时 |
| **总计** | | **17-24 小时** |

## 十、后续优化建议

### 10.1 分表存储（可选）
如果数据量很大，可以考虑将文件级和实体级关系分表存储：
- `file_relations` 表：存储文件级关系
- `entity_relations` 表：存储实体级关系

**优势**：
- 更好的查询性能
- 更清晰的物理隔离

**劣势**：
- 增加复杂度
- 需要跨表查询某些场景

### 10.2 缓存优化
对频繁查询的关系进行缓存：
- 文件导入列表
- 热门函数的调用关系

### 10.3 查询优化
添加更多复合索引：
- `(caller_level, relation_type, caller_id)`
- `(file_id, caller_level, relation_type)`

## 十一、参考文档

- [AGENTS.md](../../../AGENTS.md) - 项目开发指南
- [函数调用关系与倒排索引设计.md](../../函数调用关系与倒排索引设计.md) - 关系设计文档
- [relation 模块与重索引分析.md](../../architecture/Relation模块与重索引分析.md) - 关系模块分析

---

**文档版本**: 1.0
**创建日期**: 2026-03-31
**作者**: iFlow CLI
**审核状态**: 待审核