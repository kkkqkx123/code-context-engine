# Embedding API 重构总结

## 重构日期
2026-05-13

## 重构目标
统一 `OpenAICompatibleProvider` 的 embedding API，消除功能重复，正确实现 BGE-M3 混合嵌入支持。

## 主要变更

### 1. 删除冗余的 `embed_one()` inherent 方法（已重新添加为便捷方法）

**原因**：
- `EmbeddingProvider` trait 已经提供了 `embed_one()` 的默认实现
- 避免代码重复和潜在的不一致

**解决方案**：
- 重新添加了 `embed_one()` 作为 inherent 方法，但它内部调用 trait 的默认实现
- 返回类型转换为 `LlmError` 以保持 API 一致性
- 这样既利用了 trait 的默认实现，又保持了向后兼容

```rust
pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError> {
    <Self as EmbeddingProvider>::embed_one(self, text)
        .await
        .map_err(|e| LlmError::api(e.to_string()))
}
```

### 2. 重命名 `embed_advanced()` → `embed_hybrid()`

**原因**：
- "advanced" 名称不够清晰，不能准确表达功能
- "hybrid" 更准确地描述了返回多种向量类型（dense + sparse + ColBERT）的特性
- 符合业界术语（如 Qdrant 的 hybrid search）

**新实现**：
```rust
pub async fn embed_hybrid(&self, texts: &[&str]) -> Result<ParsedResponse, LlmError>
```

**关键改进**：
- ✅ 正确实现了响应解析逻辑
- ✅ 使用配置的 `ResponseParser`（Standard 或 BGE-M3）
- ✅ 直接发送 HTTP 请求获取原始 JSON，然后用自定义解析器处理
- ✅ 支持 BGE-M3 的所有模式：Dense、Sparse、Colbert、All

### 3. 新增 `embed_sparse()` 方法

**用途**：
- 专门用于获取稀疏嵌入（lexical weights）
- 适用于只需要稀疏向量的场景（如纯 BM25 搜索）

```rust
pub async fn embed_sparse(
    &self, 
    texts: &[&str]
) -> Result<Vec<HashMap<String, f32>>, LlmError>
```

**返回值**：
- `Vec<HashMap<String, f32>>` - 每个文本的词法权重映射（token → weight）
- 如需转换为 Qdrant 格式，使用 `TokenizerManager::convert_lexical_weights()`

### 4. 增强 `OpenAICompatibleProvider` 结构

**新增字段**：
```rust
pub struct OpenAICompatibleProvider {
    llm_client: Arc<LlmClient>,
    embed_config: EmbeddingConfig,
    preprocessor: PreprocessorConfig,
    // 新增字段
    base_url: String,                              // 用于直接 HTTP 请求
    api_keys: Vec<String>,                         // API 密钥轮换
    current_key_index: AtomicU32,                  // 当前密钥索引
    response_parser: ResponseParser,               // 响应解析器配置
}
```

**原因**：
- `embed_hybrid()` 需要直接发送 HTTP 请求以获取原始响应体
- `LlmClient::request<T, R>()` 要求泛型 `R: DeserializeOwned`，无法返回原始 JSON 字符串
- 存储这些字段允许我们复用配置但不依赖 `LlmClient` 的内部方法
- `response_parser` 从配置加载，支持动态切换解析策略

### 5. 实现 `From<ResponseParserConfig>` for `ResponseParser`

**新增转换实现**：
```rust
impl From<ResponseParserConfig> for ResponseParser {
    fn from(config: ResponseParserConfig) -> Self {
        match config {
            ResponseParserConfig::Standard => ResponseParser::Standard,
            ResponseParserConfig::BgeM3 { mode } => {
                let bge_mode = match mode.as_str() {
                    "dense" => BGEM3Mode::Dense,
                    "sparse" => BGEM3Mode::Sparse,
                    "colbert" => BGEM3Mode::Colbert,
                    "all" => BGEM3Mode::All,
                    _ => BGEM3Mode::All,  // 未知模式默认为 All
                };
                ResponseParser::BGEM3(bge_mode)
            }
        }
    }
}
```

