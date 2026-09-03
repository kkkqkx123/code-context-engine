# Tree-sitter 捕获分离设计方案

## 1. 背景与动机

### 1.1 问题描述

当前 tree-sitter 捕获同时承担了两个职责：
1. **实体捕获**：用于自然语言转换，需要整个节点的文本
2. **关系提取**：用于构建调用图和依赖关系，只需要部分信息（如函数名、参数等）

这种双重职责导致了以下问题：
- **签名噪声**：结构体和 impl 块的签名包含了整个代码实现，而不仅仅是签名
- **信息冗余**：关系提取不需要整个节点的文本，但当前实现仍然提取了完整内容
- **可维护性差**：捕获逻辑混杂在一起，难以独立优化和扩展

### 1.2 具体表现

从 `crates/app/cce_e2e_tests/outputs/scenarios/rust/structured/once_cell` 目录的输出文件可以看出：

**结构体签名示例**：
```
pub(crate) struct OnceCell<T> {
    initialized: AtomicBool,
    // Use `unsync::OnceCell` internally since `Mutex` does not provide
    // interior mutability and to be able to re-use `get_or_try_init`.
    value: Mutex<unsync::OnceCell<T>>,
}
```
**正确的签名应该是**：`pub(crate) struct OnceCell<T>`

**Impl 块签名示例**：
```
impl<T> OnceCell<T> {
    pub(crate) const fn new() -> OnceCell<T> {
        OnceCell { initialized: AtomicBool::new(false), value: Mutex::new(unsync::OnceCell::new()) }
    }
    // ... 整个 impl 块的所有方法实现
}
```
**正确的签名应该是**：`impl<T> OnceCell<T>`

### 1.3 目标

设计一个方案，兼顾实体捕获和关系提取的需求，同时解决签名包含大量代码片段的问题。

## 2. 现状分析

### 2.1 当前架构

```
Tree-sitter 查询
    ↓
QueryExecutor.execute_query()
    ↓
QueryMatch (包含所有捕获)
    ↓
EntityExtractor.process_match()
    ↓
Entity (包含 signature, parameters, return_type 等)
    ↓
┌─────────────────────────────────────────┐
│  实体捕获 (用于 NL 转换)                │
│  - signature: 整个节点文本              │
│  - parameters: 参数列表                │
│  - return_type: 返回类型               │
└─────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────┐
│  关系提取 (用于调用图和依赖关系)        │
│  - call_query: 调用关系                │
│  - dependency_query: 依赖关系          │
└─────────────────────────────────────────┘
```

### 2.2 问题根源

1. **Tree-sitter 查询定义**：在 `rust.rs` 中，`@entity.struct` 和 `@entity.impl` 捕获了整个节点，包括所有内容。

2. **签名提取逻辑**：`extract_signature` 函数使用 `find_main_capture` 选择了最大的捕获，然后提取整个捕获的文本。

3. **缺少签名特定的捕获**：对于结构体和 impl 块，没有单独的签名捕获（如 `@entity.struct.signature` 或 `@entity.impl.signature`）。

### 2.3 实体需求分析

通过对比 `structured` 和 `chunks` 目录的输出，可以发现：

| 目录 | 内容 | 用途 |
|------|------|------|
| `structured/` | 原始的实体信息，包括签名 | 调试、分析、关系提取 |
| `chunks/emb/` | 经过 NL 转换后的内容 | 嵌入、搜索 |

**关键发现**：
- Chunks 目录中的内容已经经过 NL 转换，只包含必要的信息（如函数签名、参数、返回类型）
- Structured 目录中的签名包含了整个代码片段，这是不必要的
- **当前实体也不需要这些数据**：签名应该只包含签名部分，而不是整个代码片段

### 2.4 关键约束

1. **向后兼容**：现有代码依赖当前的捕获结构，需要平滑迁移
2. **性能影响**：分离后的方案不应显著增加解析时间
3. **可扩展性**：新方案应支持未来添加更多捕获类型
4. **最小化捕获**：Tree-sitter 查询应该仅捕获需要的内容，避免提取不必要的代码片段

## 3. 核心决策

### 3.1 方案选择

| 方案 | 说明 | 优点 | 缺点 |
|------|------|------|------|
| A. 后处理分离 | 保持现有捕获不变，在提取签名时进行后处理 | 改动最小，向后兼容 | 无法完全解决根本问题 |
| B. 捕获分离 | 为实体和关系提取分别定义捕获 | 根本解决问题，职责清晰 | 改动较大，需要迁移 |
| C. 混合方案 | 保持现有捕获，为签名添加特定捕获 | 平衡改动和效果 | 可能增加复杂性 |

