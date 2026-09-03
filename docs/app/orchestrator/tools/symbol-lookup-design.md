# Symbol Lookup

## 概述

本文档描述如何使用项目内部实现来替代 LSP (Language Server Protocol) 功能，提供类似 `find_references`、`get_symbols` 和 `goto_definition` 的能力。

## 背景

参考实现（VSCode LSP 工具）依赖外部语言服务器，存在以下限制：
- 需要启动和维护 LSP 服务器进程
- 依赖 VSCode 环境和语言服务器配置
- 无法利用项目已有的索引数据

本项目已经具备完整的代码分析能力：
- **AST 解析**：使用 tree-sitter 解析多种语言
- **实体提取**：提取函数、类、变量等语义实体
- **关系索引**：构建调用关系、继承关系等
- **符号表**：四级符号表架构支持符号解析

## 核心设计思路

### 1. 架构对比

#### LSP 方式（参考实现）
```
用户请求 → VSCode Extension → LSP Client → LSP Server → 语言分析 → 返回结果
```

#### 内部实现方式（本方案）
```
用户请求 → Tool API → RelationIndex/SymbolTable → 返回结果
                ↓
          (可选) 实时 AST 解析
```

### 2. 功能映射

| LSP 功能 | 内部实现 | 数据源 |
|---------|---------|--------|
| find_references | `get_callers_by_entity` | RelationIndex.callee_index |
| get_symbols | `get_file_entities` | RelationIndex.function_index |
| goto_definition | `resolve_symbol_location` | SymbolTable + EntityFileIndex |

### 3. 核心优势

1. **零外部依赖**：无需启动 LSP 服务器
2. **利用现有索引**：复用已构建的 RelationIndex 和 SymbolTable
3. **跨语言统一**：基于 EntityKind 的统一抽象
4. **高性能**：内存索引，O(1) 或 O(log n) 查询复杂度
5. **可扩展**：支持实时解析补充索引

## 详细设计

### 一、find_references 实现

#### 1.1 功能说明

查找符号的所有引用位置，包括：
- 函数调用位置
- 变量使用位置
- 类型引用位置

#### 1.2 实现策略

```rust
pub struct FindReferencesTool {
    index: Arc<RelationIndex>,
    symbol_table: Arc<ProjectSymbolTable>,
}

impl FindReferencesTool {
    /// 查找符号引用
    pub fn find_references(
        &self,
        request: FindReferencesRequest,
    ) -> Result<FindReferencesResponse, ToolError> {
        // 1. 解析目标符号
        let symbol_id = self.resolve_symbol(&request)?;
        
        // 2. 从反向索引获取所有引用者
        let callers = self.index.get_callers_by_callee_entity(symbol_id);
        
        // 3. 获取引用位置详情
        let references = self.get_reference_locations(callers, symbol_id)?;
        
        // 4. 按文件分组
        let grouped = self.group_by_file(references);
        
        Ok(FindReferencesResponse {
            symbol: request.symbol,
            total_count: grouped.iter().map(|g| g.count).sum(),
            file_count: grouped.len(),
            references: grouped,
        })
    }
    
    /// 获取引用位置详情
    fn get_reference_locations(
        &self,
        callers: Vec<EntityId>,
        target_id: EntityId,
    ) -> Result<Vec<ReferenceLocation>, ToolError> {
        let mut locations = Vec::new();
        
        for caller_id in callers {
            // 获取调用者实体
            let caller = self.index.get_function_by_entity_id(caller_id)
                .ok_or(ToolError::EntityNotFound(caller_id))?;
            
            // 获取调用关系详情
            let relations = self.index.get_resolved_relations_by_caller(caller_id);
            if let Some(relations) = relations {
                for relation in relations.iter() {
                    if relation.callee_id == Some(target_id) {
                        locations.push(ReferenceLocation {
                            path: self.get_file_path(caller_id)?,
                            line: relation.span.start_position.row + 1,
                            column: relation.span.start_position.column + 1,
                            context: self.get_context_lines(caller_id, relation.span)?,
                        });
                    }
                }
            }
        }
        
        Ok(locations)
    }
}
```

#### 1.3 数据结构

