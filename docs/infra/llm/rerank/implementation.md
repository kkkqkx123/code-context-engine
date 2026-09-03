# 重排模块实现指南

## 概述

本文档提供了在 code-context-engine 项目中实现重排模块的详细步骤和代码示例。

## 目录结构

```
src/llm/services/rerank/
├── mod.rs              # 模块入口和导出
├── types.rs            # 类型定义
├── handler.rs          # 重排处理器
├── provider.rs         # 重排提供商接口和实现
├── cross_encoder.rs    # Cross-encoder具体实现
└── config.rs           # 配置管理

docs/llm/rerank/
├── design.md           # 设计文档
├── implementation.md   # 本实现指南
└── examples.md         # 使用示例
```

## 步骤1：创建模块基础结构

### 1.1 创建类型定义

文件：`src/llm/services/rerank/types.rs`

```rust
//! 重排服务类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 重排请求
#[derive(Debug, Clone)]
pub struct RerankRequest {
    /// 原始查询文本
    pub query: String,
    /// 待重排的候选结果列表
    pub candidates: Vec<RerankCandidate>,
    /// 重排配置
    pub config: RerankRuntimeConfig,
}

/// 重排候选项
#[derive(Debug, Clone)]
pub struct RerankCandidate {
    /// 候选ID
    pub id: String,
    /// 候选内容（代码片段或文本）
    pub content: String,
    /// 文件路径
    pub file_path: String,
    /// 初始得分（来自召回阶段）
    pub initial_score: f32,
    /// 实体类型（function/class等）
    pub entity_type: Option<String>,
    /// 其他元数据
    pub metadata: HashMap<String, String>,
}

/// 重排结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankResult {
    /// 重排后的候选列表（按新得分排序）
    pub reranked_candidates: Vec<RerankedCandidate>,
    /// 使用的token数量
    pub prompt_tokens: u64,
    /// 总token数量
    pub total_tokens: u64,
    /// 重排耗时（毫秒）
    pub elapsed_ms: u64,
}

/// 重排后的候选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankedCandidate {
    /// 候选ID
    pub id: String,
    /// 重排后的得分
    pub rerank_score: f32,
    /// 原始得分
    pub initial_score: f32,
    /// 综合得分（可能结合原始得分和重排得分）
    pub final_score: f32,
    /// 排名变化
    pub rank_change: i32,
    /// 重排理由（可选，用于调试）
    pub reasoning: Option<String>,
}

/// 重排配置
#[derive(Debug, Clone)]
pub struct RerankRuntimeConfig {
    /// 重排模型名称
    pub model: String,
    /// 最大重排候选数（避免过多调用LLM）
    pub max_candidates: usize,
    /// 温度参数
    pub temperature: f32,
    /// 是否返回重排理由
    pub return_reasoning: bool,
    /// 得分融合策略
    pub score_fusion_strategy: ScoreFusionStrategy,
    /// 超时时间（毫秒）
    pub timeout_ms: u64,
}

/// 得分融合策略
#[derive(Debug, Clone, PartialEq)]
pub enum ScoreFusionStrategy {
    /// 仅使用重排得分
    RerankOnly,
    /// 线性加权：final = α * rerank + (1-α) * initial
    LinearWeighted { alpha: f32 },
    /// 乘法融合：final = rerank * initial
    Multiplicative,
    /// 倒数排名融合（RRF）
    ReciprocalRankFusion { k: f32 },
}

impl Default for RerankRuntimeConfig {
    fn default() -> Self {
        Self {
            model: "cross-encoder".to_string(),
            max_candidates: 50,
            temperature: 0.0,
            return_reasoning: false,
            score_fusion_strategy: ScoreFusionStrategy::LinearWeighted { alpha: 0.7 },
            timeout_ms: 5000,
        }
    }
}

impl ScoreFusionStrategy {
    /// 计算最终得分
    pub fn calculate(&self, rerank_score: f32, initial_score: f32, rank: usize) -> f32 {
        match self {
            ScoreFusionStrategy::RerankOnly => rerank_score,
            ScoreFusionStrategy::LinearWeighted { alpha } => {
                alpha * rerank_score + (1.0 - alpha) * initial_score
            }
            ScoreFusionStrategy::Multiplicative => rerank_score * initial_score,
            ScoreFusionStrategy::ReciprocalRankFusion { k } => {
                // RRF公式: 1 / (k + rank)
                let rrf_score = 1.0 / (*k + rank as f32);
                rrf_score
            }
        }
    }
}
```