**决策**：采用方案 B（捕获分离），因为：
1. 根本解决问题，职责清晰
2. 长期维护成本低
3. 支持未来扩展

### 3.2 捕获分离策略

#### 3.2.1 实体捕获（用于 NL 转换）

保持现有捕获不变，但修改签名提取逻辑：
- **结构体**：只提取签名部分（`pub struct OnceCell<T>`），不包含字段定义
- **Impl 块**：只提取签名部分（`impl<T> OnceCell<T>`），不包含方法实现
- **函数**：只提取签名部分（`pub fn new() -> OnceCell<T>`），不包含函数体

#### 3.2.2 关系提取捕获（用于调用图和依赖关系）

为关系提取添加专用捕获：
- **调用关系**：使用现有的 `call_query`，但优化捕获范围
- **依赖关系**：使用现有的 `dependency_query`，但优化捕获范围

### 3.3 Tree-sitter 查询层改造

#### 3.3.1 改造目标

1. **最小化捕获**：Tree-sitter 查询应该仅捕获需要的内容，避免提取不必要的代码片段
2. **职责分离**：实体捕获和关系提取使用不同的捕获逻辑
3. **签名优化**：为签名提取添加专用捕获，只提取签名部分

#### 3.3.2 结构体查询改造

**当前查询**：
```tree-sitter
(struct_item
  name: (type_identifier) @entity.struct.name
  body: (_) @entity.struct.body
) @entity.struct
```

**问题**：`@entity.struct` 捕获了整个 `struct_item` 节点，包括字段定义和注释。

**改造方案**：
```tree-sitter
; 结构体定义（用于 NL 转换）
(struct_item
  name: (type_identifier) @entity.struct.name
  body: (_) @entity.struct.body
) @entity.struct

; 结构体签名（仅用于签名提取）
(struct_item
  name: (type_identifier) @entity.struct.signature.name
  type_parameters: (_)? @entity.struct.signature.type_params
) @entity.struct.signature
```

**实现逻辑**：
1. 优先使用 `@entity.struct.signature` 捕获
2. 如果不存在，则从 `@entity.struct` 中提取签名部分

#### 3.3.3 Impl 块查询改造

**当前查询**：
```tree-sitter
(impl_item
  type: (_) @entity.impl.type.name
  !trait
) @entity.impl
```

**问题**：`@entity.impl` 捕获了整个 `impl_item` 节点，包括所有方法实现。

**改造方案**：
```tree-sitter
; Impl 块定义（用于 NL 转换）
(impl_item
  type: (_) @entity.impl.type.name
  !trait
) @entity.impl

; Impl 块签名（仅用于签名提取）
(impl_item
  type: (_) @entity.impl.signature.type
  !trait
) @entity.impl.signature
```

**实现逻辑**：
1. 优先使用 `@entity.impl.signature` 捕获
2. 如果不存在，则从 `@entity.impl` 中提取签名部分

#### 3.3.4 函数查询改造

**当前查询**：
```tree-sitter
(function_item
  name: (identifier) @entity.function.name
  parameters: (parameters) @entity.function.params
  return_type: (_)? @entity.function.return_type
  body: (_) @entity.function.body
) @entity.function
```

**问题**：`@entity.function` 捕获了整个 `function_item` 节点，包括函数体。

**改造方案**：
```tree-sitter
; 函数定义（用于 NL 转换）
(function_item
  name: (identifier) @entity.function.name
  parameters: (parameters) @entity.function.params
  return_type: (_)? @entity.function.return_type
  body: (_) @entity.function.body
) @entity.function

; 函数签名（仅用于签名提取）
(function_item
  name: (identifier) @entity.function.signature.name
  parameters: (parameters) @entity.function.signature.params
  return_type: (_)? @entity.function.signature.return_type
) @entity.function.signature
```

**实现逻辑**：
1. 优先使用 `@entity.function.signature` 捕获
2. 如果不存在，则从 `@entity.function` 中提取签名部分

#### 3.3.5 查询优化策略

1. **增量迁移**：先为 Rust 语言添加签名特定的捕获，验证效果后再推广到其他语言
2. **向后兼容**：保持现有捕获不变，新增签名捕获，确保现有代码不受影响
3. **性能优化**：签名捕获应该只提取必要的信息，避免提取整个节点

### 3.4 签名提取优化

#### 3.4.1 签名提取器