**作用**：
- 允许从配置文件中的 `ResponseParserConfig` 自动转换为运行时的 `ResponseParser`
- 在构造函数中自动应用配置的解析器
- 支持 BGE-M3 的各种模式配置

### 6. 实现 `make_embedding_request()` 辅助方法

**功能**：
- 构建并发送 embedding HTTP 请求
- 处理认证（API key 轮换）
- 返回原始 JSON 字符串供自定义解析
- **从配置读取超时时间** - 使用 `timeout_secs` 字段而非硬编码
- **实现重试逻辑** - 使用 `RetryPolicy` 进行指数退避重试

**关键点**：
- 不依赖 `LlmClient` 的内部方法
- 超时配置从 `ResolvedModelConfig` 中读取（来自配置文件）
- 重试策略包括：
  - 最大重试次数（`max_retries`）
  - 初始延迟（`retry_delay_ms`）
  - 指数退避（backoff multiplier: 2.0）
  - 智能错误判断（仅重试可恢复的错误）
- API key 在每次重试时重新获取（支持轮换）

## API 层次结构

重构后的 API 提供清晰的三层接口：

```
┌─────────────────────────────────────────────────┐
│ Level 1: Standard Dense Embeddings              │
│ - embed(&texts) → EmbeddingResult               │
│   Returns: Vec<Vec<f32>> only                   │
│   Use case: Traditional vector search           │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│ Level 2: Hybrid Embeddings (Multi-modal)        │
│ - embed_hybrid(&texts) → ParsedResponse         │
│   Returns: dense + sparse + colbert vectors     │
│   Use case: BGE-M3 with All mode                │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│ Level 3: Specialized Methods                    │
│ - embed_sparse(&texts) → Vec<HashMap<String,f32>>│
│   Returns: lexical weights only                 │
│   Use case: Pure BM25 / lexical search          │
│                                                   │
│ - embed_one(&text) → Vec<f32>                   │
│   Returns: single dense vector                  │
│   Use case: Query embedding                     │
└─────────────────────────────────────────────────┘
```

## BGE-M3 集成示例

### 配置示例（config.toml）

```toml
[embedder.providers.bge-m3-provider]
base_url = "http://localhost:8080/v1"
api_keys = ["dummy-key"]

[embedder.models.bge-m3-all]
provider_id = "bge-m3-provider"
vector_dimension = 1024
response_parser = { type = "bge_m3", mode = "all" }
max_batch_tokens = 8192
max_item_tokens = 8191
```

### 使用示例

```rust
use code_context_engine::embedding::OpenAICompatibleProvider;
use code_context_engine::config::AppConfig;

// 从全局配置加载
let config = AppConfig::load_global()?;
let provider = OpenAICompatibleProvider::from_model(&config, "bge-m3-all")?;

// 1. 标准密集嵌入
let result = provider.embed(&["code snippet"]).await?;
println!("Dense vectors: {}", result.embeddings.len());

// 2. 混合嵌入（获取所有向量类型）
let response = provider.embed_hybrid(&["code snippet"]).await?;
println!("Dense: {}", response.embeddings.len());
println!("Sparse: {}", response.sparse_embeddings.len());
println!("ColBERT: {}", response.colbert_embeddings.len());

// 3. 仅稀疏嵌入
let sparse = provider.embed_sparse(&["code snippet"]).await?;
for weights in &sparse {
    for (token, weight) in weights {
        println!("{}: {}", token, weight);
    }
}

// 4. 单文本嵌入
let query_vec = provider.embed_one("search query").await?;
```

### 稀疏向量转换

