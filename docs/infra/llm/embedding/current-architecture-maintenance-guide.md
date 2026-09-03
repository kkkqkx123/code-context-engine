# 嵌入模型当前架构维护指南

## 概述

本文档提供了当前嵌入模块架构的维护指南，包括如何添加新模型、修改现有模型配置以及相关的最佳实践。

## 当前架构说明

### 目录结构

```
src/embedding/
├── preprocessor.rs      # 文本预处理策略
├── response.rs          # 响应解析策略
├── config.rs            # 配置管理
├── http_embedder.rs     # HTTP嵌入器实现
└── error.rs             # 错误类型定义
```

### 模块职责

| 模块 | 职责 |
|------|------|
| `preprocessor.rs` | 定义文本预处理策略（Nomic、Stella等模型的特殊要求） |
| `response.rs` | 定义响应解析策略（BGE-M3等模型的多模态响应） |
| `config.rs` | 提供配置结构和模型工厂方法 |
| `http_embedder.rs` | 实现HTTP嵌入器，协调预处理和响应解析 |
| `error.rs` | 定义错误类型 |

## 添加新模型指南

### 步骤1：定义任务类型（如需要）

如果模型需要任务特定的处理，在 `preprocessor.rs` 中定义任务类型枚举：

```rust
/// MyModel task types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MyModelTaskType {
    /// Task 1
    Task1,
    /// Task 2
    Task2,
}

impl MyModelTaskType {
    /// Get the prefix string for this task type
    pub fn as_prefix(&self) -> &'static str {
        match self {
            Self::Task1 => "task1: ",
            Self::Task2 => "task2: ",
        }
    }
}
```

### 步骤2：实现预处理器

在 `preprocessor.rs` 中实现预处理器：

```rust
/// MyModel preprocessor
#[derive(Debug, Clone)]
pub struct MyModelPreprocessor {
    task_type: MyModelTaskType,
    inner: PrefixPreprocessor,
}

impl MyModelPreprocessor {
    /// Create a new MyModel preprocessor
    pub fn new(task_type: MyModelTaskType) -> Self {
        Self {
            task_type,
            inner: PrefixPreprocessor::new(task_type.as_prefix()),
        }
    }

    /// Create a task1 preprocessor
    pub fn task1() -> Self {
        Self::new(MyModelTaskType::Task1)
    }

    /// Get the task type
    pub fn task_type(&self) -> MyModelTaskType {
        self.task_type
    }
}

impl TextPreprocessor for MyModelPreprocessor {
    fn process(&self, text: &str) -> String {
        self.inner.process(text)
    }
}
```

### 步骤3：更新预处理器配置

在 `config.rs` 的 `PreprocessorConfig` 枚举中添加新变体：

```rust
/// Preprocessor configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PreprocessorConfig {
    /// No preprocessing (default)
    #[default]
    None,
    /// Simple prefix
    Prefix { prefix: String },
    /// Template with {text} placeholder
    Template { template: String },
    /// Nomic-Embed task type
    Nomic {
        task_type: super::preprocessor::NomicTaskType,
    },
    /// Stella task type
    Stella {
        task_type: super::preprocessor::StellaTaskType,
    },
    /// MyModel task type
    MyModel {
        task_type: super::preprocessor::MyModelTaskType,
    },
}
```

### 步骤4：定义响应格式（如需要）

如果模型有特殊的响应格式，在 `response.rs` 中定义：

```rust
/// MyModel embedding response data
#[derive(Debug, Clone, Deserialize)]
pub struct MyModelEmbeddingData {
    /// Embedding vector
    pub embedding: Vec<f32>,
    /// Index in the input batch
    pub index: usize,
    /// MyModel specific field
    pub metadata: Option<serde_json::Value>,
}

/// MyModel embedding response
#[derive(Debug, Clone, Deserialize)]
pub struct MyModelEmbeddingResponse {
    pub data: Vec<MyModelEmbeddingData>,
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}
```

### 步骤5：实现响应解析器

在 `response.rs` 中实现响应解析逻辑：