### 1.2 创建配置模块

文件：`src/llm/services/rerank/config.rs`

```rust
//! 重排配置管理

use serde::{Deserialize, Serialize};

/// 重排服务配置（TOML格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankServiceConfig {
    /// 是否启用重排
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    
    /// 重排模型提供商
    #[serde(default = "default_provider")]
    pub provider: String,
    
    /// 模型名称
    #[serde(default = "default_model")]
    pub model: String,
    
    /// 最大候选数
    #[serde(default = "default_max_candidates")]
    pub max_candidates: usize,
    
    /// 温度参数
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    
    /// 是否返回重排理由
    #[serde(default)]
    pub return_reasoning: bool,
    
    /// 得分融合策略
    #[serde(default = "default_score_fusion")]
    pub score_fusion_strategy: String,
    
    /// 超时时间（毫秒）
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_enabled() -> bool {
    false
}

fn default_provider() -> String {
    "cross-encoder".to_string()
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_max_candidates() -> usize {
    50
}

fn default_temperature() -> f32 {
    0.0
}

fn default_score_fusion() -> String {
    "linear_weighted".to_string()
}

fn default_timeout_ms() -> u64 {
    5000
}

impl Default for RerankServiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "cross-encoder".to_string(),
            model: "gpt-4o-mini".to_string(),
            max_candidates: 50,
            temperature: 0.0,
            return_reasoning: false,
            score_fusion_strategy: "linear_weighted".to_string(),
            timeout_ms: 5000,
        }
    }
}

impl RerankServiceConfig {
    /// 从配置转换为RerankRuntimeConfig
    pub fn to_rerank_config(&self) -> crate::llm::services::rerank::types::RerankRuntimeConfig {
        use crate::llm::services::rerank::types::{RerankRuntimeConfig, ScoreFusionStrategy};
        
        let fusion_strategy = match self.score_fusion_strategy.as_str() {
            "rerank_only" => ScoreFusionStrategy::RerankOnly,
            "linear_weighted" => ScoreFusionStrategy::LinearWeighted { alpha: 0.7 },
            "multiplicative" => ScoreFusionStrategy::Multiplicative,
            "rrf" => ScoreFusionStrategy::ReciprocalRankFusion { k: 60.0 },
            _ => ScoreFusionStrategy::LinearWeighted { alpha: 0.7 },
        };
        
        RerankRuntimeConfig {
            model: self.model.clone(),
            max_candidates: self.max_candidates,
            temperature: self.temperature,
            return_reasoning: self.return_reasoning,
            score_fusion_strategy: fusion_strategy,
            timeout_ms: self.timeout_ms,
        }
    }
}
```

### 1.3 创建提供商接口

文件：`src/llm/services/rerank/provider.rs`