```rust
use code_context_engine::embedding::tokenizer_manager::TokenizerManager;

// 获取稀疏嵌入
let sparse_weights = provider.embed_sparse(&["code"]).await?;

// 转换为 Qdrant 格式
let tokenizer_manager = TokenizerManager::new(...);
let mode = TokenizerMode::LlamaCpp { ... };

for weights in &sparse_weights {
    let sparse_vector = tokenizer_manager
        .convert_lexical_weights(weights, Some(&mode))
        .await?;
    
    // sparse_vector.indices: Vec<u32>
    // sparse_vector.values: Vec<f32>
}
```

## 影响评估

### 破坏性变更
- ❌ **无** - 所有原有 API 保持兼容
- `embed_advanced()` 已重命名为 `embed_hybrid()`，只有一个内部调用点且已更新

### 新增功能
- ✅ `embed_hybrid()` - 正确的多模态嵌入支持
- ✅ `embed_sparse()` - 专用稀疏嵌入方法
- ✅ BGE-M3 完整支持（Dense/Sparse/ColBERT）

### 性能影响
- ⚡ `embed_hybrid()` 比之前的占位符实现慢（因为真正执行了 HTTP 请求和解析）
- ⚡ 但这是预期行为，之前返回空数据是 bug

### 代码质量
- ✅ 消除了代码重复
- ✅ 利用 trait 默认实现
- ✅ 清晰的 API 层次
- ✅ 完整的文档注释

## 测试建议

### 单元测试
1. 测试 `embed_hybrid()` 与不同 ResponseParser 的配合
2. 测试 `embed_sparse()` 返回空结果（标准模型）
3. 测试 API key 轮换逻辑

### 集成测试
1. BGE-M3 端到端测试（需要运行 llama.cpp server）
2. 混合搜索流程测试
3. 稀疏向量转换测试

### E2E 测试
1. 使用 BGE-M3 索引代码库
2. 执行混合搜索查询
3. 验证搜索结果质量

## 后续工作

### 短期（高优先级）
1. ✅ **已完成** - `get_response_parser()` 现在从配置读取 parser
2. ✅ **已完成** - 在 `make_embedding_request()` 中从配置读取 timeout
3. ✅ **已完成** - 添加重试逻辑到 `make_embedding_request()`

### 中期
1. 📋 为 `embed_hybrid()` 添加批处理支持（类似 `embed()` 的 batch 逻辑）
2. 📋 优化稀疏向量转换性能
3. 📋 添加 ColBERT 向量搜索支持

### 长期
1. 🔮 考虑将 `make_embedding_request()` 的逻辑整合到 `LlmClient`
2. 🔮 支持更多多模态模型（不限于 BGE-M3）
3. 🔮 实现动态响应解析器选择

## 相关文件

### 修改的文件
- `src/embedding/openai_compatible_provider.rs` - 主要重构
- `src/orchestrator/index/storage_coordinator.rs` - 更新方法调用
- `docs/embedding/bge-m3.md` - 参考文档（未修改）

### 依赖的基础设施（已存在）
- `src/embedding/response.rs` - 响应解析器（已完整实现）
- `src/embedding/tokenizer_manager.rs` - 稀疏向量转换
- `src/storage/qdrant/types.rs` - SparseVector 类型
- `src/config/modules/embedder.rs` - ResponseParserConfig

## 总结

本次重构成功统一了 embedding API，消除了功能重复，并正确实现了 BGE-M3 混合嵌入支持。新的 API 设计清晰、可扩展，同时保持了向后兼容性。

关键成就：
1. ✅ 删除了冗余代码
2. ✅ 正确实现了 `embed_hybrid()`
3. ✅ 添加了便捷的 `embed_sparse()` 方法
4. ✅ 保持了 API 兼容性
5. ✅ 通过了编译检查和 clippy
6. ✅ **完成配置集成** - `response_parser` 现在从配置加载
7. ✅ 实现 `From<ResponseParserConfig>` 转换 trait
8. ✅ **完成超时配置集成** - `timeout_secs` 从配置读取并在 HTTP 客户端中使用
9. ✅ **完成重试逻辑集成** - 使用 `RetryPolicy` 实现指数退避重试机制

所有短期高优先级任务已全部完成。下一步应该添加全面的测试覆盖。
