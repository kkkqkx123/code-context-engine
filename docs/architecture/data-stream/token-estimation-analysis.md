# Token 估算分析文档

## 概述

Token 估算是系统中的一个工具，用于在无法直接调用 LLM Tokenizer 的情况下快速估算文本的 token 数量。这对于控制分块大小、管理 Embedding 批处理以及确保文本不超出模型 token 限制至关重要。

## TokenEstimator 工具

**位置**：`crates/cce_core/src/utils/token_estimation.rs`

### 核心实现

```rust
pub struct TokenEstimator {
    /// CJK 字符的 token 系数 (每个 CJK 字符 ≈ 1 token)
    pub cjk_factor: f32,
    /// Latin 字符的 token 系数 (每个 Latin 字符 ≈ 0.25 token, ~4 字符/token)
    pub latin_factor: f32,
}

// 默认配置
static DEFAULT_ESTIMATOR: TokenEstimator = TokenEstimator {
    cjk_factor: 1.0,    // 每个 CJK 字符计为 1 token
    latin_factor: 0.25, // 每 4 个 Latin 字符计为 1 token
};

// 代码专用估算器 (Latin factor更高以补偿代码中特殊符号和长标识符)
pub const CODE_LATIN_FACTOR: f32 = 0.35;

static CODE_ESTIMATOR: TokenEstimator = TokenEstimator {
    cjk_factor: 1.0,
    latin_factor: CODE_LATIN_FACTOR,
};
```

### 关键方法

| 方法 | 用途 |
|------|------|
| `estimate(text)` | 使用默认系数估算 token 数 |
| `estimate_with_config(text)` | 使用自定义系数估算 |
| `estimate_with_ratio(text, chars_per_token)` | 使用固定比例估算 |
| `fits_within(text, max_tokens)` | 检查文本是否在限制内 |
| `find_split_point(text, max_tokens)` | 找到安全的分割点 (按换行/空格) |
| `estimate_code_tokens(text)` | 使用代码专用系数(CODE_LATIN_FACTOR=0.35)估算代码文件 token 数 |

### 估算策略

1. **纯 ASCII 文本**：使用字符数除以拉丁系数
2. **混合文本 (ASCII + CJK)**：
   - 空格/制表符/换行: 0.5 token
   - CJK 字符: 1 token (可配置)
   - Latin 字符: 0.25 token (可配置)
   - 其他 Unicode (emoji, 符号): 1 token
3. **分割点查找**：在 token 限制内，优先在换行或空格处分割
4. **代码文件**：使用 CODE_ESTIMATOR (latin_factor=0.35)，对代码中的特殊符号、长标识符使用更高的系数

## 需要 Token 估算的处理步骤

### 1. Embedding 批处理创建 (LLM Client)

**位置**：`crates/cce_infrastructure/src/llm/services/embedding/handler.rs`

**目的**：
- 动态计算每批文本的 token 总数
- 确保单批不超过模型的 max_tokens 限制
- 根据 token 数调整批大小

**实现要点**：
```rust
fn build_batches(texts: &[&str], max_tokens_per_batch: usize) -> Vec<Vec<&str>> {
    let mut batches = Vec::new();
    let mut current_batch = Vec::new();
    let mut current_tokens = 0;

    for text in texts {
        let text_tokens = estimate_tokens(text);  // ← Token 估算点
        if current_tokens + text_tokens > max_tokens_per_batch {
            batches.push(std::mem::take(&mut current_batch));
            current_tokens = 0;
        }
        current_batch.push(text);
        current_tokens += text_tokens;
    }
    if !current_batch.is_empty() {
        batches.push(current_batch);
    }
    batches
}
```

**默认模型限制**：
| 模型 | Max Tokens | 备注 |
|------|-----------|------|
| text-embedding-3-small | 8192 | 默认 |
| text-embedding-3-large | 8192 |  |
| nomic-embed-text-v1.5 | 8192 |  |
| bge-m3 | 8192 |  |