```rust
//! 重排提供商接口和实现

use crate::llm::core::client::LlmClient;
use crate::llm::core::config::ChatConfig;
use crate::llm::core::error::LlmError;
use crate::llm::services::chat::types::Message;
use crate::llm::services::rerank::types::{RerankRequest, RerankResult, RerankedCandidate, ScoreFusionStrategy};
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

/// 重排提供商 trait
#[async_trait]
pub trait RerankProvider: Send + Sync {
    /// 执行重排
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError>;
    
    /// 获取提供商名称
    fn provider_name(&self) -> &str;
    
    /// 检查提供商是否可用
    fn is_available(&self) -> bool;
}

/// Cross-encoder 重排提供商（基于LLM）
pub struct CrossEncoderProvider {
    client: Arc<LlmClient>,
    model_name: String,
}

impl CrossEncoderProvider {
    pub fn new(client: Arc<LlmClient>, model_name: String) -> Self {
        Self {
            client,
            model_name,
        }
    }
    
    /// 构建交叉编码器prompt
    fn build_cross_encoder_prompt(&self, request: &RerankRequest) -> String {
        let query = &request.query;
        let candidates = &request.candidates;
        
        let mut prompt = format!(
            "You are a code search relevance evaluator. Given a query and multiple code snippets, \
             evaluate the relevance of each snippet to the query on a scale of 0.0 to 1.0.\n\n\
             Query: {}\n\n\
             Code Snippets:\n",
            query
        );
        
        for (i, candidate) in candidates.iter().enumerate() {
            prompt.push_str(&format!(
                "[{}] ID: {}\nFile: {}\nType: {}\nContent:\n{}\n\n",
                i,
                candidate.id,
                candidate.file_path,
                candidate.entity_type.as_deref().unwrap_or("unknown"),
                truncate_content(&candidate.content, 500)
            ));
        }
        
        prompt.push_str(
            "Please output a JSON array with the following structure for each candidate:\n\
             [{\"id\": \"...\", \"score\": 0.0-1.0, \"reasoning\": \"...\"}]\n\
             Sort by score in descending order."
        );
        
        prompt
    }
    
    /// 解析重排响应
    fn parse_rerank_response(
        &self,
        response: &str,
        request: &RerankRequest,
    ) -> Result<Vec<RerankedCandidate>, LlmError> {
        // 尝试解析JSON响应
        let parsed: Vec<RerankResponseItem> = serde_json::from_str(response)
            .map_err(|e| LlmError::parse(format!("Failed to parse rerank response: {}", e)))?;
        
        // 构建重排后的候选列表
        let mut reranked = Vec::new();
        for (new_rank, item) in parsed.iter().enumerate() {
            if let Some(candidate) = request.candidates.iter().find(|c| c.id == item.id) {
                let initial_rank = request.candidates.iter()
                    .position(|c| c.id == item.id)
                    .unwrap_or(0) as i32;
                
                reranked.push(RerankedCandidate {
                    id: item.id.clone(),
                    rerank_score: item.score,
                    initial_score: candidate.initial_score,
                    final_score: request.config.score_fusion_strategy.calculate(
                        item.score,
                        candidate.initial_score,
                        new_rank
                    ),
                    rank_change: initial_rank - new_rank as i32,
                    reasoning: if request.config.return_reasoning {
                        Some(item.reasoning.clone())
                    } else {
                        None
                    },
                });
            }
        }
        
        // 按最终得分排序
        reranked.sort_by(|a, b| {
            b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        Ok(reranked)
    }
}

#[async_trait]
impl RerankProvider for CrossEncoderProvider {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        // 构建交叉编码器的prompt
        let prompt = self.build_cross_encoder_prompt(request);
        
        // 调用LLM进行重排
        let messages = vec![Message::user(prompt)];
        let chat_config = ChatConfig {
            model: self.model_name.clone(),
            temperature: request.config.temperature,
            max_tokens: 2000,
            ..Default::default()
        };
        
        let start = std::time::Instant::now();
        let result = self.client.chat(&messages, &chat_config).await?;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        
        // 解析LLM返回的重排结果
        let reranked_candidates = self.parse_rerank_response(&result.content, request)?;
        
        Ok(RerankResult {
            reranked_candidates,
            prompt_tokens: result.prompt_tokens,
            total_tokens: result.total_tokens,
            elapsed_ms,
        })
    }
    
    fn provider_name(&self) -> &str {
        "cross-encoder"
    }
    
    fn is_available(&self) -> bool {
        true
    }
}

#[derive(Debug, Deserialize)]
struct RerankResponseItem {
    id: String,
    score: f32,
    reasoning: String,
}

/// 截断内容以避免超出token限制
fn truncate_content(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        content.to_string()
    } else {
        format!("{}...", &content[..max_chars])
    }
}
```

### 1.4 创建重排处理器

文件：`src/llm/services/rerank/handler.rs`

