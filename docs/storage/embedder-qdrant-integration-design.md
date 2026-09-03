# Embedder 与 Qdrant 集成设计方案

## 一、设计原则

### 1.1 架构约束

根据项目整体架构设计，必须遵循以下原则：

1. **模块独立性**：`embedder` 和 `storage/qdrant` 各自保持独立，职责单一
2. **无服务层抽象**：不在 `storage` 目录下创建集成服务类
3. **调用层协调**：集成逻辑由调用层（API handlers、indexer）负责
4. **显式依赖**：依赖关系明确，不隐藏复杂性

### 1.2 模块职责

| 模块 | 职责 | 不应承担的职责 |
|------|------|----------------|
| `embedder` | 文本 → 向量 | 存储、协调 |
| `storage/qdrant` | 向量存储/搜索 | 向量化、业务逻辑 |
| `api/handlers` | 请求处理、协调 | 核心算法 |

## 二、现有模块分析

### 2.1 Embedder 模块

**位置**：`src/embedder/`

**核心接口**：

```rust
// embedder.rs
pub struct Embedder { ... }

impl Embedder {
    /// 批量嵌入文本
    pub async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, EmbedError>;
    
    /// 单文本嵌入
    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>, EmbedError>;
}

pub struct EmbeddingResult {
    pub embeddings: Vec<Vec<f32>>,
    pub prompt_tokens: u64,
    pub total_tokens: u64,
}
```

**配置**：`EmbedderConfig` 支持多种供应商（OpenAI、Gemini、Ollama、BGE-M3 等）

### 2.2 Qdrant 模块

**位置**：`src/storage/qdrant/`

**核心接口**：

```rust
// client.rs
pub struct QdrantClient { ... }

impl QdrantClient {
    /// 初始化集合
    pub async fn initialize(&self) -> Result<bool, QdrantError>;
    
    /// 批量插入向量
    pub async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), QdrantError>;
    
    /// 向量搜索
    pub async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, QdrantError>;
    
    /// 按文件路径删除
    pub async fn delete_by_file_path(&self, file_path: &str) -> Result<(), QdrantError>;
}

pub struct VectorPoint {
    pub id: String,
    pub vector: Vec<f32>,
    pub payload: Payload,
}

pub struct Payload {
    pub file_path: String,
    pub code_chunk: String,
    pub start_line: u32,
    pub end_line: u32,
    pub entity_type: Option<String>,
    pub path_segments: HashMap<String, String>,
    pub extra: HashMap<String, serde_json::Value>,
}
```

## 三、集成方案

### 3.1 集成位置

集成逻辑应放在 **调用层**，即：

- `src/api/handlers/index.rs` - 索引 API 处理器
- `src/api/handlers/search.rs` - 搜索 API 处理器

### 3.2 数据流

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          API Handler Layer                               │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                     handle_index()                                  │ │
│  │                                                                     │ │
│  │  1. Parser.parse() → ParsedFile                                     │ │
│  │  2. AstToNlConverter.convert() → Vec<ConversionResult>              │ │
│  │  3. Embedder.embed(texts) → EmbeddingResult                         │ │
│  │  4. QdrantClient.upsert_points(points)                              │ │
│  │                                                                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                     handle_search()                                 │ │
│  │                                                                     │ │
│  │  1. Embedder.embed_one(query) → Vec<f32>                            │ │
│  │  2. QdrantClient.search(query_vector) → Vec<SearchResult>           │ │
│  │                                                                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.3 索引处理器实现

更新 `src/api/handlers/index.rs`：