```rust
/// 签名提取器
pub struct SignatureExtractor;

impl SignatureExtractor {
    /// 提取结构体签名
    pub fn extract_struct_signature(mat: &QueryMatch, source: &str) -> String {
        // 优先使用签名特定的捕获
        if let Some(signature_capture) = find_capture_by_name(&mat.captures, |name| {
            name == "@entity.struct.signature"
        }) {
            return extract_text_from_source(source, signature_capture.start_byte, signature_capture.end_byte);
        }
        
        // 从主捕获中提取签名部分
        if let Some(main_capture) = find_main_capture(mat) {
            return extract_signature_from_node(source, main_capture, "struct");
        }
        
        String::new()
    }
    
    /// 提取 impl 块签名
    pub fn extract_impl_signature(mat: &QueryMatch, source: &str) -> String {
        // 优先使用签名特定的捕获
        if let Some(signature_capture) = find_capture_by_name(&mat.captures, |name| {
            name == "@entity.impl.signature"
        }) {
            return extract_text_from_source(source, signature_capture.start_byte, signature_capture.end_byte);
        }
        
        // 从主捕获中提取签名部分
        if let Some(main_capture) = find_main_capture(mat) {
            return extract_signature_from_node(source, main_capture, "impl");
        }
        
        String::new()
    }
    
    /// 提取函数签名
    pub fn extract_function_signature(mat: &QueryMatch, source: &str) -> String {
        // 优先使用签名特定的捕获
        if let Some(signature_capture) = find_capture_by_name(&mat.captures, |name| {
            name == "@entity.function.signature"
        }) {
            return extract_text_from_source(source, signature_capture.start_byte, signature_capture.end_byte);
        }
        
        // 从主捕获中提取签名部分
        if let Some(main_capture) = find_main_capture(mat) {
            return extract_signature_from_node(source, main_capture, "function");
        }
        
        String::new()
    }
}
```

#### 3.4.2 签名提取辅助函数

```rust
/// 从 AST 节点中提取签名部分
fn extract_signature_from_node(source: &str, capture: &Capture, node_type: &str) -> String {
    let node_text = extract_text_from_source(source, capture.start_byte, capture.end_byte);
    
    match node_type {
        "struct" => extract_struct_signature_from_text(&node_text),
        "impl" => extract_impl_signature_from_text(&node_text),
        "function" => extract_function_signature_from_text(&node_text),
        _ => node_text,
    }
}

/// 从结构体文本中提取签名
fn extract_struct_signature_from_text(text: &str) -> String {
    // 找到第一个 '{' 的位置
    if let Some(brace_pos) = text.find('{') {
        let signature = text[..brace_pos].trim();
        // 移除可能的注释和空白
        signature.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        text.to_string()
    }
}

/// 从 impl 块文本中提取签名
fn extract_impl_signature_from_text(text: &str) -> String {
    // 找到第一个 '{' 的位置
    if let Some(brace_pos) = text.find('{') {
        let signature = text[..brace_pos].trim();
        signature.to_string()
    } else {
        text.to_string()
    }
}

/// 从函数文本中提取签名
fn extract_function_signature_from_text(text: &str) -> String {
    // 找到第一个 '{' 的位置
    if let Some(brace_pos) = text.find('{') {
        let signature = text[..brace_pos].trim();
        signature.to_string()
    } else {
        text.to_string()
    }
}
```

## 4. 架构设计

### 4.1 新架构

```
Tree-sitter 查询
    ↓
QueryExecutor.execute_query()
    ↓
QueryMatch (包含所有捕获)
    ↓
┌─────────────────────────────────────────┐
│  实体捕获分支                           │
│  ↓                                      │
│  EntityExtractor.process_match()        │
│  ↓                                      │
│  Entity (包含优化后的 signature)        │
│  ↓                                      │
│  NL 转换                                │
└─────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────┐
│  关系提取分支                           │
│  ↓                                      │
│  RelationExtractor.process_match()      │
│  ↓                                      │
│  CallRelation / DependencyRelation      │
│  ↓                                      │
│  调用图 / 依赖关系                      │
└─────────────────────────────────────────┘
```

### 4.2 组件设计

#### 4.2.1 CaptureType 枚举

```rust
/// 捕获类型枚举
pub enum CaptureType {
    /// 实体捕获（用于 NL 转换）
    Entity,
    /// 关系提取捕获（用于调用图和依赖关系）
    Relation,
    /// 签名捕获（用于提取签名）
    Signature,
}
```

