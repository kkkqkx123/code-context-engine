# 关系增强功能设计

## 1. 功能概述

### 1.1 目标

为导出的自然语言文档添加代码关系信息，使文档不仅包含实体的自然语言描述，还包含实体之间的调用关系、依赖关系等，增强文档的语义完整性。

### 1.2 核心价值

- **关系可视化**：在文档中展示函数调用关系、类型依赖关系
- **上下文增强**：帮助读者理解实体在整个系统中的位置
- **导航辅助**：通过关系链接快速跳转到相关实体

### 1.3 关系类型

从现有的 `RelationIndex` 中提取以下关系：

1. **调用关系（Call）**：函数 A 调用函数 B
2. **被调用关系（CalledBy）**：函数 B 被函数 A 调用
3. **类型依赖（TypeDependency）**：函数/类使用某类型
4. **继承关系（Inheritance）**：类 A 继承类 B
5. **实现关系（Implementation）**：类 A 实现接口 B

## 2. 现有关系架构

### 2.1 RelationIndex 结构

```rust
// 现有的关系索引结构
pub struct RelationIndex {
    // 函数索引：函数名 → 函数信息列表
    function_index: HashMap<String, Vec<FunctionInfo>>,
    
    // 调用索引：调用者 ID → 被调用者列表
    call_index: HashMap<String, Vec<CallRelation>>,
    
    // 导入索引：文件 ID → 导入表
    import_index: HashMap<String, ImportTable>,
    
    // 文件索引：文件 ID → 文件信息
    file_index: HashMap<String, FileInfo>,
    
    // 导出索引：文件 ID → 导出列表
    export_index: HashMap<String, Vec<ExportInfo>>,
    
    // 已解析关系索引：调用者 EntityId → 已解析关系
    resolved_relation_index: HashMap<EntityId, Vec<ResolvedRelation>>,
}
```

### 2.2 已解析关系

```rust
pub struct ResolvedRelation {
    pub caller_id: EntityId,
    pub callee_name: String,
    pub callee_file: Option<String>,
    pub callee_id: Option<EntityId>,
    pub relation_type: RelationType,
    pub span: Span,
}
```

## 3. 设计方案

### 3.1 关系增强器

```rust
// src/export/relation_enhancer.rs

/// 关系增强器
/// 
/// 从 RelationIndex 提取关系信息，增强 EntityNlDocument
pub struct RelationEnhancer {
    /// 关系索引（只读引用）
    relation_index: Arc<ThreadSafeIndex>,
    /// 配置
    config: RelationEnhancerConfig,
}

/// 关系增强配置
#[derive(Debug, Clone)]
pub struct RelationEnhancerConfig {
    /// 最大相关实体数量
    pub max_related_entities: usize,
    /// 包含的关系类型
    pub include_relation_types: Vec<RelationType>,
    /// 是否包含跨文件关系
    pub include_cross_file: bool,
    /// 是否包含标准库调用
    pub include_stdlib: bool,
}

impl Default for RelationEnhancerConfig {
    fn default() -> Self {
        Self {
            max_related_entities: 10,
            include_relation_types: vec![
                RelationType::Call,
                RelationType::CalledBy,
                RelationType::TypeDependency,
            ],
            include_cross_file: true,
            include_stdlib: false,
        }
    }
}
```

### 3.2 增强流程

```
┌─────────────────────────────────────────────────────────────────┐
│                      关系增强流程                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  FileNlDocument (原始)                                          │
│      │                                                          │
│      ▼                                                          │
│  ┌─────────────────────────┐                                    │
│  │  RelationEnhancer       │                                    │
│  │                         │                                    │
│  │  for each entity:       │                                    │
│  │    1. 查询 call_index   │                                    │
│  │    2. 查询 resolved_    │                                    │
│  │       relation_index    │                                    │
│  │    3. 过滤关系类型      │                                    │
│  │    4. 构建 RelatedEntity│                                    │
│  └───────────┬─────────────┘                                    │
│              │                                                  │
│              ▼                                                  │
│  FileNlDocument (增强后)                                         │
│  - entities[].related_entities 已填充                           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.3 核心实现

```rust
impl RelationEnhancer {
    /// 创建新增强器
    pub fn new(
        relation_index: Arc<ThreadSafeIndex>,
        config: RelationEnhancerConfig,
    ) -> Self {
        Self {
            relation_index,
            config,
        }
    }
    
    /// 增强文件文档
    pub fn enhance(&self, doc: &mut FileNlDocument) {
        for entity in &mut doc.entities {
            self.enhance_entity(entity, &doc.source_path);
        }
    }
    
    /// 增强单个实体
    fn enhance_entity(&self, entity: &mut EntityNlDocument, file_path: &str) {
        let mut related = Vec::new();
        
        // 1. 查询该实体的调用关系
        if let Some(relations) = self.query_entity_relations(&entity.name, file_path) {
            for relation in relations {
                // 过滤关系类型
                if !self.config.include_relation_types.contains(&relation.relation_type) {
                    continue;
                }
                
                // 过滤跨文件关系
                if !self.config.include_cross_file && relation.file_path.is_some() {
                    continue;
                }
                
                // 过滤标准库
                if !self.config.include_stdlib && self.is_stdlib(&relation.name) {
                    continue;
                }
                
                related.push(RelatedEntity {
                    name: relation.name,
                    relation_type: relation.relation_type,
                    file_path: relation.file_path,
                });
                
                // 限制数量
                if related.len() >= self.config.max_related_entities {
                    break;
                }
            }
        }
        
        entity.related_entities = related;
    }
    
