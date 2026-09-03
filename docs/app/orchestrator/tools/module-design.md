# LSP 替代工具模块设计

## 模块结构设计

### 目录结构

```
src/orchestrator/query/tools/
├── mod.rs                    # 模块导出
├── compression/              # 已有：语义压缩工具
├── ast_diagnosis/            # 已有：AST 诊断工具
└── symbol_lookup/            # 新增：符号查找工具
    ├── mod.rs                # 模块导出
    ├── find_references.rs    # 查找引用
    ├── get_symbols.rs        # 获取符号
    ├── goto_definition.rs    # 跳转定义
    ├── types.rs              # 公共类型定义
    └── utils.rs              # 工具函数
```

## 核心复用策略

### 1. 复用 RelationIndex（关系索引）

**已有能力**：
- `function_index: DashMap<EntityId, Entity>` - 实体索引
- `entity_file_index: DashMap<EntityId, String>` - 实体文件映射
- `resolved_relation_index: DashMap<EntityId, Vec<ResolvedRelation>>` - 调用关系
- `callee_index: DashMap<EntityId, Vec<EntityId>>` - 反向索引（关键）

**复用方式**：
```rust
pub struct SymbolLookupTool {
    /// 关系索引（核心数据源）
    index: Arc<RelationIndex>,
    /// 可选：符号表（用于跨文件解析）
    symbol_table: Option<Arc<ProjectSymbolTable>>,
    /// 可选：文件缓存（用于读取源码）
    file_cache: Option<Arc<FileContentCache>>,
}
```

### 2. 复用 RelationSearcher（关系查询器）

**已有能力**：
- `get_callers(entity_id)` - 获取调用者
- `get_callees(entity_id)` - 获取被调用者
- `query_backward(entity_id, options)` - 反向调用链查询

**复用方式**：
```rust
impl FindReferencesTool {
    pub fn find_references(&self, request: FindReferencesRequest) -> Result<FindReferencesResponse> {
        // 复用 RelationSearcher 的 get_callers 方法
        let callers = self.relation_searcher.get_callers(entity_id);
        
        // 获取引用位置详情
        let references = self.get_reference_details(callers, entity_id)?;
        
        Ok(FindReferencesResponse { references, ... })
    }
}
```

### 3. 复用 ParseCoordinator（解析协调器）

**已有能力**：
- `parse_with_language_info(source, path, language_info)` - 完整解析流程
- 返回 `ParsedFile` 包含所有实体和关系

**复用方式**：
```rust
impl GetSymbolsTool {
    fn get_symbols_for_file(&self, path: &str) -> Result<Vec<SymbolInfo>> {
        // 优先从索引获取
        if let Some(entities) = self.get_entities_from_index(path) {
            return self.build_symbol_tree(entities);
        }
        
        // 索引中没有时，实时解析
        let source = self.read_file(path)?;
        let language_info = LanguageInfo::detect_from_path(path);
        let parsed_file = self.parse_coordinator.parse_with_language_info(
            &source, path, &language_info
        )?;
        
        self.build_symbol_tree(parsed_file.entities)
    }
}
```

### 4. 复用 CompressionRetrieval（压缩检索）

**已有能力**：
- 文件验证和语言检测
- 缓存检查
- 完整解析流程
- 实体分组

**复用方式**：
```rust
impl GetSymbolsTool {
    pub fn get_symbols(&self, request: GetSymbolsRequest) -> Result<GetSymbolsResponse> {
        // 复用 CompressionRetrieval 的文件处理逻辑
        let (_, source, language_info) = self.compression_retrieval
            .validate_file(&request.path)?;
        
        // 复用解析能力
        let parsed_file = self.compression_retrieval
            .parse_file(&request.path, &source, &language_info.language)?;
        
        // 构建符号树
        self.build_symbol_tree(parsed_file.entities)
    }
}
```

## 详细设计

### 一、FindReferencesTool（查找引用）

#### 1.1 核心实现