```rust
impl ResponseParser {
    /// Parse MyModel response
    fn parse_my_model(response_body: &str) -> Result<ParsedResponse, EmbedError> {
        let response: MyModelEmbeddingResponse =
            serde_json::from_str(response_body).map_err(|e| {
                EmbedError::InvalidResponse(format!("Failed to parse MyModel response: {}", e))
            })?;

        // Sort by index to ensure correct ordering
        let mut data = response.data;
        data.sort_by_key(|d| d.index);

        let embeddings: Vec<Vec<f32>> = data.into_iter().map(|d| d.embedding).collect();

        Ok(ParsedResponse {
            embeddings,
            sparse_embeddings: Vec::new(),
            colbert_embeddings: Vec::new(),
            usage: response.usage.unwrap_or_default(),
        })
    }
}
```

### 步骤6：更新响应解析器配置

在 `config.rs` 的 `ResponseParserConfig` 枚举中添加新变体：

```rust
/// Response parser configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseParserConfig {
    /// Standard OpenAI format (default)
    #[default]
    Standard,
    /// BGE-M3 multi-modal format
    BgeM3 { mode: BGEM3Mode },
    /// MyModel format
    MyModel,
}
```

### 步骤7：更新响应解析器实现

在 `response.rs` 的 `ResponseParser::parse` 方法中添加新分支：

```rust
impl ResponseParser {
    /// Parse response string according to the selected strategy
    pub fn parse(&self, response_body: &str) -> Result<ParsedResponse, EmbedError> {
        match self {
            Self::Standard => Self::parse_standard(response_body),
            Self::BGEM3(mode) => Self::parse_bge_m3(response_body, *mode),
            Self::MyModel => Self::parse_my_model(response_body),
        }
    }
}
```

### 步骤8：添加配置工厂方法

在 `config.rs` 的 `EmbedderConfig` 中添加工厂方法：

```rust
impl EmbedderConfig {
    /// Create MyModel configuration
    ///
    /// MyModel requires task-specific prefixes for optimal performance.
    ///
    /// # Example
    ///
    /// ```
    /// use code_context_engine::embedding::preprocessor::MyModelTaskType;
    /// use code_context_engine::embedding::config::EmbedderConfig;
    ///
    /// let config = EmbedderConfig::my_model(
    ///     "api-key".to_string(),
    ///     MyModelTaskType::Task1,
    /// );
    /// ```
    pub fn my_model(api_key: String, task_type: super::preprocessor::MyModelTaskType) -> Self {
        Self {
            api_keys: vec![api_key],
            base_url: "https://api.my-model.com/v1".to_string(),
            model: "my-model-v1".to_string(),
            preprocessor: PreprocessorConfig::MyModel { task_type },
            ..Default::default()
        }
    }
}
```

### 步骤9：更新HTTP嵌入器

在 `http_embedder.rs` 的 `preprocess_texts` 方法中添加新分支：

```rust
impl Embedder {
    /// Preprocess texts using the configured preprocessor
    fn preprocess_texts(&self, texts: &[&str]) -> Vec<String> {
        match &self.preprocessor {
            PreprocessorConfig::None => texts.iter().map(|s| s.to_string()).collect(),
            PreprocessorConfig::Prefix { prefix } => texts
                .iter()
                .map(|text| format!("{}{}", prefix, text))
                .collect(),
            PreprocessorConfig::Template { template } => {
                let preprocessor = TemplatePreprocessor::new(template.clone());
                preprocessor.process_batch(texts)
            }
            PreprocessorConfig::Nomic { task_type } => {
                let preprocessor = NomicPreprocessor::new(*task_type);
                preprocessor.process_batch(texts)
            }
            PreprocessorConfig::Stella { task_type } => {
                let preprocessor = StellaPreprocessor::new(*task_type);
                preprocessor.process_batch(texts)
            }
            PreprocessorConfig::MyModel { task_type } => {
                let preprocessor = MyModelPreprocessor::new(*task_type);
                preprocessor.process_batch(texts)
            }
        }
    }
}
```

### 步骤10：更新模块导出

在 `mod.rs` 中导出新类型：

```rust
pub use preprocessor::{
    ChainedPreprocessor, NomicPreprocessor, NomicTaskType, NoopPreprocessor,
    PrefixPreprocessor, StellaPreprocessor, StellaTaskType, TemplatePreprocessor,
    TextPreprocessor, MyModelPreprocessor, MyModelTaskType,
};
```

### 步骤11：编写测试

在每个相关文件中添加测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_model_preprocessor() {
        let preprocessor = MyModelPreprocessor::task1();
        assert_eq!(preprocessor.process("Hello"), "task1: Hello");
    }

    #[test]
    fn test_my_model_config() {
        let config = EmbedderConfig::my_model(
            "api-key".to_string(),
            MyModelTaskType::Task1,
        );
        assert!(config.validate().is_ok());
        assert_eq!(config.model, "my-model-v1");
    }
}
```