### 2. 文本分块处理 (Chunker)

**位置**：`crates/cce_parser/src/ast_to_nl/chunker/chunker.rs`

**目的**：
- 确保每个块不超过 `max_chunk_size` (token 数)
- 在语义边界 (函数/类/段落) 分割
- 控制块之间的重叠 token 数

**配置参数**：
```rust
// crates/cce_core/src/config/modules/chunking.rs (通过 ChunkingConfig)
pub struct ChunkingConfig {
    pub max_chunk_size: usize,   // 最大块 token 数 (默认: 512)
    pub overlap_size: usize,     // 重叠 token 数 (默认: 64)
    pub min_chunk_size: usize,   // 最小块 token 数 (默认: 128)
}
```

**分割策略**：
1. **成员级分割**: 按类/模块成员分割
2. **句子级分割**: 按句子边界分割
3. **段落级分割**: 按段落边界分割
4. **行级分割**: 按行边界分割
5. **Token 级分割**: 最后手段，强制在 token 边界分割

### 3. Embedding 预处理 (Preprocessor)

**位置**：`crates/cce_infrastructure/src/llm/services/embedding/preprocessor.rs`

**目的**：
- 为特定 embedding 模型添加前缀/模板
- 预处理后需要重新估算 token 数
- 确保处理后的文本不超出限制

**预处理类型**：

| 预处理器 | 说明 | Token 影响 |
|---------|------|-----------|
| `NoopPreprocessor` | 不处理 | 0 |
| `PrefixPreprocessor` | 添加固定前缀 | ~2 tokens |
| `TemplatePreprocessor` | 模板替换 `{text}` | 模板长度决定 |
| `NomicPreprocessor` | 添加任务类型前缀 | ~2-4 tokens |
| `StellaPreprocessor` | 添加指令模板 | ~20 tokens |
| `ChainedPreprocessor` | 链式组合多个预处理器 | 各步骤之和 |

**示例**：
```rust
// Nomic 示例: search_document 任务
let preprocessor = NomicPreprocessor::new(NomicTaskType::SearchDocument);
// 原始: "function description"
// 处理后: "search_document: function description"
// Token 增加: ~3 tokens

// Stella 示例: S2P 任务
let preprocessor = StellaPreprocessor::s2p();
// 原始: "machine learning"
// 处理后: "Instruct: Given a web search query, retrieve relevant passages that answer the query.\nQuery: machine learning"
// Token 增加: ~20 tokens
```

### 4. Chat Completion 请求 (LLM Client)

**位置**：`crates/cce_infrastructure/src/llm/services/chat/handler.rs`

**目的**：
- 估算请求的总 token 数 (prompt + max_tokens)
- 用于限流和成本控制
- 防止超出上下文窗口

## Token 估算在数据流中的位置

### 完整索引流程

```
源文件
    │
    ▼
ParseCoordinator  ── 无需 Token 估算
    │
    ▼
FileProcessor
  ├── PreProcessor (分组)        ── 无需 Token 估算
  ├── AstToNlConverter (转换)    ── 无需 Token 估算
  ├── Chunker                    ── Token 估算点 #1
  │     └── 使用 estimate_tokens() 控制块大小
  └── 增强元数据                 ── 无需 Token 估算
    │
    ▼
StorageCoordinator
  ├── BM25 存储                  ── 无需 Token 估算
  ├── SQLite 存储                ── 无需 Token 估算
  └── Qdrant 存储                ── Token 估算点 #2
        └── 在 Embedding 批处理中使用 estimate_tokens() 控制批次大小
```

### 热更新流程

```
文件系统事件
    │
    ▼
HotUpdateCoordinator
  └── Processors:
        ├── ContextProcessor      ── Token 估算点 #1 (分块)
        ├── EmbeddingProcessor    ── Token 估算点 #2 (批处理)
        ├── Bm25Processor         ── 无需 Token 估算
        ├── RelationProcessor     ── 无需 Token 估算
        └── SummaryProcessor      ── Token 估算点 #3 (LLM 请求)
```