```rust
// src/orchestrator/query/tools/symbol_lookup/find_references.rs

use std::sync::Arc;
use crate::relation::RelationIndex;
use crate::types::{EntityId, Entity, Span};

/// 查找引用工具
pub struct FindReferencesTool {
    /// 关系索引
    index: Arc<RelationIndex>,
    /// 配置
    config: FindReferencesConfig,
}

/// 配置
pub struct FindReferencesConfig {
    /// 默认上下文行数
    pub default_context_lines: usize,
    /// 是否包含定义本身
    pub include_definition: bool,
}

impl Default for FindReferencesConfig {
    fn default() -> Self {
        Self {
            default_context_lines: 2,
            include_definition: false,
        }
    }
}

impl FindReferencesTool {
    /// 创建新实例
    pub fn new(index: Arc<RelationIndex>) -> Self {
        Self {
            index,
            config: FindReferencesConfig::default(),
        }
    }
    
    /// 设置配置
    pub fn with_config(mut self, config: FindReferencesConfig) -> Self {
        self.config = config;
        self
    }
    
    /// 查找引用
    pub fn find_references(
        &self,
        request: FindReferencesRequest,
    ) -> Result<FindReferencesResponse> {
        // 1. 解析目标符号
        let target_id = self.resolve_symbol(&request)?;
        
        // 2. 从反向索引获取所有引用者（复用 callee_index）
        let callers = self.index.get_callers_by_callee_entity(target_id);
        
        // 3. 获取引用位置详情
        let references = self.get_reference_locations(callers, target_id)?;
        
        // 4. 按文件分组
        let grouped = self.group_by_file(references);
        
        Ok(FindReferencesResponse {
            symbol: request.symbol,
            total_count: grouped.iter().map(|g| g.references.len()).sum(),
            file_count: grouped.len(),
            references: grouped,
        })
    }
    
    /// 解析符号（复用 entity_file_index 和 function_index）
    fn resolve_symbol(&self, request: &FindReferencesRequest) -> Result<EntityId> {
        // 从文件路径和位置解析实体
        let entities = self.get_entities_by_file(&request.path)?;
        
        // 查找包含该位置的实体
        entities.into_iter()
            .find(|e| self.contains_position(&e.span, request.line, request.column))
            .map(|e| e.id)
            .ok_or_else(|| ToolError::SymbolNotFound)
    }
    
    /// 获取文件的实体（复用 entity_file_index）
    fn get_entities_by_file(&self, path: &str) -> Result<Vec<Entity>> {
        // 遍历 entity_file_index 找到该文件的所有实体
        let entities: Vec<Entity> = self.index.iter_entity_file_index()
            .filter(|(_, file_path)| file_path == path)
            .filter_map(|(entity_id, _)| {
                self.index.get_function_by_entity_id(entity_id)
                    .map(|e| e.value().clone())
            })
            .collect();
        
        if entities.is_empty() {
            Err(ToolError::FileNotFound(path.to_string()))
        } else {
            Ok(entities)
        }
    }
    
    /// 获取引用位置详情（复用 resolved_relation_index）
    fn get_reference_locations(
        &self,
        callers: Vec<EntityId>,
        target_id: EntityId,
    ) -> Result<Vec<ReferenceLocation>> {
        let mut locations = Vec::new();
        
        for caller_id in callers {
            // 获取调用关系（复用 resolved_relation_index）
            if let Some(relations) = self.index.get_resolved_relations_by_caller(caller_id) {
                for relation in relations.iter() {
                    if relation.callee_id == Some(target_id) {
                        // 获取文件路径（复用 entity_file_index）
                        let path = self.index.get_file_path_by_entity(caller_id)
                            .ok_or_else(|| ToolError::EntityNotFound(caller_id))?;
                        
                        locations.push(ReferenceLocation {
                            path,
                            line: relation.span.start_position.row + 1,
                            column: relation.span.start_position.column + 1,
                            end_line: relation.span.end_position.row + 1,
                            end_column: relation.span.end_position.column + 1,
                        });
                    }
                }
            }
        }
        
        Ok(locations)
    }
    
    /// 按文件分组
    fn group_by_file(&self, references: Vec<ReferenceLocation>) -> Vec<GroupedReferences> {
        use std::collections::HashMap;
        
        let mut groups: HashMap<String, Vec<ReferenceLocation>> = HashMap::new();
        
        for reference in references {
            groups.entry(reference.path.clone())
                .or_default()
                .push(reference);
        }
        
        groups.into_iter()
            .map(|(path, refs)| {
                GroupedReferences {
                    path,
                    count: refs.len(),
                    references: refs,
                }
            })
            .collect()
    }
}
```

#### 1.2 数据结构