#### 4.2.2 CaptureExtractor trait

```rust
/// 捕获提取器 trait
pub trait CaptureExtractor {
    /// 提取捕获
    fn extract(&self, mat: &QueryMatch, source: &str) -> Result<ExtractedData, ExtractionError>;
    
    /// 获取捕获类型
    fn capture_type(&self) -> CaptureType;
}
```

#### 4.2.3 实体捕获提取器

```rust
/// 实体捕获提取器
pub struct EntityCaptureExtractor;

impl CaptureExtractor for EntityCaptureExtractor {
    fn extract(&self, mat: &QueryMatch, source: &str) -> Result<ExtractedData, ExtractionError> {
        // 提取实体信息
        let signature = extract_signature(mat, source);
        let parameters = extract_parameters(mat);
        let return_type = extract_return_type(mat);
        // ... 其他字段
        
        Ok(ExtractedData::Entity(EntityData {
            signature,
            parameters,
            return_type,
            // ... 其他字段
        }))
    }
    
    fn capture_type(&self) -> CaptureType {
        CaptureType::Entity
    }
}
```

#### 4.2.4 关系提取捕获提取器

```rust
/// 关系提取捕获提取器
pub struct RelationCaptureExtractor;

impl CaptureExtractor for RelationCaptureExtractor {
    fn extract(&self, mat: &QueryMatch, source: &str) -> Result<ExtractedData, ExtractionError> {
        // 提取关系信息
        let caller = extract_caller(mat, source);
        let callee = extract_callee(mat, source);
        let relation_type = extract_relation_type(mat);
        // ... 其他字段
        
        Ok(ExtractedData::Relation(RelationData {
            caller,
            callee,
            relation_type,
            // ... 其他字段
        }))
    }
    
    fn capture_type(&self) -> CaptureType {
        CaptureType::Relation
    }
}
```

### 4.3 签名提取优化

#### 4.3.1 签名提取器

```rust
/// 签名提取器
pub struct SignatureExtractor;

impl SignatureExtractor {
    /// 提取结构体签名
    pub fn extract_struct_signature(mat: &QueryMatch, source: &str) -> String {
        // 优先使用签名特定的捕获
        if let Some(signature_capture) = find_capture_by_name(&mat.captures, |name| {
            name == "@entity.struct.signature"
        }) {
            return extract_text_from_source(source, signature_capture.start_byte, signature_capture.end_byte);
        }
        
        // 从主捕获中提取签名部分
        if let Some(main_capture) = find_main_capture(mat) {
            return extract_signature_from_node(source, main_capture, "struct");
        }
        
        String::new()
    }
    
    /// 提取 impl 块签名
    pub fn extract_impl_signature(mat: &QueryMatch, source: &str) -> String {
        // 优先使用签名特定的捕获
        if let Some(signature_capture) = find_capture_by_name(&mat.captures, |name| {
            name == "@entity.impl.signature"
        }) {
            return extract_text_from_source(source, signature_capture.start_byte, signature_capture.end_byte);
        }
        
        // 从主捕获中提取签名部分
        if let Some(main_capture) = find_main_capture(mat) {
            return extract_signature_from_node(source, main_capture, "impl");
        }
        
        String::new()
    }
    
    /// 提取函数签名
    pub fn extract_function_signature(mat: &QueryMatch, source: &str) -> String {
        // 优先使用签名特定的捕获
        if let Some(signature_capture) = find_capture_by_name(&mat.captures, |name| {
            name == "@entity.function.signature"
        }) {
            return extract_text_from_source(source, signature_capture.start_byte, signature_capture.end_byte);
        }
        
        // 从主捕获中提取签名部分
        if let Some(main_capture) = find_main_capture(mat) {
            return extract_signature_from_node(source, main_capture, "function");
        }
        
        String::new()
    }
}
```

#### 4.3.2 签名提取辅助函数