```rust
//! 重排请求处理器

use crate::llm::core::error::LlmError;
use crate::llm::services::rerank::provider::RerankProvider;
use crate::llm::services::rerank::types::{RerankRequest, RerankResult};
use std::sync::Arc;

/// 重排请求处理器
pub struct RerankRequestHandler {
    /// 重排提供商
    provider: Arc<dyn RerankProvider>,
}

impl RerankRequestHandler {
    pub fn new(provider: Arc<dyn RerankProvider>) -> Self {
        Self { provider }
    }
    
    /// 执行重排
    pub async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        // 验证输入
        self.validate_request(request)?;
        
        // 限制候选数量
        let limited_request = self.limit_candidates(request);
        
        // 调用提供商执行重排
        self.provider.rerank(&limited_request).await
    }
    
    /// 验证请求
    fn validate_request(&self, request: &RerankRequest) -> Result<(), LlmError> {
        if request.query.is_empty() {
            return Err(LlmError::parse("Query cannot be empty".to_string()));
        }
        
        if request.candidates.is_empty() {
            return Err(LlmError::parse("Candidates cannot be empty".to_string()));
        }
        
        if request.candidates.len() > request.config.max_candidates {
            return Err(LlmError::parse(format!(
                "Too many candidates: {} (max: {})",
                request.candidates.len(),
                request.config.max_candidates
            )));
        }
        
        Ok(())
    }
    
    /// 限制候选数量
    fn limit_candidates<'a>(&self, request: &'a RerankRequest) -> RerankRequest {
        if request.candidates.len() <= request.config.max_candidates {
            return request.clone();
        }
        
        // 按初始得分排序并取前N个
        let mut sorted_candidates = request.candidates.clone();
        sorted_candidates.sort_by(|a, b| {
            b.initial_score.partial_cmp(&a.initial_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        let limited_candidates = sorted_candidates.into_iter()
            .take(request.config.max_candidates)
            .collect();
        
        RerankRequest {
            query: request.query.clone(),
            candidates: limited_candidates,
            config: request.config.clone(),
        }
    }
}
```

### 1.5 创建模块入口

文件：`src/llm/services/rerank/mod.rs`

```rust
//! 重排服务模块
//!
//! 提供对召回结果的重排功能，使用LLM或专用模型对候选结果进行更精确的相关性评分。

pub mod config;
pub mod handler;
pub mod provider;
pub mod types;

// 重新导出常用类型
pub use config::RerankServiceConfig;
pub use handler::RerankRequestHandler;
pub use provider::{CrossEncoderProvider, RerankProvider};
pub use types::{
    RerankCandidate, RerankRuntimeConfig, RerankRequest, RerankResult, RerankedCandidate,
    ScoreFusionStrategy,
};
```

## 步骤2：更新LLM服务模块

文件：`src/llm/services/mod.rs`

```rust
//! LLM Services Module
//!
//! Provides service-level abstractions for LLM operations.
//! This layer handles business logic like preprocessing, metrics, and request orchestration.

pub mod chat;
pub mod embedding;
pub mod rerank;  // 添加这一行
```

## 步骤3：集成到Searcher

### 3.1 更新QueryConfig

文件：`src/orchestrator/query/types.rs`

在 `QueryConfig` 结构体中添加重排相关字段：

```rust
#[derive(Debug, Clone)]
pub struct QueryConfig {
    // ... existing fields ...
    
    /// 是否启用重排
    pub enable_reranking: bool,
    
    /// 重排模型名称
    pub rerank_model: String,
    
    /// 最大重排候选数
    pub rerank_max_candidates: usize,
    
    /// 重排温度参数
    pub rerank_temperature: f32,
    
    /// 是否返回重排理由
    pub rerank_return_reasoning: bool,
    
    /// 重排得分融合策略
    pub rerank_score_fusion: ScoreFusionStrategy,
    
    /// 重排超时时间（毫秒）
    pub rerank_timeout_ms: u64,
}
```

同时需要导入 `ScoreFusionStrategy`：

```rust
use crate::llm::services::rerank::types::ScoreFusionStrategy;
```

### 3.2 更新Searcher结构

文件：`src/orchestrator/query/searcher.rs`