```rust
/// 查找引用请求
pub struct FindReferencesRequest {
    /// 文件路径
    pub path: String,
    /// 行号（1-based）
    pub line: usize,
    /// 列号（1-based，可选）
    pub column: Option<usize>,
    /// 符号名称（可选，用于文档）
    pub symbol: Option<String>,
    /// 上下文行数
    pub context_lines: Option<usize>,
}

/// 引用位置
pub struct ReferenceLocation {
    /// 文件路径
    pub path: String,
    /// 行号（1-based）
    pub line: usize,
    /// 列号（1-based）
    pub column: usize,
    /// 上下文代码
    pub context: String,
}

/// 按文件分组的引用
pub struct GroupedReferences {
    /// 文件路径
    pub path: String,
    /// 引用数量
    pub count: usize,
    /// 引用列表
    pub references: Vec<ReferenceLocation>,
}

/// 查找引用响应
pub struct FindReferencesResponse {
    /// 符号名称
    pub symbol: Option<String>,
    /// 总引用数
    pub total_count: usize,
    /// 文件数
    pub file_count: usize,
    /// 按文件分组的引用
    pub references: Vec<GroupedReferences>,
}
```

#### 1.4 性能优化

- **反向索引**：使用 `callee_index` 实现 O(1) 查找
- **批量查询**：支持一次查询多个符号
- **缓存**：缓存文件内容和上下文提取结果

### 二、get_symbols 实现

#### 2.1 功能说明

获取文件中的所有符号（函数、类、变量等），支持：
- 层级结构（类包含方法）
- 符号类型和位置
- 批量查询多个文件

#### 2.2 实现策略

```rust
pub struct GetSymbolsTool {
    index: Arc<RelationIndex>,
    parser: Arc<AstParser>,
}

impl GetSymbolsTool {
    /// 获取文件符号
    pub fn get_symbols(
        &self,
        request: GetSymbolsRequest,
    ) -> Result<GetSymbolsResponse, ToolError> {
        let mut results = Vec::new();
        
        for path in &request.paths {
            let result = self.get_symbols_for_file(path)?;
            results.push(result);
        }
        
        Ok(GetSymbolsResponse {
            results,
            success_count: results.iter().filter(|r| r.success).count(),
            fail_count: results.iter().filter(|r| !r.success).count(),
        })
    }
    
    /// 获取单个文件的符号
    fn get_symbols_for_file(&self, path: &str) -> Result<FileSymbolResult, ToolError> {
        // 1. 从索引获取已解析的实体
        let entities = self.index.get_entities_by_file(path);
        
        if !entities.is_empty() {
            // 使用索引数据
            return self.build_symbol_tree_from_entities(entities);
        }
        
        // 2. 如果索引中没有，实时解析
        self.parse_and_extract_symbols(path)
    }
    
    /// 从实体构建符号树
    fn build_symbol_tree_from_entities(
        &self,
        entities: Vec<Entity>,
    ) -> Result<FileSymbolResult, ToolError> {
        // 构建父子关系映射
        let mut children_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut root_entities = Vec::new();
        
        for entity in &entities {
            if let Some(parent_id) = entity.parent {
                children_map.entry(parent_id).or_default().push(entity.id);
            } else {
                root_entities.push(entity.id);
            }
        }
        
        // 递归构建符号树
        let symbols = root_entities.into_iter()
            .filter_map(|id| self.build_symbol_node(id, &entities, &children_map))
            .collect();
        
        Ok(FileSymbolResult {
            path: entities.first().map(|e| e.span.start_byte.to_string()).unwrap_or_default(),
            success: true,
            symbol_count: entities.len(),
            symbols,
        })
    }
    
    /// 构建符号节点
    fn build_symbol_node(
        &self,
        entity_id: EntityId,
        entities: &[Entity],
        children_map: &HashMap<EntityId, Vec<EntityId>>,
    ) -> Option<SymbolInfo> {
        let entity = entities.iter().find(|e| e.id == entity_id)?;
        
        let children = children_map.get(&entity_id)
            .map(|ids| ids.iter()
                .filter_map(|id| self.build_symbol_node(*id, entities, children_map))
                .collect())
            .unwrap_or_default();
        
        Some(SymbolInfo {
            name: entity.name.clone(),
            kind: entity.kind.to_symbol_kind(),
            line: entity.span.start_position.row + 1,
            end_line: entity.span.end_position.row + 1,
            detail: entity.signature.clone(),
            children: if children.is_empty() { None } else { Some(children) },
        })
    }
}
```

