# 重排模块 (Re-ranking Module)

## 概述

重排模块为 code-context-engine 提供了在召回后、最终排序前的二次排序能力。通过使用LLM或专用重排模型，对初步召回的候选结果进行更精确的相关性评分，从而提升搜索结果的质量。

## 为什么需要重排？

### 当前检索流程的局限性

现有的检索流程包括：
- **向量检索**：基于语义相似度，但可能忽略关键词匹配
- **BM25检索**：基于关键词统计，但缺乏深层语义理解
- **混合检索**：结合两者，但仍属于浅层相关性判断

这些方法在召回阶段表现良好，但在精排阶段存在不足：
1. 无法理解复杂的查询意图
2. 难以判断代码片段的实际可用性
3. 缺乏对上下文关系的深度分析

### 重排的优势

引入重排后可以：
- ✅ 提高Top-K结果的相关性质量
- ✅ 更好地处理复杂和多意图查询
- ✅ 减少噪声结果的干扰
- ✅ 提供可解释的排序理由（可选）

## 架构设计

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌─────────────┐
│   Query     │ -> │  Retrieval   │ -> │  Re-ranking  │ -> │ Final Sort  │
│             │    │ (Recall)     │    │ (Precision)  │    │ & Filtering │
└─────────────┘    └──────────────┘    └─────────────┘    └─────────────┘
                         │                    │
                   Vector/BM25          LLM-based scoring
                   Hybrid search        Cross-encoder model
                   
召回阶段                重排阶段              输出阶段
(快速、广覆盖)         (精准、高质量)        (最终排序)
```

## 核心组件

### 1. RerankProvider Trait

定义重排提供商的统一接口：

```rust
#[async_trait]
pub trait RerankProvider: Send + Sync {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError>;
    fn provider_name(&self) -> &str;
    fn is_available(&self) -> bool;
}
```

### 2. 内置提供商

#### CrossEncoderProvider
基于LLM的交叉编码器实现，使用prompt工程让LLM评估相关性。

**特点：**
- 高精度
- 灵活可定制
- 支持多种LLM后端

**适用场景：**
- 对精度要求高
- 候选数量较少（<100）
- 预算充足

#### RuleBasedRerankProvider（示例）
基于规则的轻量级重排，无需调用外部API。

**特点：**
- 零成本
- 超低延迟
- 效果有限

**适用场景：**
- 快速原型验证
- 离线环境
- 成本敏感场景

### 3. RerankRequestHandler

重排请求处理器，负责：
- 请求验证
- 候选数量限制
- 调用提供商执行重排
- 结果后处理

### 4. 得分融合策略

支持多种将重排得分与原始得分结合的策略：

| 策略 | 公式 | 适用场景 |
|------|------|----------|
| RerankOnly | final = rerank | 完全信任重排结果 |
| LinearWeighted | final = α·rerank + (1-α)·initial | 平衡重排和原始得分 |
| Multiplicative | final = rerank × initial | 双重确认机制 |
| ReciprocalRankFusion | final = 1/(k+rank) | 排名融合 |

## 快速开始

### 1. 安装依赖

在 `Cargo.toml` 中添加：

```toml
[dependencies]
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
tracing = "0.1"
```

### 2. 基础用法

```rust
use std::sync::Arc;
use code_context_engine::llm::{LlmClient, LlmConfig};
use code_context_engine::llm::services::rerank::{
    RerankRequest, RerankCandidate, RerankRuntimeConfig,
    RerankRequestHandler, CrossEncoderProvider,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建LLM客户端
    let config = LlmConfig::openai("sk-your-api-key".to_string());
    let llm_client = Arc::new(LlmClient::new(config)?);
    
    // 创建重排处理器
    let provider = Arc::new(CrossEncoderProvider::new(
        llm_client,
        "gpt-4o-mini".to_string()
    ));
    let handler = RerankRequestHandler::new(provider);
    
    // 准备候选
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
    
    // 执行重排
    let request = RerankRequest {
        query: "how to start the app".to_string(),
        candidates,
        config: RerankRuntimeConfig::default(),
    };
    
    let result = handler.rerank(&request).await?;
    
    // 处理结果
    for candidate in result.reranked_candidates {
        println!("{}: {:.3}", candidate.id, candidate.final_score);
    }
    
    Ok(())
}
```

### 3. 与Searcher集成

```rust
// 创建带重排支持的Searcher
let rerank_handler = Arc::new(RerankRequestHandler::new(provider));
let searcher = Searcher::builder(qdrant, embedder, bm25)
    .with_rerank(rerank_handler)
    .build();

// 配置查询选项启用重排
let options = QueryOptions {
    query: "error handling in Rust".to_string(),
    config: QueryConfigBuilder::new()
        .build("search")
        .with_enable_reranking(true)
        .with_rerank_max_candidates(30),
    // ...
};

// 执行搜索（自动应用重排）
let result = searcher.search(&options).await?;
```

## 配置

### TOML配置

```toml
# config.toml

[rerank]
enabled = true
provider = "cross-encoder"
model = "gpt-4o-mini"
max_candidates = 50
temperature = 0.0
return_reasoning = false
score_fusion_strategy = "linear_weighted"
timeout_ms = 5000

