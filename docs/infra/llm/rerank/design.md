# 重排模块设计文档

## 概述

本文档描述了在 code-context-engine 项目中实现重排（Re-ranking）阶段的设计方案。重排阶段位于召回（Retrieval）之后、最终排序之前，用于对初步召回的结果进行更精确的相关性评分和排序。

## 背景

当前项目已经实现了以下检索流程：
1. 向量检索（Vector Search）- 基于语义相似度
2. BM25检索 - 基于关键词匹配
3. 混合检索 - 结合向量和BM25结果
4. 摘要预过滤 - 先通过文件摘要筛选相关文件

然而，这些方法都是基于浅层语义或统计特征的召回策略，缺乏深度的相关性判断。引入重排阶段可以：
- 提高最终结果的相关性质量
- 更好地处理复杂查询意图
- 减少噪声结果的干扰

## 架构设计

### 整体流程

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌─────────────┐
│   Query     │ -> │  Retrieval   │ -> │  Re-ranking  │ -> │ Final Sort  │
│             │    │ (Recall)     │    │ (Precision)  │    │ & Filtering │
└─────────────┘    └──────────────┘    └─────────────┘    └─────────────┘
                         │                    │
                   Vector/BM25          LLM-based scoring
                   Hybrid search        Cross-encoder model
```

### 模块结构

```
src/llm/services/rerank/
├── mod.rs              # 模块入口
├── types.rs            # 类型定义
├── handler.rs          # 重排处理器
├── provider.rs         # 重排服务提供商接口
├── cross_encoder.rs    # Cross-encoder实现
└── config.rs           # 配置管理
```

## 核心组件设计

### 1. 重排服务类型定义

```rust
// src/llm/services/rerank/types.rs

use serde::{Deserialize, Serialize};

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
    pub metadata: std::collections::HashMap<String, String>,
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
```

### 2. 重排提供商接口

```rust
// src/llm/services/rerank/provider.rs

use crate::llm::services::rerank::types::{RerankRequest, RerankResult};
use crate::llm::core::error::LlmError;
use async_trait::async_trait;

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

/// Cross-encoder 重排提供商
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

impl CrossEncoderProvider {
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
        for item in parsed {
            if let Some(candidate) = request.candidates.iter().find(|c| c.id == item.id) {
                let initial_rank = request.candidates.iter().position(|c| c.id == item.id).unwrap() as i32;
                let new_rank = reranked.len() as i32;
                
                reranked.push(RerankedCandidate {
                    id: item.id,
                    rerank_score: item.score,
                    initial_score: candidate.initial_score,
                    final_score: self.calculate_final_score(
                        item.score,
                        candidate.initial_score,
                        &request.config.score_fusion_strategy
                    ),
                    rank_change: initial_rank - new_rank,
                    reasoning: if request.config.return_reasoning {
                        Some(item.reasoning)
                    } else {
                        None
                    },
                });
            }
        }
        