#### 2.3 数据结构

```rust
/// 获取符号请求
pub struct GetSymbolsRequest {
    /// 文件路径列表
    pub paths: Vec<String>,
}

/// 符号信息
pub struct SymbolInfo {
    /// 符号名称
    pub name: String,
    /// 符号类型
    pub kind: SymbolKind,
    /// 起始行号（1-based）
    pub line: usize,
    /// 结束行号（1-based）
    pub end_line: usize,
    /// 详细信息（签名）
    pub detail: Option<String>,
    /// 子符号
    pub children: Option<Vec<SymbolInfo>>,
}

/// 符号类型（映射到 LSP SymbolKind）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Method,
    Property,
    Field,
    Constructor,
    Enum,
    Interface,
    Function,
    Variable,
    Constant,
    Struct,
    EnumMember,
    // ... 其他类型
}

/// 文件符号结果
pub struct FileSymbolResult {
    /// 文件路径
    pub path: String,
    /// 是否成功
    pub success: bool,
    /// 符号数量
    pub symbol_count: Option<usize>,
    /// 符号列表
    pub symbols: Option<Vec<SymbolInfo>>,
    /// 错误信息
    pub error: Option<String>,
}

/// 获取符号响应
pub struct GetSymbolsResponse {
    /// 各文件结果
    pub results: Vec<FileSymbolResult>,
    /// 成功数量
    pub success_count: usize,
    /// 失败数量
    pub fail_count: usize,
}
```

#### 2.4 EntityKind 到 SymbolKind 映射

```rust
impl EntityKind {
    pub fn to_symbol_kind(&self) -> SymbolKind {
        match self {
            EntityKind::Function => SymbolKind::Function,
            EntityKind::Method => SymbolKind::Method,
            EntityKind::Class => SymbolKind::Class,
            EntityKind::Struct => SymbolKind::Struct,
            EntityKind::Interface => SymbolKind::Interface,
            EntityKind::Enum => SymbolKind::Enum,
            EntityKind::EnumMember => SymbolKind::EnumMember,
            EntityKind::Variable => SymbolKind::Variable,
            EntityKind::Constant => SymbolKind::Constant,
            EntityKind::Field => SymbolKind::Field,
            EntityKind::Property => SymbolKind::Property,
            EntityKind::Constructor => SymbolKind::Constructor,
            EntityKind::Module => SymbolKind::Module,
            EntityKind::Namespace => SymbolKind::Namespace,
            _ => SymbolKind::Variable,
        }
    }
}
```

### 三、goto_definition 实现

#### 3.1 功能说明

跳转到符号的定义位置，并返回完整定义代码：
- 函数定义
- 类/结构体定义
- 变量定义
- 类型定义

#### 3.2 实现策略