```rust
use crate::llm::services::rerank::handler::RerankRequestHandler;
use crate::llm::services::rerank::types::{RerankRequest, RerankCandidate, RerankRuntimeConfig};

pub struct Searcher {
    vector_retrieval: VectorRetrieval,
    bm25_retrieval: Bm25Retrieval,
    qdrant: Arc<QdrantClient>,
    embedder: Arc<OpenAICompatibleProvider>,
    bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
    sqlite: Option<Arc<SqliteDatabase>>,
    result_processor: Arc<ResultProcessor>,
    assembly_handler: Option<Arc<AssemblyHandler>>,
    /// 可选的重排处理器
    rerank_handler: Option<Arc<RerankRequestHandler>>,
}

impl Searcher {
    /// 创建带重排支持的searcher
    pub fn with_rerank(
        qdrant: Arc<QdrantClient>,
        embedder: Arc<OpenAICompatibleProvider>,
        bm25: Arc<tokio::sync::Mutex<Bm25Client>>,
        rerank_handler: Arc<RerankRequestHandler>,
    ) -> Self {
        Self {
            vector_retrieval: VectorRetrieval::new(),
            bm25_retrieval: Bm25Retrieval::new(),
            qdrant,
            embedder,
            bm25,
            sqlite: None,
            result_processor: Arc::new(ResultProcessor::new()),
            assembly_handler: None,
            rerank_handler: Some(rerank_handler),
        }
    }
    
    /// 在搜索结果处理后应用重排
    async fn apply_reranking(
        &self,
        results: Vec<SearchResult>,
        options: &QueryOptions,
    ) -> Result<Vec<SearchResult>> {
        if let Some(ref handler) = self.rerank_handler {
            if !options.config.enable_reranking {
                return Ok(results);
            }
            
            if results.is_empty() {
                return Ok(results);
            }
            
            // 转换为重排候选
            let candidates = results.iter().map(|r| RerankCandidate {
                id: r.id.clone(),
                content: r.content.clone(),
                file_path: r.file_path.clone(),
                initial_score: r.score,
                entity_type: Some(r.kind.clone()),
                metadata: HashMap::new(),
            }).collect();
            
            // 构建重排配置
            let rerank_config = RerankRuntimeConfig {
                model: options.config.rerank_model.clone(),
                max_candidates: options.config.rerank_max_candidates,
                temperature: options.config.rerank_temperature,
                return_reasoning: options.config.rerank_return_reasoning,
                score_fusion_strategy: options.config.rerank_score_fusion.clone(),
                timeout_ms: options.config.rerank_timeout_ms,
            };
            
            // 构建重排请求
            let request = RerankRequest {
                query: options.query.clone(),
                candidates,
                config: rerank_config,
            };
            
            // 执行重排
            let rerank_result = handler.rerank(&request).await
                .map_err(|e| QueryError::Rerank(format!("Reranking failed: {}", e)))?;
            
            // 将重排结果映射回SearchResult
            let reranked_results = self.merge_rerank_results(results, rerank_result);
            
            tracing::info!(
                "Reranking completed: {} candidates processed in {}ms",
                rerank_result.reranked_candidates.len(),
                rerank_result.elapsed_ms
            );
            
            Ok(reranked_results)
        } else {
            Ok(results)
        }
    }
    
    /// 合并重排结果到原始搜索结果
    fn merge_rerank_results(
        &self,
        original_results: Vec<SearchResult>,
        rerank_result: RerankResult,
    ) -> Vec<SearchResult> {
        // 创建ID到重排信息的映射
        let rerank_map: HashMap<String, RerankedCandidate> = rerank_result
            .reranked_candidates
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect();
        
        // 更新原始结果的得分
        let mut updated_results = original_results;
        for result in &mut updated_results {
            if let Some(reranked) = rerank_map.get(&result.id) {
                result.score = reranked.final_score;
                result.original_score = reranked.initial_score;
                // 添加重排相关信息到metadata
                result.metadata.insert(
                    "rerank_score".to_string(),
                    reranked.rerank_score.to_string()
                );
                result.metadata.insert(
                    "rank_change".to_string(),
                    reranked.rank_change.to_string()
                );
                if let Some(ref reasoning) = reranked.reasoning {
                    result.metadata.insert(
                        "rerank_reasoning".to_string(),
                        reasoning.clone()
                    );
                }
            }
        }
        
        // 按新得分重新排序
        updated_results.sort_by(|a, b| {
            b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        updated_results
    }
}
```

### 3.3 在搜索流程中调用重排

修改 `search_vector_enhanced` 等方法，在BM25增强后、最终排序前调用重排：

```rust
async fn search_vector_enhanced(&self, options: &QueryOptions) -> Result<Vec<SearchResult>> {
    let results = if options.config.enable_summary_pre_filter {
        // Hierarchical flow: summary -> detail
        self.search_with_summary_filter(options).await?
    } else {
        // Standard flow: direct vector retrieval
        self.search_vector(options).await?
    };

    // BM25 enhancement
    let enhanced_results = self.enhance_with_bm25(results, options).await?;

    // Apply reranking (NEW)
    let reranked_results = self.apply_reranking(enhanced_results, options).await?;

    // Apply diversity control and ranking
    let ranked_results = self
        .result_processor
        .rank_and_diversify(reranked_results, &options.config);

    // Apply post-search filters
    let filtered_results = self
        .result_processor
        .apply_post_filters(ranked_results, options);

    // Apply thresholds and limits
    self.result_processor
        .apply_thresholds(filtered_results, &options.config)
}
```