```rust
//! Index API handler
//!
//! Handles code indexing requests, coordinating:
//! - Parser: code parsing
//! - AstToNlConverter: AST to natural language
//! - Embedder: text vectorization
//! - QdrantClient: vector storage

use crate::embedder::{Embedder, EmbeddingResult};
use crate::parser::Parser;
use crate::ast_to_nl::AstToNlConverter;
use crate::storage::qdrant::{QdrantClient, VectorPoint, Payload};
use crate::types::ast::ConversionOptions;

/// Index request
pub struct IndexRequest {
    /// File paths to index
    pub file_paths: Vec<String>,
    /// Workspace path
    pub workspace_path: String,
}

/// Index result
pub struct IndexResult {
    /// Number of files processed
    pub files_processed: usize,
    /// Number of entities indexed
    pub entities_indexed: usize,
    /// Total tokens used
    pub tokens_used: u64,
    /// Errors
    pub errors: Vec<String>,
}

/// Handle index request
///
/// Coordinates the full indexing pipeline:
/// 1. Parse files → AST
/// 2. Convert AST → Natural Language
/// 3. Embed text → Vectors
/// 4. Store vectors → Qdrant
pub async fn handle_index(
    request: IndexRequest,
    parser: &Parser,
    converter: &AstToNlConverter,
    embedder: &Embedder,
    qdrant: &QdrantClient,
) -> Result<IndexResult, IndexError> {
    let mut result = IndexResult {
        files_processed: 0,
        entities_indexed: 0,
        tokens_used: 0,
        errors: Vec::new(),
    };
    
    // Process each file
    for file_path in &request.file_paths {
        match process_file(file_path, parser, converter, embedder, qdrant).await {
            Ok(file_result) => {
                result.files_processed += 1;
                result.entities_indexed += file_result.entities_count;
                result.tokens_used += file_result.tokens_used;
            }
            Err(e) => {
                result.errors.push(format!("{}: {}", file_path, e));
            }
        }
    }
    
    Ok(result)
}

/// Process a single file
async fn process_file(
    file_path: &str,
    parser: &Parser,
    converter: &AstToNlConverter,
    embedder: &Embedder,
    qdrant: &QdrantClient,
) -> Result<FileResult, IndexError> {
    // 1. Parse file
    let parsed = parser.parse_file(file_path).await
        .map_err(IndexError::Parse)?;
    
    // 2. Convert to natural language (embedding mode)
    let options = ConversionOptions::embedding();
    let conversions: Vec<_> = parsed.entities.iter()
        .map(|entity| converter.convert(entity, file_path, Some(&options)))
        .collect();
    
    // 3. Extract texts for embedding
    let texts: Vec<&str> = conversions.iter()
        .filter_map(|c| c.embedding_text.as_deref())
        .collect();
    
    if texts.is_empty() {
        return Ok(FileResult { entities_count: 0, tokens_used: 0 });
    }
    
    // 4. Embed texts
    let embedding_result = embedder.embed(&texts).await
        .map_err(IndexError::Embed)?;
    
    // 5. Build vector points
    let points: Vec<VectorPoint> = conversions.iter()
        .zip(embedding_result.embeddings.iter())
        .filter_map(|(conv, vector)| {
            Some(VectorPoint::new(
                format!("{}-{}", file_path, conv.entity_id.0),
                vector.clone(),
                Payload::new(
                    file_path,
                    conv.embedding_text.as_ref()?,
                    conv.entity_id.0 as u32, // start_line (simplified)
                    conv.entity_id.0 as u32 + 1, // end_line (simplified)
                ),
            ))
        })
        .collect();
    
    // 6. Store in Qdrant
    qdrant.upsert_points(&points).await
        .map_err(IndexError::Storage)?;
    
    Ok(FileResult {
        entities_count: points.len(),
        tokens_used: embedding_result.total_tokens,
    })
}

struct FileResult {
    entities_count: usize,
    tokens_used: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("Parse error: {0}")]
    Parse(#[from] crate::types::error::ParseError),
    
    #[error("Embed error: {0}")]
    Embed(#[from] crate::types::error::EmbedError),
    
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::qdrant::QdrantError),
}
```

### 3.4 搜索处理器实现

更新 `src/api/handlers/search.rs`：