```rust
pub struct GotoDefinitionTool {
    index: Arc<RelationIndex>,
    symbol_table: Arc<ProjectSymbolTable>,
    file_cache: Arc<FileCache>,
}

impl GotoDefinitionTool {
    /// 跳转到定义
    pub fn goto_definition(
        &self,
        request: GotoDefinitionRequest,
    ) -> Result<GotoDefinitionResponse, ToolError> {
        // 1. 解析当前位置的符号
        let symbol_id = self.resolve_symbol_at_position(&request)?;
        
        // 2. 查找定义位置
        let definitions = self.find_definitions(symbol_id)?;
        
        // 3. 获取定义代码
        let definition_codes = definitions.into_iter()
            .map(|def| self.get_definition_code(def, request.include_body))
            .collect::<Result<Vec<_>, _>>()?;
        
        Ok(GotoDefinitionResponse {
            symbol: request.symbol,
            definitions: definition_codes,
        })
    }
    
    /// 解析位置处的符号
    fn resolve_symbol_at_position(
        &self,
        request: &GotoDefinitionRequest,
    ) -> Result<EntityId, ToolError> {
        // 1. 获取文件实体
        let entities = self.index.get_entities_by_file(&request.path);
        
        // 2. 查找包含该位置的实体
        let target_entity = entities.into_iter()
            .find(|e| {
                e.span.start_position.row + 1 <= request.line &&
                e.span.end_position.row + 1 >= request.line
            })
            .ok_or(ToolError::NoSymbolAtPosition)?;
        
        // 3. 如果指定了列号，进一步精确匹配
        if let Some(column) = request.column {
            return self.find_symbol_at_column(target_entity, column);
        }
        
        Ok(target_entity.id)
    }
    
    /// 查找定义位置
    fn find_definitions(&self, symbol_id: EntityId) -> Result<Vec<DefinitionLocation>, ToolError> {
        let mut definitions = Vec::new();
        
        // 1. 检查是否是定义本身
        if let Some(entity) = self.index.get_function_by_entity_id(symbol_id) {
            let file_path = self.index.get_file_path_by_entity(symbol_id)
                .ok_or(ToolError::FileNotFound)?;
            
            definitions.push(DefinitionLocation {
                path: file_path,
                entity_id: symbol_id,
                span: entity.span.clone(),
            });
        }
        
        // 2. 检查符号表中的其他定义（如接口实现）
        if let Ok(other_defs) = self.find_related_definitions(symbol_id) {
            definitions.extend(other_defs);
        }
        
        Ok(definitions)
    }
    
    /// 获取定义代码
    fn get_definition_code(
        &self,
        location: DefinitionLocation,
        include_body: bool,
    ) -> Result<DefinitionCode, ToolError> {
        // 1. 读取文件内容
        let content = self.file_cache.get_content(&location.path)?;
        
        // 2. 提取定义代码
        let entity = self.index.get_function_by_entity_id(location.entity_id)
            .ok_or(ToolError::EntityNotFound(location.entity_id))?;
        
        let code = if include_body {
            // 返回完整定义（包括函数体）
            self.extract_full_definition(&content, &entity)?
        } else {
            // 只返回签名
            entity.signature.clone()
        };
        
        Ok(DefinitionCode {
            path: location.path,
            line: entity.span.start_position.row + 1,
            end_line: entity.span.end_position.row + 1,
            content: code,
            line_count: entity.span.end_position.row - entity.span.start_position.row + 1,
        })
    }
    
    /// 提取完整定义（包括函数体）
    fn extract_full_definition(
        &self,
        content: &str,
        entity: &Entity,
    ) -> Result<String, ToolError> {
        let lines: Vec<&str> = content.lines().collect();
        let start_line = entity.span.start_position.row;
        let end_line = entity.span.end_position.row;
        
        // 如果范围太小，尝试扩展到完整代码块
        let actual_end = if end_line - start_line < 2 {
            self.find_block_end(&lines, start_line)?
        } else {
            end_line
        };
        
        // 提取代码并添加行号
        let code = (start_line..=actual_end)
            .map(|i| {
                let line_num = i + 1;
                let line_text = lines.get(i).unwrap_or(&"");
                format!("{:4} | {}", line_num, line_text)
            })
            .collect::<Vec<_>>()
            .join("\n");
        
        Ok(code)
    }
    
    /// 查找代码块结束位置（括号匹配）
    fn find_block_end(&self, lines: &[&str], start_line: usize) -> Result<usize, ToolError> {
        let mut brace_count = 0;
        let mut found_open = false;
        
        for (i, line) in lines.iter().enumerate().skip(start_line) {
            for ch in line.chars() {
                match ch {
                    '{' => {
                        brace_count += 1;
                        found_open = true;
                    }
                    '}' => {
                        brace_count -= 1;
                        if found_open && brace_count == 0 {
                            return Ok(i);
                        }
                    }
                    _ => {}
                }
            }
            
            // 防止无限循环
            if i - start_line > 500 {
                break;
            }
        }
        
        Ok(start_line)
    }
}
```

#### 3.3 数据结构