对其他搜索方法（`search_vector_only`、`search_bm25_only` 等）也做类似修改。

## 步骤4：添加错误类型

文件：`src/orchestrator/query/error.rs`

在 `QueryError` 枚举中添加重排相关错误：

```rust
#[derive(Error, Debug)]
pub enum QueryError {
    // ... existing variants ...
    
    #[error("Reranking error: {0}")]
    Rerank(String),
}
```

## 步骤5：更新配置加载

在主配置文件加载逻辑中添加重排配置的解析。

文件：`src/config/mod.rs` 或相关配置文件

```rust
use crate::llm::services::rerank::config::RerankServiceConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    // ... existing fields ...
    
    #[serde(default)]
    pub rerank: RerankServiceConfig,
}
```

## 步骤6：编写测试

文件：`tests/integration_rerank.rs`

```rust
//! 重排模块集成测试

use code_context_engine::llm::services::rerank::{
    RerankRequest, RerankCandidate, RerankRuntimeConfig, ScoreFusionStrategy,
};

#[tokio::test]
async fn test_rerank_basic() {
    // 这个测试需要实际的LLM客户端
    // 可以使用mock或者跳过
}

#[test]
fn test_score_fusion_strategies() {
    let strategies = vec![
        ScoreFusionStrategy::RerankOnly,
        ScoreFusionStrategy::LinearWeighted { alpha: 0.7 },
        ScoreFusionStrategy::Multiplicative,
        ScoreFusionStrategy::ReciprocalRankFusion { k: 60.0 },
    ];
    
    for strategy in strategies {
        let final_score = strategy.calculate(0.9, 0.8, 0);
        assert!(final_score >= 0.0 && final_score <= 1.0);
    }
}

#[test]
fn test_truncate_content() {
    use code_context_engine::llm::services::rerank::provider::truncate_content;
    
    let short = "short text";
    assert_eq!(truncate_content(short, 100), short);
    
    let long = "a".repeat(1000);
    let truncated = truncate_content(&long, 100);
    assert_eq!(truncated.len(), 103); // 100 chars + "..."
    assert!(truncated.ends_with("..."));
}
```

## 步骤7：更新文档

在 `docs/api/` 目录下添加重排API文档。

文件：`docs/api/rerank.md`

```markdown
# 重排API

## 概述

重排API提供对召回结果的二次排序功能，使用LLM或专用模型对候选结果进行更精确的相关性评分。

## 配置

在 `config.toml` 中配置重排服务：

```toml
[rerank]
enabled = true
provider = "cross-encoder"
model = "gpt-4o-mini"
max_candidates = 50
temperature = 0.0
return_reasoning = false
score_fusion_strategy = "linear_weighted"
timeout_ms = 5000
```

## 使用示例

```rust
use code_context_engine::llm::services::rerank::{
    RerankRequest, RerankCandidate, RerankRuntimeConfig, RerankRequestHandler, CrossEncoderProvider
};

// 创建重排处理器
let provider = Arc::new(CrossEncoderProvider::new(llm_client, "gpt-4o-mini".to_string()));
let handler = RerankRequestHandler::new(provider);

// 构建重排请求
let candidates = vec![
    RerankCandidate {
        id: "func:main".to_string(),
        content: "fn main() { ... }".to_string(),
        file_path: "src/main.rs".to_string(),
        initial_score: 0.85,
        entity_type: Some("function".to_string()),
        metadata: HashMap::new(),
    },
    // ... more candidates
];

let config = RerankRuntimeConfig::default();
let request = RerankRequest {
    query: "how to start the application".to_string(),
    candidates,
    config,
};

// 执行重排
let result = handler.rerank(&request).await?;