```rust
//! Search API handler
//!
//! Handles semantic code search requests.

use crate::embedder::Embedder;
use crate::storage::qdrant::{QdrantClient, SearchQuery, SearchResult};

/// Search request
pub struct SearchRequest {
    /// Query text
    pub query: String,
    /// Maximum results
    pub limit: usize,
    /// Minimum similarity score
    pub min_score: Option<f32>,
    /// Directory prefix filter
    pub directory_prefix: Option<String>,
}

/// Search result
pub struct SearchResponse {
    /// Search results
    pub results: Vec<SearchResult>,
    /// Query tokens used
    pub tokens_used: u64,
}

/// Handle search request
///
/// Coordinates:
/// 1. Embed query text → query vector
/// 2. Search in Qdrant → similar vectors
pub async fn handle_search(
    request: SearchRequest,
    embedder: &Embedder,
    qdrant: &QdrantClient,
) -> Result<SearchResponse, SearchError> {
    // 1. Embed query
    let query_vector = embedder.embed_one(&request.query).await
        .map_err(SearchError::Embed)?;
    
    // 2. Build search query
    let mut search_query = SearchQuery::new(query_vector, request.limit);
    
    if let Some(score) = request.min_score {
        search_query = search_query.with_min_score(score);
    }
    
    if let Some(prefix) = request.directory_prefix.as_deref() {
        search_query = search_query.with_directory_prefix(prefix);
    }
    
    // 3. Search in Qdrant
    let results = qdrant.search(search_query).await
        .map_err(SearchError::Storage)?;
    
    Ok(SearchResponse {
        results,
        tokens_used: 0, // embed_one doesn't return token count
    })
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Embed error: {0}")]
    Embed(#[from] crate::types::error::EmbedError),
    
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::qdrant::QdrantError),
}
```

## 四、配置集成

### 4.1 配置结构

更新 `src/config/config.rs`：

```rust
//! Application configuration

use serde::{Deserialize, Serialize};
use crate::embedder::EmbedderConfig;
use crate::storage::qdrant::QdrantConfig;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    /// Embedder configuration
    pub embedder: EmbedderConfig,
    /// Qdrant configuration
    pub qdrant: QdrantConfig,
    /// Indexing configuration
    pub indexing: IndexingConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

/// Indexing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingConfig {
    /// Batch size for embedding
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Maximum concurrent embedding requests
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
}

fn default_batch_size() -> usize { 100 }
fn default_max_concurrent() -> usize { 5 }

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            batch_size: default_batch_size(),
            max_concurrent: default_max_concurrent(),
        }
    }
}
```

### 4.2 配置文件示例

`config.toml`：

```toml
[server]
host = "0.0.0.0"
port = 8080

[indexing]
batch_size = 100
max_concurrent = 5

[embedder]
api_keys = ["sk-xxx"]
base_url = "https://api.openai.com/v1"
model = "text-embedding-3-small"
max_batch_tokens = 8192
timeout_secs = 30

[qdrant]
url = "http://localhost:6333"
vector_size = 1536
distance_metric = "cosine"
preset = "medium"
```

## 五、向量维度匹配

### 5.1 常见模型维度

| 模型 | 维度 | 配置 vector_size |
|------|------|------------------|
| `text-embedding-3-small` | 1536 | 1536 |
| `text-embedding-3-large` | 3072 | 3072 |
| `gemini-embedding-001` | 768 | 768 |
| `bge-m3` | 1024 | 1024 |
| `nomic-embed-text-v1` | 768 | 768 |

### 5.2 维度验证

在初始化时验证维度匹配：

```rust
/// Initialize and validate configuration
pub async fn initialize(
    embedder: &Embedder,
    qdrant: &QdrantClient,
) -> Result<(), InitError> {
    // Initialize Qdrant collection
    qdrant.initialize().await.map_err(InitError::Storage)?;
    
    // Validate dimension by test embedding
    let test_vector = embedder.embed_one("test").await
        .map_err(InitError::Embed)?;
    
    let actual_dim = test_vector.len();
    let expected_dim = qdrant.config().vector_size;
    
    if actual_dim != expected_dim {
        return Err(InitError::DimensionMismatch {
            expected: expected_dim,
            actual: actual_dim,
        });
    }
    
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("Embed error: {0}")]
    Embed(#[from] crate::types::error::EmbedError),
    
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::qdrant::QdrantError),
    
    #[error("Dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
}
```

## 六、批量处理

### 6.1 在 Handler 中实现批量逻辑