```rust
/// 跳转到定义请求
pub struct GotoDefinitionRequest {
    /// 文件路径
    pub path: String,
    /// 行号（1-based）
    pub line: usize,
    /// 列号（1-based，可选）
    pub column: Option<usize>,
    /// 符号名称（可选）
    pub symbol: Option<String>,
    /// 是否包含函数体
    pub include_body: Option<bool>,
}

/// 定义位置
pub struct DefinitionLocation {
    /// 文件路径
    pub path: String,
    /// 实体 ID
    pub entity_id: EntityId,
    /// 源码范围
    pub span: Span,
}

/// 定义代码
pub struct DefinitionCode {
    /// 文件路径
    pub path: String,
    /// 起始行号（1-based）
    pub line: usize,
    /// 结束行号（1-based）
    pub end_line: usize,
    /// 代码内容
    pub content: String,
    /// 行数
    pub line_count: usize,
}

/// 跳转到定义响应
pub struct GotoDefinitionResponse {
    /// 符号名称
    pub symbol: Option<String>,
    /// 定义列表
    pub definitions: Vec<DefinitionCode>,
}
```

## 实现路线图

### Phase 1: 基础实现（优先级：高）

1. **FindReferencesTool**
   - 实现 `resolve_symbol` 符号解析
   - 实现 `get_reference_locations` 引用位置获取
   - 实现 `group_by_file` 文件分组
   - 添加单元测试

2. **GetSymbolsTool**
   - 实现 `get_symbols_for_file` 文件符号获取
   - 实现 `build_symbol_tree_from_entities` 符号树构建
   - 实现 EntityKind 到 SymbolKind 映射
   - 添加单元测试

3. **GotoDefinitionTool**
   - 实现 `resolve_symbol_at_position` 位置符号解析
   - 实现 `find_definitions` 定义查找
   - 实现 `get_definition_code` 代码提取
   - 添加单元测试

### Phase 2: 增强功能（优先级：中）

1. **实时解析支持**
   - 当索引中没有数据时，实时解析文件
   - 缓存解析结果

2. **上下文提取**
   - 提取引用位置的上下文代码
   - 支持可配置的上下文行数

3. **批量查询优化**
   - 支持批量查询多个符号
   - 减少重复的文件读取

### Phase 3: 高级功能（优先级：低）

1. **跨文件符号解析**
   - 利用 ImportTable 解析导入符号
   - 支持跨文件跳转

2. **继承关系支持**
   - 查找接口实现
   - 查找基类/派生类

3. **模糊匹配**
   - 支持符号名称模糊搜索
   - 支持正则表达式匹配

## 与 LSP 的对比

| 特性 | LSP | 内部实现 | 说明 |
|-----|-----|---------|------|
| 启动时间 | 慢（需启动服务器） | 快（内存索引） | 内部实现无需启动外部进程 |
| 内存占用 | 高（独立进程） | 低（共享索引） | 复用已有索引数据 |
| 准确性 | 高（语言特定） | 中（基于 AST） | LSP 有更精确的类型信息 |
| 跨语言支持 | 需多个服务器 | 统一实现 | 内部实现支持所有已解析语言 |
| 实时性 | 高（增量更新） | 中（依赖索引更新） | LSP 有更好的增量支持 |
| 离线支持 | 否 | 是 | 内部实现可离线工作 |
| 可扩展性 | 高 | 中 | LSP 有丰富的生态系统 |

## 使用场景建议

### 适合使用内部实现的场景

1. **快速查询**：需要快速查找引用或定义，不需要完整的类型信息
2. **离线环境**：无法启动 LSP 服务器的环境
3. **批量分析**：需要分析大量文件的场景
4. **CI/CD**：在持续集成环境中使用
5. **轻量级编辑器**：不支持 LSP 的编辑器

### 适合使用 LSP 的场景

1. **精确重构**：需要精确的类型信息和重构能力
2. **实时编辑**：需要实时的错误诊断和补全
3. **复杂项目**：有复杂的构建系统和依赖关系
4. **语言特性**：需要语言特定的高级特性（如宏展开）

## 总结

本设计方案充分利用项目现有的索引能力，提供轻量级、高性能的 LSP 替代方案。通过 RelationIndex 和 SymbolTable 的组合，可以实现大部分常用的代码导航功能，同时保持零外部依赖和跨语言统一的优势。

建议优先实现 Phase 1 的基础功能，满足最常见的使用场景。后续根据实际需求逐步增强功能。