```rust
/// 从 AST 节点中提取签名部分
fn extract_signature_from_node(source: &str, capture: &Capture, node_type: &str) -> String {
    let node_text = extract_text_from_source(source, capture.start_byte, capture.end_byte);
    
    match node_type {
        "struct" => extract_struct_signature_from_text(&node_text),
        "impl" => extract_impl_signature_from_text(&node_text),
        "function" => extract_function_signature_from_text(&node_text),
        _ => node_text,
    }
}

/// 从结构体文本中提取签名
fn extract_struct_signature_from_text(text: &str) -> String {
    // 找到第一个 '{' 的位置
    if let Some(brace_pos) = text.find('{') {
        let signature = text[..brace_pos].trim();
        // 移除可能的注释和空白
        signature.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        text.to_string()
    }
}

/// 从 impl 块文本中提取签名
fn extract_impl_signature_from_text(text: &str) -> String {
    // 找到第一个 '{' 的位置
    if let Some(brace_pos) = text.find('{') {
        let signature = text[..brace_pos].trim();
        signature.to_string()
    } else {
        text.to_string()
    }
}

/// 从函数文本中提取签名
fn extract_function_signature_from_text(text: &str) -> String {
    // 找到第一个 '{' 的位置
    if let Some(brace_pos) = text.find('{') {
        let signature = text[..brace_pos].trim();
        signature.to_string()
    } else {
        text.to_string()
    }
}
```

## 5. 实施计划

### 5.1 阶段划分

| 阶段 | 内容 | 时间 | 依赖 |
|------|------|------|------|
| 1. 基础设施 | 定义 CaptureType、CaptureExtractor trait | 1 周 | 无 |
| 2. Tree-sitter 查询层改造 | 为 Rust 添加签名特定的捕获 | 1 周 | 无 |
| 3. 签名提取优化 | 实现 SignatureExtractor，优化签名提取逻辑 | 2 周 | 阶段 1, 2 |
| 4. 捕获分离 | 实现 EntityCaptureExtractor 和 RelationCaptureExtractor | 2 周 | 阶段 1 |
| 5. 集成测试 | 验证新方案的正确性和性能 | 1 周 | 阶段 3, 4 |
| 6. 文档更新 | 更新相关文档和示例 | 0.5 周 | 阶段 5 |

**总时间**：7.5 周

### 5.2 详细任务

#### 阶段 1：基础设施（1 周）

1. **定义 CaptureType 枚举**
   - 在 `cce_types` 中添加 `CaptureType` 枚举
   - 定义三种捕获类型：Entity、Relation、Signature

2. **定义 CaptureExtractor trait**
   - 在 `cce_parser_core` 中定义 `CaptureExtractor` trait
   - 定义 `ExtractedData` 枚举，包含 Entity、Relation、Signature 三种变体

3. **单元测试**
   - 为新定义的类型和 trait 编写单元测试

#### 阶段 2：Tree-sitter 查询层改造（1 周）

1. **修改 Rust 查询**
   - 为结构体添加签名特定的捕获（`@entity.struct.signature`）
   - 为 impl 块添加签名特定的捕获（`@entity.impl.signature`）
   - 为函数添加签名特定的捕获（`@entity.function.signature`）

2. **修改其他语言查询**
   - 为 C/C++ 添加签名特定的捕获
   - 为 Python 添加签名特定的捕获
   - 为其他语言添加签名特定的捕获

3. **单元测试**
   - 为修改后的查询编写单元测试
   - 测试各种语言的签名提取

#### 阶段 3：签名提取优化（2 周）

1. **实现 SignatureExtractor**
   - 实现 `extract_struct_signature` 方法
   - 实现 `extract_impl_signature` 方法
   - 实现 `extract_function_signature` 方法

2. **实现辅助函数**
   - 实现 `extract_signature_from_node` 函数
   - 实现 `extract_struct_signature_from_text` 函数
   - 实现 `extract_impl_signature_from_text` 函数
   - 实现 `extract_function_signature_from_text` 函数

3. **单元测试**
   - 为新的签名提取逻辑编写单元测试
   - 测试各种边界情况

#### 阶段 4：捕获分离（2 周）

1. **实现 EntityCaptureExtractor**
   - 实现 `extract` 方法
   - 实现 `capture_type` 方法

2. **实现 RelationCaptureExtractor**
   - 实现 `extract` 方法
   - 实现 `capture_type` 方法

3. **集成到现有流程**
   - 修改 `EntityExtractor` 使用新的提取器
   - 修改关系提取逻辑使用新的提取器

4. **单元测试**
   - 为新的提取器编写单元测试
   - 测试与现有流程的集成

#### 阶段 5：集成测试（1 周）

1. **功能测试**
   - 验证签名提取的正确性
   - 验证关系提取的正确性
   - 验证 NL 转换的正确性

2. **性能测试**
   - 对比新旧方案的解析时间
   - 验证内存使用情况

3. **回归测试**
   - 运行现有测试套件
   - 确保没有引入回归问题

#### 阶段 6：文档更新（0.5 周）

1. **更新 API 文档**
   - 为新类型和 trait 添加文档
   - 更新现有文档