```rust
/// Process files in batches
pub async fn handle_index_batch(
    request: IndexRequest,
    parser: &Parser,
    converter: &AstToNlConverter,
    embedder: &Embedder,
    qdrant: &QdrantClient,
    batch_size: usize,
) -> Result<IndexResult, IndexError> {
    let mut result = IndexResult::default();
    
    // Process files in batches
    for batch in request.file_paths.chunks(batch_size) {
        // Collect all texts from batch
        let mut all_texts = Vec::new();
        let mut all_metadata = Vec::new();
        
        for file_path in batch {
            let parsed = parser.parse_file(file_path).await?;
            let options = ConversionOptions::embedding();
            
            for entity in &parsed.entities {
                let conv = converter.convert(entity, file_path, Some(&options));
                if let Some(ref text) = conv.embedding_text {
                    all_texts.push(text.as_str());
                    all_metadata.push((file_path, conv.entity_id, conv));
                }
            }
        }
        
        // Embed all texts in one request
        let embedding_result = embedder.embed(&all_texts).await?;
        
        // Build and store points
        let points: Vec<VectorPoint> = all_metadata.iter()
            .zip(embedding_result.embeddings.iter())
            .map(|((path, id, conv), vector)| {
                VectorPoint::new(
                    format!("{}-{}", path, id.0),
                    vector.clone(),
                    Payload::new(path, conv.embedding_text.as_ref().unwrap(), 1, 1),
                )
            })
            .collect();
        
        qdrant.upsert_points(&points).await?;
        
        result.entities_indexed += points.len();
        result.tokens_used += embedding_result.total_tokens;
    }
    
    Ok(result)
}
```

## 七、依赖关系

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          API Handler Layer                               │
│                                                                          │
│              handle_index() / handle_search()                            │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    │               │               │
                    ▼               ▼               ▼
┌─────────────────────┐ ┌─────────────────────┐ ┌─────────────────────┐
│      Embedder       │ │    QdrantClient     │ │       Parser        │
│   (文本 → 向量)      │ │   (向量存储)         │ │   (代码解析)         │
└─────────────────────┘ └─────────────────────┘ └─────────────────────┘
```

**关键点**：
- Handler 直接依赖 Embedder 和 QdrantClient
- 无中间服务层
- 集成逻辑在 Handler 中显式实现

## 八、文件结构

```
src/
├── api/
│   └── handlers/
│       ├── mod.rs           # 导出
│       ├── index.rs         # 索引处理器（集成逻辑）
│       └── search.rs        # 搜索处理器（集成逻辑）
├── embedder/                # 独立模块，不修改
│   ├── mod.rs
│   ├── embedder.rs
│   ├── config.rs
│   └── ...
├── storage/
│   ├── mod.rs
│   └── qdrant/              # 独立模块，不修改
│       ├── mod.rs
│       ├── client.rs
│       ├── config.rs
│       └── ...
├── config/
│   └── config.rs            # 更新：添加集成配置
└── ...
```

**不创建的文件**：
- ❌ `src/storage/vector_service.rs` - 破坏架构
- ❌ `src/embedder/service.rs` - 职责不清

## 九、总结

### 9.1 设计要点

1. **保持模块独立**：Embedder 和 Qdrant 各自保持单一职责
2. **调用层协调**：集成逻辑在 API handlers 中实现
3. **显式依赖**：Handler 直接依赖 Embedder 和 QdrantClient
4. **无服务层抽象**：不创建额外的"服务类"

### 9.2 实现步骤

1. 更新 `src/config/config.rs` - 添加集成配置
2. 更新 `src/api/handlers/index.rs` - 实现索引处理器
3. 更新 `src/api/handlers/search.rs` - 实现搜索处理器
4. 添加初始化验证逻辑

### 9.3 与旧方案的区别

| 方面 | 旧方案（错误） | 新方案（正确） |
|------|---------------|---------------|
| 集成位置 | `src/storage/vector_service.rs` | `src/api/handlers/` |
| 架构层次 | 新增服务层 | 调用层协调 |
| 模块独立性 | 破坏 | 保持 |
| 依赖关系 | 隐藏 | 显式 |

---

*文档创建日期：2026-03-23*