```rust
// src/orchestrator/query/tools/symbol_lookup/types.rs

use serde::{Deserialize, Serialize};

/// 查找引用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindReferencesRequest {
    /// 文件路径
    pub path: String,
    /// 行号（1-based）
    pub line: usize,
    /// 列号（1-based，可选）
    pub column: Option<usize>,
    /// 符号名称（可选，用于文档）
    pub symbol: Option<String>,
    /// 上下文行数（可选）
    pub context_lines: Option<usize>,
}

/// 引用位置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceLocation {
    /// 文件路径
    pub path: String,
    /// 起始行号（1-based）
    pub line: usize,
    /// 起始列号（1-based）
    pub column: usize,
    /// 结束行号（1-based）
    pub end_line: usize,
    /// 结束列号（1-based）
    pub end_column: usize,
}

/// 按文件分组的引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedReferences {
    /// 文件路径
    pub path: String,
    /// 引用数量
    pub count: usize,
    /// 引用列表
    pub references: Vec<ReferenceLocation>,
}

/// 查找引用响应
#[derive(Debug, Clone, Serialize, Deserialize)]
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

### 二、GetSymbolsTool（获取符号）

#### 2.1 核心实现

```rust
// src/orchestrator/query/tools/symbol_lookup/get_symbols.rs

use std::sync::Arc;
use crate::relation::RelationIndex;
use crate::parser::coordinator::ParseCoordinator;
use crate::types::{Entity, EntityId, EntityKind};

/// 获取符号工具
pub struct GetSymbolsTool {
    /// 关系索引
    index: Arc<RelationIndex>,
    /// 解析协调器（用于实时解析）
    parse_coordinator: ParseCoordinator,
}

impl GetSymbolsTool {
    /// 创建新实例
    pub fn new(index: Arc<RelationIndex>) -> Self {
        Self {
            index,
            parse_coordinator: ParseCoordinator::new(),
        }
    }
    
    /// 获取符号
    pub fn get_symbols(
        &self,
        request: GetSymbolsRequest,
    ) -> Result<GetSymbolsResponse> {
        let mut results = Vec::new();
        
        for path in &request.paths {
            let result = self.get_symbols_for_file(path)?;
            results.push(result);
        }
        
        let success_count = results.iter().filter(|r| r.success).count();
        let fail_count = results.len() - success_count;
        
        Ok(GetSymbolsResponse {
            results,
            success_count,
            fail_count,
        })
    }
    
    /// 获取单个文件的符号
    fn get_symbols_for_file(&self, path: &str) -> Result<FileSymbolResult> {
        // 1. 尝试从索引获取（复用 entity_file_index）
        let entities = self.get_entities_from_index(path);
        
        if !entities.is_empty() {
            // 使用索引数据
            let symbols = self.build_symbol_tree(&entities)?;
            return Ok(FileSymbolResult {
                path: path.to_string(),
                success: true,
                symbol_count: Some(symbols.len()),
                symbols: Some(symbols),
                error: None,
            });
        }
        
        // 2. 索引中没有，实时解析（复用 ParseCoordinator）
        self.parse_and_extract_symbols(path)
    }
    
    /// 从索引获取实体（复用 entity_file_index）
    fn get_entities_from_index(&self, path: &str) -> Vec<Entity> {
        self.index.iter_entity_file_index()
            .filter(|(_, file_path)| file_path == path)
            .filter_map(|(entity_id, _)| {
                self.index.get_function_by_entity_id(entity_id)
                    .map(|e| e.value().clone())
            })
            .collect()
    }
    
    /// 构建符号树
    fn build_symbol_tree(&self, entities: &[Entity]) -> Result<Vec<SymbolInfo>> {
        // 构建父子关系映射
        let mut children_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut root_ids = Vec::new();
        
        for entity in entities {
            if let Some(parent_id) = entity.parent {
                children_map.entry(parent_id).or_default().push(entity.id);
            } else {
                root_ids.push(entity.id);
            }
        }
        
        // 递归构建符号树
        let symbols = root_ids.into_iter()
            .filter_map(|id| self.build_symbol_node(id, entities, &children_map))
            .collect();
        
        Ok(symbols)
    }
    
    /// 构建符号节点
    fn build_symbol_node(
        &self,
        entity_id: EntityId,
        entities: &[Entity],
        children_map: &HashMap<EntityId, Vec<EntityId>>,
    ) -> Option<SymbolInfo> {
        let entity = entities.iter().find(|e| e.id == entity_id)?;
        
        // 递归构建子节点
        let children = children_map.get(&entity_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.build_symbol_node(*id, entities, children_map))
                    .collect()
            })
            .unwrap_or_default();
        
        Some(SymbolInfo {
            name: entity.name.clone(),
            kind: entity.kind.to_symbol_kind(),
            line: entity.span.start_position.row + 1,
            end_line: entity.span.end_position.row + 1,
            detail: Some(entity.signature.clone()),
            children: if children.is_empty() { None } else { Some(children) },
        })
    }
    
    /// 实时解析并提取符号（复用 ParseCoordinator）
    fn parse_and_extract_symbols(&self, path: &str) -> Result<FileSymbolResult> {
        // 读取文件
        let source = std::fs::read_to_string(path)
            .map_err(|e| ToolError::FileNotReadable(e.to_string()))?;
        
        // 检测语言
        let language_info = LanguageInfo::detect_from_path(path);
        
        if language_info.language == Language::Unknown {
            return Ok(FileSymbolResult {
                path: path.to_string(),
                success: false,
                symbol_count: None,
                symbols: None,
                error: Some("Unknown language".to_string()),
            });
        }
        
        // 解析文件（复用 ParseCoordinator）
        let parsed_file = self.parse_coordinator
            .parse_with_language_info(&source, path, &language_info)
            .map_err(|e| ToolError::ParseError(e.to_string()))?;
        
        // 构建符号树
        let symbols = self.build_symbol_tree(&parsed_file.entities)?;
        
        Ok(FileSymbolResult {
            path: path.to_string(),
            success: true,
            symbol_count: Some(symbols.len()),
            symbols: Some(symbols),
            error: None,
        })
    }
}
```

#### 2.2 数据结构

```rust
/// 获取符号请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetSymbolsRequest {
    /// 文件路径列表
    pub paths: Vec<String>,
}