// 处理结果
for candidate in result.reranked_candidates {
    println!("ID: {}, Final Score: {}", candidate.id, candidate.final_score);
}
```

## API参考

### RerankRequest

重排请求结构。

**字段：**
- `query`: String - 原始查询文本
- `candidates`: Vec<RerankCandidate> - 待重排的候选列表
- `config`: RerankRuntimeConfig - 重排配置

### RerankResult

重排结果结构。

**字段：**
- `reranked_candidates`: Vec<RerankedCandidate> - 重排后的候选列表
- `prompt_tokens`: u64 - 使用的prompt token数
- `total_tokens`: u64 - 总token数
- `elapsed_ms`: u64 - 重排耗时（毫秒）

### ScoreFusionStrategy

得分融合策略枚举。

**变体：**
- `RerankOnly` - 仅使用重排得分
- `LinearWeighted { alpha: f32 }` - 线性加权融合
- `Multiplicative` - 乘法融合
- `ReciprocalRankFusion { k: f32 }` - 倒数排名融合
```

## 步骤8：性能优化

### 8.1 实现缓存

文件：`src/llm/services/rerank/cache.rs`

```rust
//! 重排结果缓存

use moka::future::Cache;
use std::sync::Arc;
use crate::llm::services::rerank::types::{RerankRequest, RerankResult};

/// 重排缓存
pub struct RerankCache {
    cache: Cache<String, Arc<RerankResult>>,
}

impl RerankCache {
    pub fn new(max_capacity: u64, ttl_seconds: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(std::time::Duration::from_secs(ttl_seconds))
            .build();
        
        Self { cache }
    }
    
    /// 生成缓存键
    fn generate_key(request: &RerankRequest) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        request.query.hash(&mut hasher);
        request.candidates.len().hash(&mut hasher);
        
        // 为每个候选的ID和初始得分哈希
        for candidate in &request.candidates {
            candidate.id.hash(&mut hasher);
            candidate.initial_score.to_bits().hash(&mut hasher);
        }
        
        format!("{:x}", hasher.finish())
    }
    
    /// 从缓存获取结果
    pub async fn get(&self, request: &RerankRequest) -> Option<Arc<RerankResult>> {
        let key = Self::generate_key(request);
        self.cache.get(&key).await
    }
    
    /// 将结果存入缓存
    pub async fn insert(&self, request: &RerankRequest, result: RerankResult) {
        let key = Self::generate_key(request);
        self.cache.insert(key, Arc::new(result)).await;
    }
    
    /// 清除缓存
    pub fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }
}
```

### 8.2 实现批处理

在 `RerankRequestHandler` 中添加批处理方法：

```rust
impl RerankRequestHandler {
    /// 批量重排
    pub async fn batch_rerank(
        &self,
        requests: Vec<RerankRequest>,
    ) -> Result<Vec<RerankResult>, LlmError> {
        use futures::future::join_all;
        
        let futures = requests.iter().map(|req| {
            self.rerank(req)
        }).collect::<Vec<_>>();
        
        let results = join_all(futures).await;
        
        // 收集结果，过滤掉错误
        results.into_iter().collect::<Result<Vec<_>, _>>()
    }
}
```

## 步骤9：监控和日志

在重排过程中添加详细的日志和指标：

```rust
// 在 RerankRequestHandler::rerank 中
tracing::info!(
    query_length = request.query.len(),
    candidate_count = request.candidates.len(),
    model = request.config.model,
    "Starting reranking"
);

let start = std::time::Instant::now();
let result = self.provider.rerank(&limited_request).await?;
let elapsed = start.elapsed();

tracing::info!(
    elapsed_ms = elapsed.as_millis(),
    candidates_processed = result.reranked_candidates.len(),
    prompt_tokens = result.prompt_tokens,
    total_tokens = result.total_tokens,
    "Reranking completed"
);

// 记录指标
crate::metrics::record_rerank_duration(elapsed);
crate::metrics::record_rerank_tokens(result.total_tokens);
```

## 总结

按照以上步骤，你可以完整地实现重排模块并将其集成到 code-context-engine 项目中。关键点：

1. **模块化设计**：将重排功能独立为服务层模块
2. **灵活的提供商接口**：支持多种重排实现
3. **可配置的融合策略**：允许用户根据需求调整得分计算方式
4. **性能优化**：通过缓存、批处理等手段降低延迟
5. **完善的错误处理**：确保重排失败时能优雅降级

实施建议：
- 先在测试环境中验证效果
- 从小规模开始，逐步扩大应用范围
- 持续监控性能和效果指标
- 根据实际使用情况调整配置参数