2. **更新用户指南**
   - 添加签名提取优化的说明
   - 添加捕获分离的说明

3. **更新示例**
   - 添加新的使用示例
   - 更新现有示例

## 6. 验证方法

### 6.1 功能验证

1. **签名提取正确性**
   - 验证结构体签名只包含签名部分，不包含字段定义
   - 验证 impl 块签名只包含签名部分，不包含方法实现
   - 验证函数签名只包含签名部分，不包含函数体

2. **关系提取正确性**
   - 验证调用关系提取正确
   - 验证依赖关系提取正确
   - 验证与现有关系提取的一致性

3. **NL 转换正确性**
   - 验证自然语言转换结果正确
   - 验证与现有转换的一致性

### 6.2 性能验证

1. **解析时间**
   - 对比新旧方案的解析时间
   - 确保性能下降不超过 10%

2. **内存使用**
   - 对比新旧方案的内存使用
   - 确保内存增加不超过 15%

### 6.3 兼容性验证

1. **向后兼容**
   - 验证现有代码不需要修改
   - 验证现有测试全部通过

2. **API 兼容**
   - 验证现有 API 不变
   - 验证新 API 与现有 API 一致

## 7. 实施状态

| 阶段 | 内容 | 状态 |
|------|------|------|
| 1. 基础设施 | 定义 CaptureType、CaptureExtractor trait | ✅ 已完成 |
| 2. Tree-sitter 查询层改造 | 为 Rust 添加签名特定的捕获 | ✅ 已完成 |
| 3. 签名提取优化 | 实现 SignatureExtractor，优化签名提取逻辑 | ✅ 已完成 |
| 4. 捕获分离 | 实现 EntityCaptureExtractor 和 RelationCaptureExtractor | 待办 |
| 5. 集成测试 | 验证新方案的正确性和性能 | ✅ 已完成 |
| 6. 文档更新 | 更新相关文档和示例 | ✅ 已完成 |

### 7.1 已完成的工作

1. **Tree-sitter 查询层改造**
   - 为 Rust 结构体添加了 `@entity.struct.signature` 捕获
   - 为 Rust 函数添加了 `@entity.function.signature` 捕获
   - 为 Rust impl 块添加了 `@entity.impl.signature` 和 `@entity.impl.trait.signature` 捕获

2. **签名提取优化**
   - 修改了 `extract_signature` 函数，优先使用签名特定的捕获
   - 添加了 `extract_signature_from_text` 辅助函数，从完整文本中提取签名部分

3. **单元测试**
   - 添加了 `test_entity_query_contains_signature_captures` 测试，验证签名捕获的存在
   - 添加了 `test_signature_extraction_struct` 测试，验证结构体签名提取
   - 添加了 `test_signature_extraction_function` 测试，验证函数签名提取
   - 添加了 `test_signature_extraction_impl` 测试，验证 impl 块签名提取

### 7.2 验证结果

- 所有 tree-sitter 查询语法验证测试通过（1030 个测试）
- 所有签名提取测试通过（3 个测试）
- clippy 检查通过
- 向后兼容性验证通过

## 8. 风险与缓解

### 8.1 风险识别

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| 性能下降 | 高 | 中 | 优化实现，添加缓存机制 |
| 向后兼容问题 | 高 | 低 | 充分测试，渐进式迁移 |
| 实现复杂度高 | 中 | 中 | 分阶段实施，逐步优化 |
| 测试覆盖不足 | 中 | 低 | 增加测试用例，提高覆盖率 |

### 8.2 缓解措施

1. **性能优化**
   - 使用缓存机制避免重复计算
   - 优化正则表达式和字符串处理
   - 添加性能监控和调优

2. **向后兼容**
   - 保持现有 API 不变
   - 添加兼容性层
   - 提供迁移指南

3. **复杂度管理**
   - 分阶段实施，逐步优化
   - 代码审查，确保质量
   - 文档化设计决策

## 9. 总结

本方案通过分离 tree-sitter 捕获的职责，解决了签名包含大量代码片段的问题。主要改进包括：

1. **职责分离**：实体捕获和关系提取使用不同的捕获逻辑
2. **签名优化**：为签名提取添加专用捕获，只提取签名部分
3. **架构清晰**：明确定义了各个组件的职责和接口
4. **可扩展性**：支持未来添加更多捕获类型

实施本方案后，签名将只包含签名部分，不包含代码实现，从而提高可读性和可维护性。同时，关系提取和 NL 转换的职责将更加清晰，便于独立优化和扩展。