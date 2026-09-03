# 重排模块使用示例

## 目录

- [基础用法](#基础用法)
- [高级配置](#高级配置)
- [与Searcher集成](#与searcher集成)
- [自定义重排提供商](#自定义重排提供商)
- [性能优化示例](#性能优化示例)
- [故障排除](#故障排除)

## 基础用法

### 示例1：简单的重排调用

```rust
use std::sync::Arc;
use code_context_engine::llm::{LlmClient, LlmConfig};
use code_context_engine::llm::services::rerank::{
    RerankRequest, RerankCandidate, RerankRuntimeConfig,
    RerankRequestHandler, CrossEncoderProvider,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建LLM客户端
    let config = LlmConfig::openai("sk-your-api-key".to_string());
    let llm_client = Arc::new(LlmClient::new(config)?);
    
    // 2. 创建重排提供商
    let provider = Arc::new(CrossEncoderProvider::new(
        llm_client.clone(),
        "gpt-4o-mini".to_string()
    ));
    
    // 3. 创建重排处理器
    let handler = RerankRequestHandler::new(provider);
    
    // 4. 准备候选结果
    let candidates = vec![
        RerankCandidate {
            id: "func:main".to_string(),
            content: r#"
fn main() {
    println!("Hello, world!");
    let app = App::new();
    app.run();
}
            "#.to_string(),
            file_path: "src/main.rs".to_string(),
            initial_score: 0.85,
            entity_type: Some("function".to_string()),
            metadata: std::collections::HashMap::new(),
        },
        RerankCandidate {
            id: "struct:App".to_string(),
            content: r#"
pub struct App {
    name: String,
    version: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            name: "MyApp".to_string(),
            version: "1.0.0".to_string(),
        }
    }
    
    pub fn run(&self) {
        println!("Running {} v{}", self.name, self.version);
    }
}
            "#.to_string(),
            file_path: "src/app.rs".to_string(),
            initial_score: 0.78,
            entity_type: Some("struct".to_string()),
            metadata: std::collections::HashMap::new(),
        },
        RerankCandidate {
            id: "mod:utils".to_string(),
            content: r#"
pub mod utils {
    pub fn format_message(msg: &str) -> String {
        format!("[INFO] {}", msg)
    }
}
            "#.to_string(),
            file_path: "src/utils/mod.rs".to_string(),
            initial_score: 0.65,
            entity_type: Some("module".to_string()),
            metadata: std::collections::HashMap::new(),
        },
    ];
    
    // 5. 配置重排参数
    let config = RerankRuntimeConfig {
        model: "gpt-4o-mini".to_string(),
        max_candidates: 50,
        temperature: 0.0,
        return_reasoning: true,
        score_fusion_strategy: code_context_engine::llm::services::rerank::types::ScoreFusionStrategy::LinearWeighted {
            alpha: 0.7
        },
        timeout_ms: 5000,
    };
    
    // 6. 构建重排请求
    let request = RerankRequest {
        query: "how to initialize and run the application".to_string(),
        candidates,
        config,
    };
    
    // 7. 执行重排
    println!("Executing reranking...");
    let result = handler.rerank(&request).await?;
    
    // 8. 处理结果
    println!("\nReranking Results:");
    println!("Processed {} candidates in {}ms", 
             result.reranked_candidates.len(),
             result.elapsed_ms);
    println!("Token usage: {} prompt, {} total\n",
             result.prompt_tokens,
             result.total_tokens);
    
    for (rank, candidate) in result.reranked_candidates.iter().enumerate() {
        println!("Rank {}: ID={}", rank + 1, candidate.id);
        println!("  Initial Score: {:.3}", candidate.initial_score);
        println!("  Rerank Score: {:.3}", candidate.rerank_score);
        println!("  Final Score: {:.3}", candidate.final_score);
        println!("  Rank Change: {}", candidate.rank_change);
        if let Some(ref reasoning) = candidate.reasoning {
            println!("  Reasoning: {}", reasoning);
        }
        println!();
    }
    
    Ok(())
}
```

### 示例2：不同的得分融合策略

```rust
use code_context_engine::llm::services::rerank::types::ScoreFusionStrategy;

// 策略1：仅使用重排得分
let strategy1 = ScoreFusionStrategy::RerankOnly;
let final_score1 = strategy1.calculate(0.9, 0.8, 0);
println!("RerankOnly: {:.3}", final_score1); // 输出: 0.900

// 策略2：线性加权（70%重排 + 30%初始）
let strategy2 = ScoreFusionStrategy::LinearWeighted { alpha: 0.7 };
let final_score2 = strategy2.calculate(0.9, 0.8, 0);
println!("LinearWeighted: {:.3}", final_score2); // 输出: 0.870

// 策略3：乘法融合
let strategy3 = ScoreFusionStrategy::Multiplicative;
let final_score3 = strategy3.calculate(0.9, 0.8, 0);
println!("Multiplicative: {:.3}", final_score3); // 输出: 0.720

// 策略4：倒数排名融合
let strategy4 = ScoreFusionStrategy::ReciprocalRankFusion { k: 60.0 };
let final_score4 = strategy4.calculate(0.9, 0.8, 0);
println!("RRF: {:.3}", final_score4); // 输出: 0.016
```

## 高级配置

### 示例3：从配置文件加载重排配置

```toml
# config.toml

[rerank]
enabled = true
provider = "cross-encoder"
model = "gpt-4o-mini"
max_candidates = 30
temperature = 0.0
return_reasoning = false
score_fusion_strategy = "linear_weighted"
timeout_ms = 5000

[query]
enable_reranking = true
rerank_model = "gpt-4o-mini"
rerank_max_candidates = 30
rerank_temperature = 0.0
rerank_return_reasoning = false
rerank_score_fusion = "linear_weighted"
rerank_alpha = 0.7
rerank_timeout_ms = 5000
```

```rust
use code_context_engine::config::AppConfig;
use code_context_engine::llm::services::rerank::RerankServiceConfig;

// 加载配置
let config_str = std::fs::read_to_string("config.toml")?;
let app_config: AppConfig = toml::from_str(&config_str)?;

// 获取重排配置
let rerank_config = &app_config.rerank;

if rerank_config.enabled {
    println!("Reranking is enabled");
    println!("Model: {}", rerank_config.model);
    println!("Max candidates: {}", rerank_config.max_candidates);
    
    // 转换为运行时配置
    let runtime_config = rerank_config.to_rerank_config();
    
    // 使用配置创建重排处理器
    let provider = Arc::new(CrossEncoderProvider::new(
        llm_client,
        rerank_config.model.clone()
    ));
    let handler = RerankRequestHandler::new(provider);
}
```

### 示例4：条件启用重排

```rust
// 只对特定类型的查询启用重排
fn should_enable_reranking(query: &str, result_count: usize) -> bool {
    // 条件1：查询长度足够（避免对太短的查询重排）
    if query.len() < 10 {
        return false;
    }
    
    // 条件2：结果数量在合理范围内
    if result_count == 0 || result_count > 100 {
        return false;
    }
    
    // 条件3：查询包含特定关键词
    let important_keywords = ["how", "implement", "create", "build", "design"];
    let query_lower = query.to_lowercase();
    important_keywords.iter().any(|kw| query_lower.contains(kw))
}

// 使用示例
let enable_rerank = should_enable_reranking(&query, results.len());

let mut config = base_config.clone();
config.enable_reranking = enable_rerank;

if enable_rerank {
    println!("Reranking enabled for query: {}", query);
} else {
    println!("Reranking skipped for query: {}", query);
}
```

## 与Searcher集成

### 示例5：创建带重排支持的Searcher

```rust
use std::sync::Arc;
use code_context_engine::orchestrator::query::Searcher;
use code_context_engine::storage::{QdrantClient, Bm25Client, SqliteDatabase};
use code_context_engine::llm::OpenAICompatibleProvider;
use code_context_engine::llm::services::rerank::{
    RerankRequestHandler, CrossEncoderProvider, RerankServiceConfig
};

async fn create_searcher_with_rerank() -> Result<Searcher, Box<dyn std::error::Error>> {
    // 1. 初始化各个组件
    let qdrant = Arc::new(QdrantClient::new(/* config */)?);
    let embedder = Arc::new(OpenAICompatibleProvider::new(/* config */)?);
    let bm25 = Arc::new(tokio::sync::Mutex::new(Bm25Client::new(/* config */)?));
    let sqlite = Arc::new(SqliteDatabase::new(/* config */)?);
    
    // 2. 创建LLM客户端
    let llm_config = code_context_engine::llm::LlmConfig::openai(
        std::env::var("OPENAI_API_KEY")?
    );
    let llm_client = Arc::new(code_context_engine::llm::LlmClient::new(llm_config)?);
    
    // 3. 创建重排处理器
    let rerank_config = RerankServiceConfig {
        enabled: true,
        model: "gpt-4o-mini".to_string(),
        max_candidates: 30,
        ..Default::default()
    };
    
    let provider = Arc::new(CrossEncoderProvider::new(
        llm_client,
        rerank_config.model.clone()
    ));
    let rerank_handler = Arc::new(RerankRequestHandler::new(provider));
    
    // 4. 创建带重排支持的Searcher
    let searcher = Searcher::builder(qdrant, embedder, bm25)
        .with_rerank(rerank_handler)
        .build();
    
    Ok(searcher)
}

// 使用示例
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let searcher = create_searcher_with_rerank().await?;
    
    // 构建查询选项
    let options = QueryOptions {
        query: "how to handle errors in Rust".to_string(),
        project_id: Some(1),
        config: QueryConfigBuilder::new()
            .build("test")
            .with_enable_reranking(true)
            .with_rerank_max_candidates(30)
            .with_rerank_score_fusion(ScoreFusionStrategy::LinearWeighted { alpha: 0.7 }),
        // ... other fields
    };
    
    // 执行搜索（会自动应用重排）
    let result = searcher.search(&options).await?;
    
    println!("Found {} results", result.total);
    for (i, item) in result.items.iter().take(5).enumerate() {
        println!("{}. {} (score: {:.3})", i + 1, item.name, item.score);
    }
    
    Ok(())
}
```

### 示例6：动态控制重排

```rust
use code_context_engine::orchestrator::query::types::QueryConfigBuilder;

// 根据查询复杂度动态决定是否启用重排
fn build_query_options(query: &str, complexity: QueryComplexity) -> QueryOptions {
    let mut builder = QueryConfigBuilder::new().build("search");
    
    match complexity {
        QueryComplexity::Simple => {
            // 简单查询不启用重排
            builder = builder.with_enable_reranking(false);
        },
        QueryComplexity::Medium => {
            // 中等复杂度查询启用轻量级重排
            builder = builder
                .with_enable_reranking(true)
                .with_rerank_max_candidates(20)
                .with_rerank_score_fusion(ScoreFusionStrategy::LinearWeighted { alpha: 0.6 });
        },
        QueryComplexity::High => {
            // 复杂查询启用完整重排
            builder = builder
                .with_enable_reranking(true)
                .with_rerank_max_candidates(50)
                .with_rerank_return_reasoning(true)
                .with_rerank_score_fusion(ScoreFusionStrategy::LinearWeighted { alpha: 0.8 });
        }
    }
    
    QueryOptions {
        query: query.to_string(),
        project_id: Some(1),
        config: builder,
        // ... other fields
    }
}

enum QueryComplexity {
    Simple,
    Medium,
    High,
}

fn estimate_complexity(query: &str) -> QueryComplexity {
    let word_count = query.split_whitespace().count();
    let has_special_chars = query.chars().any(|c| c == ':' || c == '.' || c == '-');
    
    if word_count <= 2 && !has_special_chars {
        QueryComplexity::Simple
    } else if word_count <= 5 {
        QueryComplexity::Medium
    } else {
        QueryComplexity::High
    }
}
```

## 自定义重排提供商

### 示例7：实现自定义重排提供商

```rust
use async_trait::async_trait;
use code_context_engine::llm::core::error::LlmError;
use code_context_engine::llm::services::rerank::{
    RerankProvider, RerankRequest, RerankResult, RerankedCandidate
};

/// 自定义重排提供商 - 使用规则-based评分
pub struct RuleBasedRerankProvider;

#[async_trait]
impl RerankProvider for RuleBasedRerankProvider {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        let start = std::time::Instant::now();
        
        // 基于规则的评分逻辑
        let mut scored_candidates = Vec::new();
        
        for candidate in &request.candidates {
            let score = self.calculate_relevance_score(&request.query, candidate);
            
            scored_candidates.push(RerankedCandidate {
                id: candidate.id.clone(),
                rerank_score: score,
                initial_score: candidate.initial_score,
                final_score: request.config.score_fusion_strategy.calculate(
                    score,
                    candidate.initial_score,
                    scored_candidates.len()
                ),
                rank_change: 0, // 稍后计算
                reasoning: Some(format!("Rule-based score: {:.3}", score)),
            });
        }
        
        // 按最终得分排序
        scored_candidates.sort_by(|a, b| {
            b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // 计算排名变化
        for (new_rank, candidate) in scored_candidates.iter_mut().enumerate() {
            let initial_rank = request.candidates.iter()
                .position(|c| c.id == candidate.id)
                .unwrap_or(0);
            candidate.rank_change = initial_rank as i32 - new_rank as i32;
        }
        
        let elapsed_ms = start.elapsed().as_millis() as u64;
        
        Ok(RerankResult {
            reranked_candidates: scored_candidates,
            prompt_tokens: 0, // 规则-based不需要token
            total_tokens: 0,
            elapsed_ms,
        })
    }
    
    fn provider_name(&self) -> &str {
        "rule-based"
    }
    
    fn is_available(&self) -> bool {
        true
    }
}

impl RuleBasedRerankProvider {
    fn calculate_relevance_score(&self, query: &str, candidate: &code_context_engine::llm::services::rerank::types::RerankCandidate) -> f32 {
        let mut score = 0.0;
        let query_lower = query.to_lowercase();
        let content_lower = candidate.content.to_lowercase();
        
        // 规则1：查询词匹配
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let matched_words = query_words.iter()
            .filter(|word| content_lower.contains(word))
            .count();
        
        if !query_words.is_empty() {
            score += (matched_words as f32 / query_words.len() as f32) * 0.4;
        }
        
        // 规则2：实体类型匹配
        if let Some(ref entity_type) = candidate.entity_type {
            if query_lower.contains(entity_type) {
                score += 0.2;
            }
        }
        
        // 规则3：文件路径相关性
        if candidate.file_path.to_lowercase().contains(&query_lower) {
            score += 0.2;
        }
        
        // 规则4：代码长度适中（不是太长或太短）
        let line_count = candidate.content.lines().count();
        if line_count >= 5 && line_count <= 50 {
            score += 0.2;
        }
        
        score.min(1.0)
    }
}

// 使用自定义提供商
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(RuleBasedRerankProvider);
    let handler = RerankRequestHandler::new(provider);
    
    // 使用方式与CrossEncoderProvider相同
    let request = RerankRequest {
        query: "error handling".to_string(),
        candidates: vec![/* ... */],
        config: RerankRuntimeConfig::default(),
    };
    
    let result = handler.rerank(&request).await?;
    
    Ok(())
}
```

### 示例8：使用外部重排API（如Cohere）

```rust
use serde::{Deserialize, Serialize};
use reqwest::Client;

/// Cohere重排提供商
pub struct CohereRerankProvider {
    api_key: String,
    model: String,
    client: Client,
}

#[derive(Serialize)]
struct CohereRerankRequest {
    model: String,
    query: String,
    documents: Vec<String>,
    top_n: Option<usize>,
}

#[derive(Deserialize)]
struct CohereRerankResponse {
    results: Vec<CohereRerankResult>,
}

#[derive(Deserialize)]
struct CohereRerankResult {
    index: usize,
    relevance_score: f32,
}

impl CohereRerankProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl RerankProvider for CohereRerankProvider {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        let start = std::time::Instant::now();
        
        // 准备文档列表
        let documents: Vec<String> = request.candidates.iter()
            .map(|c| c.content.clone())
            .collect();
        
        // 构建请求
        let cohere_request = CohereRerankRequest {
            model: self.model.clone(),
            query: request.query.clone(),
            documents,
            top_n: Some(request.candidates.len()),
        };
        
        // 调用Cohere API
        let response = self.client
            .post("https://api.cohere.ai/v1/rerank")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&cohere_request)
            .send()
            .await
            .map_err(|e| LlmError::request(e.to_string()))?;
        
        let cohere_response: CohereRerankResponse = response
            .json()
            .await
            .map_err(|e| LlmError::parse(e.to_string()))?;
        
        // 转换结果
        let mut reranked_candidates = Vec::new();
        for result in cohere_response.results {
            if let Some(candidate) = request.candidates.get(result.index) {
                reranked_candidates.push(RerankedCandidate {
                    id: candidate.id.clone(),
                    rerank_score: result.relevance_score,
                    initial_score: candidate.initial_score,
                    final_score: request.config.score_fusion_strategy.calculate(
                        result.relevance_score,
                        candidate.initial_score,
                        reranked_candidates.len()
                    ),
                    rank_change: 0, // 稍后计算
                    reasoning: None,
                });
            }
        }
        
        // 排序和计算排名变化
        reranked_candidates.sort_by(|a, b| {
            b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        for (new_rank, candidate) in reranked_candidates.iter_mut().enumerate() {
            let initial_rank = request.candidates.iter()
                .position(|c| c.id == candidate.id)
                .unwrap_or(0);
            candidate.rank_change = initial_rank as i32 - new_rank as i32;
        }
        
        let elapsed_ms = start.elapsed().as_millis() as u64;
        
        Ok(RerankResult {
            reranked_candidates,
            prompt_tokens: 0,
            total_tokens: 0,
            elapsed_ms,
        })
    }
    
    fn provider_name(&self) -> &str {
        "cohere"
    }
    
    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}
```

## 性能优化示例

### 示例9：使用缓存优化重排

```rust
use code_context_engine::llm::services::rerank::cache::RerankCache;

pub struct CachedRerankHandler {
    handler: Arc<RerankRequestHandler>,
    cache: Arc<RerankCache>,
}

impl CachedRerankHandler {
    pub fn new(handler: Arc<RerankRequestHandler>, cache_size: u64, ttl_seconds: u64) -> Self {
        Self {
            handler,
            cache: Arc::new(RerankCache::new(cache_size, ttl_seconds)),
        }
    }
    
    pub async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        // 尝试从缓存获取
        if let Some(cached_result) = self.cache.get(request).await {
            tracing::info!("Cache hit for rerank request");
            return Ok((*cached_result).clone());
        }
        
        // 缓存未命中，执行重排
        tracing::info!("Cache miss, executing rerank");
        let result = self.handler.rerank(request).await?;
        
        // 存入缓存
        self.cache.insert(request, result.clone()).await;
        
        Ok(result)
    }
}

// 使用示例
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(CrossEncoderProvider::new(llm_client, "gpt-4o-mini".to_string()));
    let handler = Arc::new(RerankRequestHandler::new(provider));
    let cached_handler = Arc::new(CachedRerankHandler::new(handler, 1000, 3600));
    
    // 第一次调用 - 会执行实际的重排
    let result1 = cached_handler.rerank(&request).await?;
    println!("First call: {}ms", result1.elapsed_ms);
    
    // 第二次调用 - 会从缓存返回
    let result2 = cached_handler.rerank(&request).await?;
    println!("Second call: {}ms (cached)", result2.elapsed_ms);
    
    Ok(())
}
```

### 示例10：批量重排优化

```rust
use futures::future::join_all;

pub struct BatchRerankOptimizer {
    handler: Arc<RerankRequestHandler>,
    batch_size: usize,
}

impl BatchRerankOptimizer {
    pub fn new(handler: Arc<RerankRequestHandler>, batch_size: usize) -> Self {
        Self {
            handler,
            batch_size,
        }
    }
    
    /// 批量重排多个查询的结果
    pub async fn batch_rerank(
        &self,
        requests: Vec<RerankRequest>,
    ) -> Result<Vec<RerankResult>, LlmError> {
        // 将请求分批
        let batches = requests.chunks(self.batch_size);
        
        let mut all_results = Vec::new();
        
        for batch in batches {
            // 并行处理每批
            let futures = batch.iter().map(|req| {
                self.handler.rerank(req)
            }).collect::<Vec<_>>();
            
            let batch_results = join_all(futures).await;
            
            // 收集结果
            for result in batch_results {
                all_results.push(result?);
            }
        }
        
        Ok(all_results)
    }
}

// 使用示例
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(CrossEncoderProvider::new(llm_client, "gpt-4o-mini".to_string()));
    let handler = Arc::new(RerankRequestHandler::new(provider));
    let optimizer = BatchRerankOptimizer::new(handler, 5);
    
    // 准备多个重排请求
    let requests = vec![
        RerankRequest { /* ... */ },
        RerankRequest { /* ... */ },
        // ... more requests
    ];
    
    // 批量重排
    let results = optimizer.batch_rerank(requests).await?;
    
    println!("Processed {} requests", results.len());
    
    Ok(())
}
```

## 故障排除

### 问题1：重排响应解析失败

**症状：**
```
Error: Failed to parse rerank response: expected value at line 1 column 1
```

**解决方案：**

```rust
// 添加更详细的错误处理和日志
fn parse_rerank_response_with_debug(
    &self,
    response: &str,
    request: &RerankRequest,
) -> Result<Vec<RerankedCandidate>, LlmError> {
    tracing::debug!("Raw LLM response: {}", response);
    
    // 尝试提取JSON部分（如果LLM返回了额外文本）
    let json_str = extract_json_from_response(response);
    
    match serde_json::from_str::<Vec<RerankResponseItem>>(&json_str) {
        Ok(parsed) => {
            tracing::debug!("Successfully parsed {} items", parsed.len());
            // ... 正常处理
        },
        Err(e) => {
            tracing::error!(
                "Failed to parse response. Error: {}. Response preview: {}",
                e,
                &response[..response.len().min(200)]
            );
            Err(LlmError::parse(format!("Parse error: {}", e)))
        }
    }
}

fn extract_json_from_response(response: &str) -> String {
    // 尝试找到JSON数组的开始和结束
    if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            return response[start..=end].to_string();
        }
    }
    response.to_string()
}
```

### 问题2：重排超时

**症状：**
```
Error: Request timeout after 5000ms
```

**解决方案：**

```rust
use tokio::time::timeout;

impl RerankRequestHandler {
    pub async fn rerank_with_timeout(
        &self,
        request: &RerankRequest,
    ) -> Result<RerankResult, LlmError> {
        let timeout_duration = std::time::Duration::from_millis(request.config.timeout_ms);
        
        match timeout(timeout_duration, self.provider.rerank(request)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(
                    "Rerank timed out after {}ms",
                    request.config.timeout_ms
                );
                Err(LlmError::timeout(format!(
                    "Rerank timed out after {}ms",
                    request.config.timeout_ms
                )))
            }
        }
    }
}
```

### 问题3：Token超出限制

**症状：**
```
Error: Token limit exceeded: 8500 > 4096
```

**解决方案：**

```rust
impl CrossEncoderProvider {
    fn build_optimized_prompt(&self, request: &RerankRequest) -> String {
        let query = &request.query;
        
        // 估算每个候选的平均token数
        let avg_tokens_per_candidate = 100; // 保守估计
        let max_candidates = (4096 - 500) / avg_tokens_per_candidate; // 预留500 tokens给prompt模板
        
        let candidates_to_include = request.candidates.iter()
            .take(max_candidates)
            .collect::<Vec<_>>();
        
        let mut prompt = format!(
            "Evaluate relevance of code snippets to query. Score 0.0-1.0.\n\n\
             Query: {}\n\nCandidates:\n",
            query
        );
        
        for (i, candidate) in candidates_to_include.iter().enumerate() {
            // 进一步截断内容
            let truncated_content = truncate_content(&candidate.content, 200);
            prompt.push_str(&format!(
                "[{}] {}\n{}\n\n",
                i,
                candidate.file_path,
                truncated_content
            ));
        }
        
        prompt.push_str("Output JSON: [{\"id\":\"...\",\"score\":0.0-1.0}]");
        
        prompt
    }
}
```

### 问题4：重排效果不佳

**症状：**
重排后的结果不如预期，相关性没有明显提升。

**诊断和改进：**

```rust
// 1. 记录重排前后的对比
fn log_rerank_comparison(
    original: &[SearchResult],
    reranked: &[RerankedCandidate],
) {
    tracing::info!("=== Rerank Comparison ===");
    
    for (i, orig) in original.iter().take(5).enumerate() {
        if let Some(rerank) = reranked.iter().find(|r| r.id == orig.id) {
            tracing::info!(
                "Item {}: Original rank={}, New rank={}, Score change: {:.3} -> {:.3}",
                i + 1,
                i + 1,
                i + 1 + rerank.rank_change as usize,
                orig.score,
                rerank.final_score
            );
        }
    }
}

// 2. 调整融合策略参数
fn optimize_fusion_strategy(results: &[SearchResult]) -> ScoreFusionStrategy {
    // 如果原始结果已经很准确，降低重排权重
    let avg_original_score = results.iter()
        .map(|r| r.score)
        .sum::<f32>() / results.len() as f32;
    
    if avg_original_score > 0.8 {
        // 原始结果质量高，降低重排影响
        ScoreFusionStrategy::LinearWeighted { alpha: 0.5 }
    } else {
        // 原始结果质量一般，增加重排影响
        ScoreFusionStrategy::LinearWeighted { alpha: 0.8 }
    }
}

// 3. 改进prompt
fn build_enhanced_prompt(query: &str, candidates: &[RerankCandidate]) -> String {
    format!(
        "You are an expert code search evaluator. Analyze each code snippet's \
         relevance to the query considering:\n\
         1. Semantic similarity\n\
         2. Code completeness\n\
         3. Practical usefulness\n\n\
         Query: {}\n\n\
         Rate each snippet from 0.0 (irrelevant) to 1.0 (highly relevant).\n\n\
         {}",
        query,
        format_candidates_for_evaluation(candidates)
    )
}
```

## 最佳实践总结

1. **选择合适的融合策略**
   - 对于高质量召回结果，使用较低的alpha值（0.5-0.6）
   - 对于噪声较多的结果，使用较高的alpha值（0.7-0.9）

2. **控制重排候选数量**
   - 一般设置20-50个候选
   - 过多会增加成本和延迟
   - 过少可能错过好的结果

3. **合理使用缓存**
   - 对频繁出现的查询启用缓存
   - 设置合理的TTL（通常1-24小时）
   - 监控缓存命中率

4. **渐进式启用**
   - 先在测试环境验证效果
   - 小流量灰度发布
   - 持续监控指标变化

5. **降级策略**
   - 重排失败时回退到原始排序
   - 超时情况下使用部分结果
   - 提供开关快速禁用重排

通过这些示例和最佳实践，你可以有效地在项目中实现和优化重排功能。