## Token 估算配置参数汇总

| 参数 | 默认值 | 说明 | 配置位置 |
|------|--------|------|---------|
| `max_chunk_size` | 512 | 最大块 token 数 | ChunkingConfig |
| `overlap_size` | 64 | 块间重叠 token 数 | ChunkingConfig |
| `min_chunk_size` | 128 | 最小块 token 数 | ChunkingConfig |
| `max_tokens_per_batch` | 8192 | 每批最大 token 数 | EmbedderConfig |
| `batch_max_tokens` | 4096 | 单 Embedding 请求的 max_tokens | EmbeddingProvider |
| `token_estimation_factor` | 0.25/1.0 (通用) / 0.35/1.0 (代码) | Latin/CJK token 估算系数，代码路径使用 CODE_LATIN_FACTOR=0.35 | TokenEstimator (硬编码) |

## Token 估算性能考虑

### 估算速度

- **ASCII 快速路径**：纯 ASCII 文本直接计算 `len / 4`，极快
- **混合路径**：逐字符遍历，对非 ASCII 文本较慢
- **批量估算**：建议批量处理，减少重复解析开销

### 估算精度

- **估算 vs 实际**：
  - Latin 文本：偏差约 ±5% (取决于具体 LLM Tokenizer)
  - CJK 文本：偏差约 ±10% (不同 Tokenizer 对 CJK 的处理差异)
  - 混合文本：偏差约 ±8%
  - 代码文本：偏差约 ±8% (取决于语言密度，如 Rust generics vs Go)

- **安全建议**：估算值不应超过实际限制的 90%，为预处理等留出余量

### 内存影响

- 估算操作是纯计算，无额外内存分配
- `find_split_point` 按字符迭代，无需拷贝
- 批量估算通常小于 1ms/100KB 文本

## 最佳实践建议

### 1. 批处理大小调优

- 设置 `max_tokens_per_batch` 为模型限制的 80%
- 动态调整批大小：token 数达到限制即创建新批
- 大文本 (超过 max_tokens) 应提前分割

### 2. Token 限制设置

```
max_chunk_size = 512  (默认值) 适用于大多数代码场景
max_chunk_size = 256  (代码文件较短时，提高精确度)
max_chunk_size = 1024 (需要更多上下文时，如大型类注释)
```

### 3. 预处理 Token 开销

```rust
// 预处理后需要重新估算 token 数
fn estimate_with_preprocessing(text: &str, preprocessor: &dyn TextPreprocessor) -> usize {
    let processed = preprocessor.process(text);
    estimate_tokens(&processed)
}
```

### 4. 监控与调优

- 使用日志记录估算 vs 实际 token 数的差异
- 定期校准系数以获得更好的精度
- 对不同的模型使用不同的系数 (如 GPT-4 vs Claude vs Nomic)

## 相关源文件

| 文件 | 作用 |
|------|------|
| `cce_core/src/utils/token_estimation.rs` | Token 估算核心工具 |
| `cce_parser/src/ast_to_nl/chunker/splitter.rs` | 分块分割器 (使用 find_split_point) |
| `cce_infrastructure/src/llm/services/embedding/handler.rs` | Embedding 批处理 (使用 estimate_tokens) |
| `cce_infrastructure/src/llm/services/embedding/preprocessor.rs` | 文本预处理 (影响 token 数) |
| `cce_core/src/config/modules/embedder.rs` | Embedding 配置 |
| `cce_core/src/config/modules/ast_to_nl.rs` | 分块配置 |

## 总结

Token 估算是系统中的一个关键工具，在分块和嵌入批处理两个核心环节发挥作用。通过快速、近似的 token 计算，系统能够在无法访问真实 Tokenizer 的情况下有效控制文本长度，确保下游处理不会超出模型限制。估算精度虽然不如真实 Tokenizer，但在设计上留有足够的安全余量，足以满足实际使用需求。