    /// 查询实体的关系
    fn query_entity_relations(
        &self,
        entity_name: &str,
        file_path: &str,
    ) -> Option<Vec<RelationInfo>> {
        let index = self.relation_index.read();
        
        // 构建实体 ID（file_path + entity_name）
        let entity_id = format!("{}::{}", file_path, entity_name);
        
        // 查询已解析关系
        let mut relations = Vec::new();
        
        // 查询调用关系（Call）
        if let Some(calls) = index.get_calls(&entity_id) {
            for call in calls {
                relations.push(RelationInfo {
                    name: call.callee_name.clone(),
                    relation_type: RelationType::Call,
                    file_path: call.callee_file.clone(),
                });
            }
        }
        
        // 查询被调用关系（CalledBy）
        if let Some(callers) = index.get_callers(&entity_id) {
            for caller in callers {
                relations.push(RelationInfo {
                    name: caller.caller_name.clone(),
                    relation_type: RelationType::CalledBy,
                    file_path: Some(caller.caller_file.clone()),
                });
            }
        }
        
        Some(relations)
    }
    
    /// 判断是否为标准库
    fn is_stdlib(&self, name: &str) -> bool {
        // 简单判断：标准库通常没有路径前缀
        !name.contains("::") || name.starts_with("std::") || name.starts_with("core::")
    }
}
```

### 3.4 关系类型定义

```rust
/// 关系类型（用于文档展示）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    /// 调用关系：A calls B
    Call,
    /// 被调用关系：A is called by B
    CalledBy,
    /// 类型依赖：A uses type B
    TypeDependency,
    /// 继承关系：A extends B
    Inheritance,
    /// 实现关系：A implements B
    Implementation,
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationType::Call => write!(f, "calls"),
            RelationType::CalledBy => write!(f, "called by"),
            RelationType::TypeDependency => write!(f, "uses"),
            RelationType::Inheritance => write!(f, "extends"),
            RelationType::Implementation => write!(f, "implements"),
        }
    }
}
```

## 4. 输出格式增强

### 4.1 Markdown 格式变化

增强前：

```markdown
### Function: parse_with_language_info

**Location**: Line 100-150

Parses a file using pre-detected language information.
```

增强后：

```markdown
### Function: parse_with_language_info

**Location**: Line 100-150

Parses a file using pre-detected language information.

**Related**:
- `parse` (calls) - `src/parser/mod.rs`
- `detect_language` (calls) - `src/parser/language_detector.rs`
- `index_file` (called by) - `src/orchestrator/index.rs`
```

### 4.2 关系分组展示

对于关系较多的情况，可以分组展示：

```markdown
**Related**:

*Calls*:
- `parse` - `src/parser/mod.rs`
- `detect_language` - `src/parser/language_detector.rs`

*Called By*:
- `index_file` - `src/orchestrator/index.rs`
- `process_batch` - `src/orchestrator/index.rs`

*Uses*:
- `ParsedFile` - `src/types/entity.rs`
- `LanguageInfo` - `src/types/language.rs`
```

## 5. 性能考虑

### 5.1 查询优化

- 使用索引的 O(1) 查询
- 批量查询减少锁竞争
- 结果缓存避免重复查询

### 5.2 内存优化

- 按需加载关系数据
- 限制最大关系数量
- 使用 Arc 共享索引

## 6. 配置选项

```toml
[export.relation_enhancement]
# 是否启用关系增强
enabled = true

# 最大相关实体数量
max_related_entities = 10

# 包含的关系类型
include_relation_types = ["call", "called_by", "type_dependency"]

# 是否包含跨文件关系
include_cross_file = true

# 是否包含标准库调用
include_stdlib = false
```

## 7. 与导出流程集成

```rust
impl NlDocumentExporter {
    pub async fn export_file(
        &self,
        chunks: &[ChunkedResult],
        summary: Option<&FileSummary>,
    ) -> Result<PathBuf, ExportError> {
        // 1. 聚合
        let mut doc = self.aggregator.aggregate(chunks, summary);
        
        // 2. 关系增强（如果启用）
        if self.config.enable_relation_enhancement {
            if let Some(ref enhancer) = self.relation_enhancer {
                enhancer.enhance(&mut doc);
            }
        }
        
        // 3. 格式化
        let content = self.formatter.format(&doc)?;
        
        // 4. 写入
        self.write_document(&doc).await
    }
}
```

## 8. 测试策略

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_relation_enhancer_filters_by_type() {
        // 测试按关系类型过滤
    }
    
    #[test]
    fn test_relation_enhancer_limits_count() {
        // 测试数量限制
    }
    
    #[test]
    fn test_relation_enhancer_cross_file() {
        // 测试跨文件关系处理
    }
}
```

## 9. 总结

关系增强功能作为可选功能，为导出的自然语言文档添加代码关系信息：

1. **可选启用**：通过配置控制是否启用
2. **灵活配置**：可配置关系类型、数量限制等
3. **性能优化**：利用现有索引，O(1) 查询
4. **格式增强**：在 Markdown 中清晰展示关系

---

**文档版本**：1.0
**创建日期**：2026-04-30
**维护者**：架构团队