[query]
enable_reranking = true
rerank_model = "gpt-4o-mini"
rerank_max_candidates = 50
rerank_temperature = 0.0
rerank_return_reasoning = false
rerank_score_fusion = "linear_weighted"
rerank_alpha = 0.7
rerank_timeout_ms = 5000
```

### 环境变量

```bash
# .env
RERANK_ENABLED=true
RERANK_MODEL=gpt-4o-mini
RERANK_MAX_CANDIDATES=50
RERANK_TIMEOUT_MS=5000
```

## 性能优化

### 1. 缓存

```rust
use code_context_engine::llm::services::rerank::cache::RerankCache;

let cache = Arc::new(RerankCache::new(1000, 3600)); // 1000条，TTL 1小时
let cached_handler = CachedRerankHandler::new(handler, cache);
```

**效果：**
- 缓存命中率：30-60%（取决于查询重复度）
- 延迟降低：90%+（缓存命中时）
- Token节省：显著

### 2. 批处理

```rust
let optimizer = BatchRerankOptimizer::new(handler, 5);
let results = optimizer.batch_rerank(requests).await?;
```

**效果：**
- 吞吐量提升：2-5倍
- 平均延迟降低：20-40%

### 3. 候选筛选

```rust
// 只对Top-N候选进行重排
config.max_candidates = 30; // 而不是全部100+候选
```

**效果：**
- 成本降低：60-70%
- 延迟降低：50-60%
- 质量损失：<5%

## 监控指标

建议跟踪以下指标：

### 性能指标

```rust
// 重排耗时
metrics::histogram!("rerank_duration_ms").record(elapsed_ms as f64);

// Token使用量
metrics::counter!("rerank_tokens_total").increment(result.total_tokens);

// 缓存命中率
metrics::gauge!("rerank_cache_hit_rate").set(hit_rate);
```

### 质量指标

```rust
// 重排前后对比
let avg_rank_change = reranked.iter()
    .map(|r| r.rank_change.abs())
    .sum::<i32>() as f64 / reranked.len() as f64;

metrics::gauge!("rerank_avg_rank_change").set(avg_rank_change);

// 得分分布
for candidate in &reranked {
    metrics::histogram!("rerank_score_distribution")
        .record(candidate.final_score as f64);
}
```

### 业务指标

- 用户点击率（CTR）变化
- 平均点击位置
- 查询满意度评分
- A/B测试结果

## 故障排除

### 常见问题

#### 1. 重排超时

**症状：** `Error: Request timeout after 5000ms`

**解决：**
- 增加超时时间：`config.timeout_ms = 10000`
- 减少候选数量：`config.max_candidates = 20`
- 使用更快的模型：切换到 `gpt-3.5-turbo`

#### 2. 解析失败

**症状：** `Failed to parse rerank response`

**解决：**
- 启用调试日志查看原始响应
- 改进prompt确保JSON格式
- 添加响应清洗逻辑

#### 3. 成本过高

**症状：** Token消耗超出预期

**解决：**
- 启用缓存
- 减少重排频率（只对重要查询）
- 使用更便宜的模型
- 缩短候选内容

#### 4. 效果不佳

**症状：** 重排后结果不如预期

**解决：**
- 调整融合策略参数（alpha值）
- 改进prompt模板
- 尝试不同的得分融合策略
- 检查原始召回质量

## 最佳实践

### 1. 渐进式启用

```
阶段1: 测试环境验证（1周）
  ↓
阶段2: 小流量灰度（5%流量，1周）
  ↓
阶段3: 扩大流量（20% → 50% → 100%，每阶段1周）
  ↓
阶段4: 全量上线，持续监控
```

### 2. 选择合适的场景

**适合重排的场景：**
- ✅ 复杂的多意图查询
- ✅ 对精度要求高的搜索
- ✅ 候选数量适中（20-100）
- ✅ 预算充足

**不适合重排的场景：**
- ❌ 简单关键词查询
- ❌ 实时性要求极高
- ❌ 成本极度敏感
- ❌ 候选数量过大（>200）

### 3. 降级策略

```rust
match handler.rerank(&request).await {
    Ok(result) => {
        // 使用重排结果
        apply_rerank_results(result)
    },
    Err(e) => {
        // 降级：使用原始排序
        tracing::warn!("Rerank failed, using original ranking: {}", e);
        use_original_ranking()
    }
}
```

### 4. 持续优化

- 定期分析重排效果
- A/B测试不同配置
- 收集用户反馈
- 根据数据调整策略

## 扩展开发

### 添加新的重排提供商

```rust
pub struct MyCustomProvider {
    // 你的实现
}

#[async_trait]
impl RerankProvider for MyCustomProvider {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult, LlmError> {
        // 实现重排逻辑
        todo!()
    }
    
    fn provider_name(&self) -> &str {
        "my-custom"
    }
    
    fn is_available(&self) -> bool {
        true
    }
}
```

### 自定义得分融合策略

```rust
impl ScoreFusionStrategy {
    pub fn custom_fusion(rerank_score: f32, initial_score: f32) -> f32 {
        // 你的融合逻辑
        rerank_score.powf(0.8) * initial_score.powf(0.2)
    }
}
```

## 文档导航

- [设计文档](design.md) - 详细的架构设计和方案对比
- [实现指南](implementation.md) - 逐步实现教程
- [使用示例](examples.md) - 丰富的代码示例

## 参考资料

- [Cross-encoder vs Bi-encoder](https://www.sbert.net/examples/applications/cross-encoder/README.html)
- [Reciprocal Rank Fusion](https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf)
- [LLM for Information Retrieval](https://arxiv.org/abs/2305.06983)

## 贡献

欢迎提交Issue和Pull Request！

## 许可证

与主项目保持一致。