/// 符号信息
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    Struct = 23,
    EnumMember = 22,
}

/// 文件符号结果
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetSymbolsResponse {
    /// 各文件结果
    pub results: Vec<FileSymbolResult>,
    /// 成功数量
    pub success_count: usize,
    /// 失败数量
    pub fail_count: usize,
}
```

#### 2.3 EntityKind 映射

```rust
// src/types/entity/kind.rs

impl EntityKind {
    /// 转换为 SymbolKind
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

### 三、GotoDefinitionTool（跳转定义）

#### 3.1 核心实现

```rust
// src/orchestrator/query/tools/symbol_lookup/goto_definition.rs

use std::sync::Arc;
use crate::relation::RelationIndex;

/// 跳转定义工具
pub struct GotoDefinitionTool {
    /// 关系索引
    index: Arc<RelationIndex>,
}

impl GotoDefinitionTool {
    /// 创建新实例
    pub fn new(index: Arc<RelationIndex>) -> Self {
        Self { index }
    }
    
    /// 跳转到定义
    pub fn goto_definition(
        &self,
        request: GotoDefinitionRequest,
    ) -> Result<GotoDefinitionResponse> {
        // 1. 解析当前位置的符号
        let symbol_id = self.resolve_symbol_at_position(&request)?;
        
        // 2. 查找定义位置
        let definitions = self.find_definitions(symbol_id)?;
        
        // 3. 获取定义代码（如果需要）
        let definition_codes = if request.include_body.unwrap_or(true) {
            definitions.into_iter()
                .map(|def| self.get_definition_code(def))
                .collect::<Result<Vec<_>>>()?
        } else {
            definitions.into_iter()
                .map(|def| self.get_definition_location(def))
                .collect()
        };
        
        Ok(GotoDefinitionResponse {
            symbol: request.symbol,
            definitions: definition_codes,
        })
    }
    
    /// 解析位置处的符号（复用 entity_file_index）
    fn resolve_symbol_at_position(
        &self,
        request: &GotoDefinitionRequest,
    ) -> Result<EntityId> {
        // 获取文件实体
        let entities = self.get_entities_by_file(&request.path)?;
        
        // 查找包含该位置的实体
        let entity = entities.into_iter()
            .find(|e| {
                e.span.start_position.row + 1 <= request.line &&
                e.span.end_position.row + 1 >= request.line
            })
            .ok_or_else(|| ToolError::NoSymbolAtPosition)?;
        
        // 如果指定了列号，进一步精确匹配
        if let Some(column) = request.column {
            // 查找该列位置的符号
            self.find_symbol_at_column(&entity, column)
                .unwrap_or(Ok(entity.id))
        } else {
            Ok(entity.id)
        }
    }
    
    /// 查找定义位置（复用 function_index）
    fn find_definitions(&self, symbol_id: EntityId) -> Result<Vec<DefinitionLocation>> {
        let mut definitions = Vec::new();
        
        // 获取实体（复用 function_index）
        if let Some(entity) = self.index.get_function_by_entity_id(symbol_id) {
            let entity = entity.value();
            
            // 获取文件路径（复用 entity_file_index）
            let file_path = self.index.get_file_path_by_entity(symbol_id)
                .ok_or_else(|| ToolError::EntityNotFound(symbol_id))?;
            
            definitions.push(DefinitionLocation {
                path: file_path,
                entity_id: symbol_id,
                span: entity.span.clone(),
            });
        }
        
        // TODO: 查找相关定义（如接口实现）
        
        Ok(definitions)
    }
    
    /// 获取定义代码
    fn get_definition_code(
        &self,
        location: DefinitionLocation,
    ) -> Result<DefinitionCode> {
        // 读取文件
        let content = std::fs::read_to_string(&location.path)
            .map_err(|e| ToolError::FileNotReadable(e.to_string()))?;
        
        // 获取实体
        let entity = self.index.get_function_by_entity_id(location.entity_id)
            .ok_or_else(|| ToolError::EntityNotFound(location.entity_id))?
            .value()
            .clone();
        
        // 提取代码
        let code = self.extract_definition_code(&content, &entity)?;
        
        Ok(DefinitionCode {
            path: location.path,
            line: entity.span.start_position.row + 1,
            end_line: entity.span.end_position.row + 1,
            content: code,
            line_count: entity.span.end_position.row - entity.span.start_position.row + 1,
        })
    }
    
    /// 提取定义代码
    fn extract_definition_code(
        &self,
        content: &str,
        entity: &Entity,
    ) -> Result<String> {
        let lines: Vec<&str> = content.lines().collect();
        let start_line = entity.span.start_position.row;
        let end_line = entity.span.end_position.row;
        
        // 如果范围太小，尝试扩展到完整代码块
        let actual_end = if end_line - start_line < 2 {
            self.find_block_end(&lines, start_line)
                .unwrap_or(end_line)
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
    fn find_block_end(&self, lines: &[&str], start_line: usize) -> Option<usize> {
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
                            return Some(i);
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
        
        None
    }
}
```

#### 3.2 数据结构

```rust
/// 跳转定义请求
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone)]
pub struct DefinitionLocation {
    /// 文件路径
    pub path: String,
    /// 实体 ID
    pub entity_id: EntityId,
    /// 源码范围
    pub span: Span,
}

/// 定义代码
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// 跳转定义响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotoDefinitionResponse {
    /// 符号名称
    pub symbol: Option<String>,
    /// 定义列表
    pub definitions: Vec<DefinitionCode>,
}
```

## 模块导出

```rust
// src/orchestrator/query/tools/symbol_lookup/mod.rs

mod find_references;
mod get_symbols;
mod goto_definition;
mod types;
mod utils;

pub use find_references::{FindReferencesTool, FindReferencesConfig};
pub use get_symbols::GetSymbolsTool;
pub use goto_definition::GotoDefinitionTool;
pub use types::*;

/// 符号查找工具集合
pub struct SymbolLookupTools {
    pub find_references: FindReferencesTool,
    pub get_symbols: GetSymbolsTool,
    pub goto_definition: GotoDefinitionTool,
}

impl SymbolLookupTools {
    /// 创建所有符号查找工具
    pub fn new(index: Arc<RelationIndex>) -> Self {
        Self {
            find_references: FindReferencesTool::new(index.clone()),
            get_symbols: GetSymbolsTool::new(index.clone()),
            goto_definition: GotoDefinitionTool::new(index),
        }
    }
}
```

```rust
// src/orchestrator/query/tools/mod.rs

pub mod ast_diagnosis;
pub mod compression;
pub mod symbol_lookup;  // 新增

pub use ast_diagnosis::{...};
pub use compression::{...};
pub use symbol_lookup::{
    SymbolLookupTools,
    FindReferencesTool, FindReferencesRequest, FindReferencesResponse,
    GetSymbolsTool, GetSymbolsRequest, GetSymbolsResponse,
    GotoDefinitionTool, GotoDefinitionRequest, GotoDefinitionResponse,
};
```

## 复用总结

### 核心复用点

| 工具 | 复用组件 | 复用方法/数据 |
|-----|---------|-------------|
| FindReferencesTool | RelationIndex | `callee_index`（反向索引）<br>`resolved_relation_index`（调用关系）<br>`entity_file_index`（实体文件映射） |
| GetSymbolsTool | RelationIndex | `function_index`（实体索引）<br>`entity_file_index`（实体文件映射） |
| | ParseCoordinator | `parse_with_language_info`（实时解析） |
| GotoDefinitionTool | RelationIndex | `function_index`（实体索引）<br>`entity_file_index`（实体文件映射） |

### 性能优势

1. **O(1) 引用查找**：使用 `callee_index` 反向索引
2. **内存索引**：所有数据在内存中，无需磁盘 I/O
3. **线程安全**：使用 DashMap，支持并发查询
4. **可选实时解析**：索引缺失时可降级到实时解析

### 扩展性

1. **符号表集成**：可集成 `ProjectSymbolTable` 支持跨文件解析
2. **缓存层**：可添加文件内容缓存减少 I/O
3. **上下文提取**：可扩展支持引用位置的上下文代码提取