## 修改现有模型配置

### 修改预处理逻辑

如果需要修改现有模型的预处理逻辑：

1. 在 `preprocessor.rs` 中找到对应的预处理器
2. 修改 `process` 方法的实现
3. 更新相关测试
4. 检查是否需要更新文档

### 修改响应解析逻辑

如果需要修改现有模型的响应解析逻辑：

1. 在 `response.rs` 中找到对应的解析方法
2. 修改解析逻辑
3. 更新相关测试
4. 检查是否需要更新文档

### 修改配置参数

如果需要修改模型的配置参数：

1. 在 `config.rs` 中找到对应的工厂方法
2. 修改配置参数
3. 更新相关测试
4. 检查是否需要更新文档

## 最佳实践

### 1. 保持一致性

- 遵循现有的命名约定
- 保持代码风格一致
- 使用相同的错误处理模式

### 2. 充分测试

- 为每个新功能编写单元测试
- 测试边界条件和错误情况
- 确保测试覆盖率

### 3. 文档更新

- 更新相关文档
- 添加使用示例
- 说明模型的特殊要求

### 4. 版本控制

- 使用语义化版本
- 在变更日志中记录重要变更
- 保持向后兼容性

### 5. 性能考虑

- 避免不必要的字符串分配
- 使用高效的算法
- 考虑批处理优化

## 常见问题

### Q: 如何处理模型的多个版本？

A: 在配置中添加版本字段，根据版本选择不同的处理逻辑：

```rust
pub struct MyModelPreprocessor {
    version: String,
    task_type: MyModelTaskType,
}

impl MyModelPreprocessor {
    pub fn new(version: String, task_type: MyModelTaskType) -> Self {
        Self { version, task_type }
    }
}

impl TextPreprocessor for MyModelPreprocessor {
    fn process(&self, text: &str) -> String {
        match self.version.as_str() {
            "v1" => format!("v1:{}", text),
            "v2" => format!("v2:{}", text),
            _ => text.to_string(),
        }
    }
}
```

### Q: 如何添加自定义预处理逻辑？

A: 实现 `TextPreprocessor` trait：

```rust
pub struct CustomPreprocessor {
    // 自定义字段
}

impl TextPreprocessor for CustomPreprocessor {
    fn process(&self, text: &str) -> String {
        // 自定义处理逻辑
        text.to_uppercase()
    }
}
```

### Q: 如何处理嵌套或链式预处理？

A: 使用 `ChainedPreprocessor`：

```rust
let preprocessor = ChainedPreprocessor::new()
    .with_preprocessor(ConcretePreprocessor::Prefix(PrefixPreprocessor::new("prefix: ")))
    .with_preprocessor(ConcretePreprocessor::Template(TemplatePreprocessor::new("Query: {text}")));
```

### Q: 如何添加新的响应格式？

A: 在 `response.rs` 中定义新的响应结构体和解析方法，然后在 `ResponseParser` 枚举中添加新变体。

## 维护检查清单

### 添加新模型时

- [ ] 定义任务类型（如需要）
- [ ] 实现预处理器
- [ ] 更新预处理器配置
- [ ] 定义响应格式（如需要）
- [ ] 实现响应解析器
- [ ] 更新响应解析器配置
- [ ] 添加配置工厂方法
- [ ] 更新HTTP嵌入器
- [ ] 更新模块导出
- [ ] 编写测试
- [ ] 更新文档

### 修改现有模型时

- [ ] 定位需要修改的代码
- [ ] 进行修改
- [ ] 更新测试
- [ ] 运行测试确保没有破坏现有功能
- [ ] 更新文档
- [ ] 检查向后兼容性

## 相关文档

- [模型适配器架构设计](./model-adapter-architecture.md) - 可选的重构方案
- [embedder实现文档](./embedder-implementation.md) - 嵌入器实现细节
- [embedder增强计划](./embedder-enhancement-plan.md) - 增强功能规划

## 总结

当前架构设计合理，职责清晰，代码质量高。按照本指南添加和维护模型可以保持代码的一致性和可维护性。如果未来需要支持大量模型或频繁添加新模型，可以考虑迁移到模型适配器架构。