        // 按最终得分排序
        reranked.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal));
        
        Ok(reranked)
    }
    
    /// 计算最终得分
    fn calculate_final_score(
        &self,
        rerank_score: f32,
        initial_score: f32,
        strategy: &ScoreFusionStrategy,
    ) -> f32 {
        match strategy {
            ScoreFusionStrategy::RerankOnly => rerank_score,
            ScoreFusionStrategy::LinearWeighted { alpha } => {
                alpha * rerank_score + (1.0 - alpha) * initial_score
            }
            ScoreFusionStrategy::Multiplicative => rerank_score * initial_score,
            ScoreFusionStrategy::ReciprocalRankFusion { k } => {
                // RRF公式会在实际实现中根据排名计算
                rerank_score // 简化实现
            }
        }
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

### 3. 重排处理器

```rust
// src/llm/services/rerank/handler.rs

use crate::llm::services::rerank::provider::RerankProvider;
use crate::llm::services::rerank::types::{RerankRequest, RerankResult};
use crate::llm::core::error::LlmError;
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

### 4. 配置管理

```rust
// src/llm/services/rerank/config.rs

use serde::{Deserialize, Serialize};

/// 重排配置（TOML格式）
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
    #[serde(default)]
    pub score_fusion_strategy: String,
    
    /// 超时时间（毫秒）
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_enabled() -> bool {
    false // 默认禁用，需要时手动启用
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
```

## 集成到现有架构

### 1. 修改 Searcher 以支持重排

在 `src/orchestrator/query/searcher.rs` 中添加重排支持：

```rust
use crate::llm::services::rerank::handler::RerankRequestHandler;
use crate::llm::services::rerank::types::{RerankRequest, RerankCandidate, RerankRuntimeConfig};

pub struct Searcher {
    // ... existing fields ...
    
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
            // ... initialize other fields ...
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
            
            // 转换为重排候选
            let candidates = results.iter().map(|r| RerankCandidate {
                id: r.id.clone(),
                content: r.content.clone(),
                file_path: r.file_path.clone(),
                initial_score: r.score,
                entity_type: Some(r.kind.clone()),
                metadata: HashMap::new(),
            }).collect();
            
            // 构建重排请求
            let rerank_config = RerankRuntimeConfig {
                model: options.config.rerank_model.clone(),
                max_candidates: options.config.rerank_max_candidates,
                temperature: options.config.rerank_temperature,
                return_reasoning: options.config.rerank_return_reasoning,
                score_fusion_strategy: options.config.rerank_score_fusion.clone(),
                timeout_ms: options.config.rerank_timeout_ms,
            };
            
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
                // 可以添加重排相关信息到metadata
                result.metadata.insert("rerank_score".to_string(), reranked.rerank_score.to_string());
                result.metadata.insert("rank_change".to_string(), reranked.rank_change.to_string());
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

### 2. 在查询协调器中集成重排

在 `src/orchestrator/query/coordinator.rs` 中添加重排步骤：

```rust
pub async fn execute_query(&self, options: &QueryOptions) -> Result<QueryResult> {
    // 1. 执行检索
    let results = self.searcher.search(options).await?;
    
    // 2. 应用重排（如果启用）
    let results = self.searcher.apply_reranking(results, options).await?;
    
    // 3. 应用关系增强（如果启用）
    let results = self.relation_enhancer.enhance(results, &options.config).await?;
    
    // 4. 返回最终结果
    Ok(QueryResult {
        total: results.len(),
        items: results,
        elapsed_ms: /* calculate */,
        sources: /* collect */,
        sub_queries_count: 1,
    })
}
```

### 3. 添加配置选项

在 `src/orchestrator/query/types.rs` 的 `QueryConfig` 中添加重排相关字段：

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

## 实现方案对比

### 方案一：基于LLM的Cross-encoder重排

**优点：**
- 精度高，能理解复杂的语义关系
- 灵活，可以自定义prompt和评分标准
- 可以利用现有的LLM基础设施

**缺点：**
- 成本高，每次重排都需要调用LLM
- 速度慢，延迟较高
- Token消耗大

**适用场景：**
- 对精度要求高的场景
- 候选数量较少（<100）
- 预算充足

### 方案二：基于专用重排模型

使用专门训练的重排模型（如BGE-reranker、Cohere rerank等）

**优点：**
- 速度快，专为重排优化
- 成本低，比通用LLM便宜
- 效果好，经过专门训练

**缺点：**
- 需要额外的模型部署
- 灵活性较低
- 可能需要微调以适应代码场景

**实现示例：**

```rust
pub struct DedicatedRerankProvider {
    api_endpoint: String,
    api_key: String,
    model: String,
}

#[async_trait]
impl RerankProvider for DedicatedRerankProvider {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        // 调用专用重排API
        let response = reqwest::Client::new()
            .post(&self.api_endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({
                "model": self.model,
                "query": request.query,
                "documents": request.candidates.iter().map(|c| &c.content).collect::<Vec<_>>()
            }))
            .send()
            .await
            .map_err(|e| LlmError::request(e.to_string()))?
            .json::<RerankApiResponse>()
            .await
            .map_err(|e| LlmError::parse(e.to_string()))?;
        
        // 解析响应并返回结果
        // ...
    }
}
```

### 方案三：轻量级本地重排

使用小型本地模型（如sentence-transformers的cross-encoder）

**优点：**
- 无网络延迟
- 隐私保护好
- 成本最低

**缺点：**
- 需要本地GPU/CPU资源
- 模型效果可能不如云端
- 增加系统复杂度

## 性能优化策略

### 1. 候选筛选

在重排前先进行初步筛选，只重排最有希望的候选：

```rust
// 只重排前N个候选
let top_candidates = results.iter()
    .take(config.rerank_top_n)
    .cloned()
    .collect();
```

### 2. 批处理

将多个查询的重排请求批量处理：

```rust
pub async fn batch_rerank(
    &self,
    requests: Vec<RerankRequest>,
) -> Result<Vec<RerankResult>, LlmError> {
    // 批量发送到LLM API
    // ...
}
```

### 3. 缓存

缓存常见查询的重排结果：

```rust
use moka::future::Cache;

pub struct CachedRerankHandler {
    handler: Arc<RerankRequestHandler>,
    cache: Cache<String, RerankResult>,
}
```

### 4. 异步并行

对独立的候选进行并行评分：

```rust
use futures::future::join_all;

let futures = candidates.iter().map(|c| {
    score_candidate(query, c)
}).collect::<Vec<_>>();

let scores = join_all(futures).await;
```

## 评估指标

为了评估重排效果，建议跟踪以下指标：

1. **NDCG@K** (Normalized Discounted Cumulative Gain)
   - 衡量排序质量
   - K通常取5、10、20

2. **MRR** (Mean Reciprocal Rank)
   - 第一个相关结果的排名倒数平均值

3. **Precision@K**
   - 前K个结果中的相关比例

4. **重排前后对比**
   - 用户点击率变化
   - 平均排名变化
   - 查询满意度

## 配置文件示例

在 `config.toml` 中添加重排配置：

```toml
[rerank]
enabled = true
provider = "cross-encoder"
model = "gpt-4o-mini"
max_candidates = 50
temperature = 0.0
return_reasoning = false
score_fusion_strategy = "linear_weighted"  # rerank_only | linear_weighted | multiplicative | rrf
timeout_ms = 5000

# 查询配置中的重排选项
[query]
enable_reranking = true
rerank_top_n = 20  # 只对前20个结果进行重排
rerank_model = "gpt-4o-mini"
rerank_max_candidates = 50
rerank_temperature = 0.0
rerank_return_reasoning = false
rerank_score_fusion = "linear_weighted"
rerank_alpha = 0.7  # 用于linear_weighted策略
rerank_timeout_ms = 5000
```

## 实施路线图

### 阶段一：基础框架（1-2周）
1. 创建重排模块的基础结构
2. 实现RerankProvider trait
3. 实现CrossEncoderProvider
4. 编写单元测试

### 阶段二：集成测试（1周）
1. 集成到Searcher
2. 集成到QueryCoordinator
3. 端到端测试
4. 性能基准测试

### 阶段三：优化与调优（1-2周）
1. 实现缓存机制
2. 实现批处理
3. 调整融合策略参数
4. A/B测试框架

### 阶段四：生产部署（1周）
1. 监控和日志
2. 错误处理和降级策略
3. 文档完善
4. 灰度发布

## 风险与挑战

### 1. 性能开销
- **风险**：重排会显著增加查询延迟
- **缓解**：
  - 限制重排候选数量
  - 使用缓存
  - 异步处理
  - 提供降级选项

### 2. 成本控制
- **风险**：LLM调用成本高
- **缓解**：
  - 选择性启用（只对重要查询）
  - 使用更便宜的模型
  - 批量处理降低成本

### 3. 稳定性
- **风险**：外部API不稳定
- **缓解**：
  - 实现重试机制
  - 超时控制
  - 降级到不使用重排

### 4. 效果不确定性
- **风险**：重排可能不如预期
- **缓解**：
  - A/B测试验证效果
  - 可配置开关
  - 持续监控指标

## 总结

重排模块的引入可以显著提升code-context-engine的搜索质量，但需要权衡性能、成本和效果。建议采用渐进式实施方案：

1. 首先实现基础框架，支持多种重排提供商
2. 从小规模开始测试，逐步扩大应用范围
3. 持续监控效果，根据实际情况调整策略
4. 提供灵活的配置选项，让用户可以根据需求选择

通过合理的设计和实现，重排模块将成为提升代码搜索体验的重要组件。
