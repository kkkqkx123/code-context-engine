# Qdrant 融合策略支持情况分析

**文档版本**: v1.0  
**创建日期**: 2026-05-18  
**审查状态**: 待审查  
**前置文档**: `sparse_dense_fusion_config_design.md` 第4阶段任务

---

## 1. 概述

本文档基于对 Qdrant 官方文档的调研，详细分析 Qdrant 对各种向量融合策略的支持情况，为 `sparse_dense_fusion_config_design.md` 第4阶段（高级策略实现）提供技术可行性依据。

### 1.1 调研范围

- **RRF（Reciprocal Rank Fusion）**：基于排名的倒数融合
- **WeightedLinear（加权线性组合）**：对多路分数加权求和
- **ScoreBoost（阈值增益）**：基于条件的分数增益
- **应用层后处理**：在客户端实现的自定义融合逻辑

### 1.2 Qdrant 版本要求

- **最低版本**: Qdrant 1.7.0+（支持混合搜索 API）
- **推荐版本**: Qdrant 1.14.0+（支持公式查询和高级评分）
- **开源版 vs 企业版**: 本文档所有功能均在开源版中可用

---

## 2. RRF（Reciprocal Rank Fusion）策略

### 2.1 支持情况

**✅ 完全支持（原生支持）**

Qdrant 原生支持 RRF 融合算法，通过 `FusionQuery` 类型实现，无需额外配置或企业版许可。

### 2.2 实现方式

#### 2.2.1 REST API 调用

```http
POST /collections/{collection_name}/points/query
{
    "prefetch": [
        {
            "query": {
                "indices": [1, 42],
                "values": [0.22, 0.8]
            },
            "using": "sparse",
            "limit": 20
        },
        {
            "query": [0.01, 0.45, 0.67],
            "using": "dense",
            "limit": 20
        }
    ],
    "query": { 
        "fusion": "rrf",
        "k": 60,
        "weights": [0.5, 0.5]
    },
    "limit": 10
}
```

**关键参数说明**：
- `fusion`: 固定值 `"rrf"`，启用 RRF 融合
- `k`: RRF k 值（默认 60），控制排名平滑度
  - 较小的 k 值（如 10-30）：更重视高排名结果
  - 较大的 k 值（如 60-100）：排名分布更均匀
- `weights`: 每路召回的权重数组（可选）
  - 长度必须与 prefetch 数量一致
  - 未指定时默认等权重 `[0.5, 0.5]`
  - 示例：`[0.7, 0.3]` 表示稠密向量占 70% 权重

#### 2.2.2 Rust 客户端实现

当前项目使用 HTTP 客户端直接调用 REST API，但 Qdrant 也提供了 Rust SDK：

```rust
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{Fusion, PrefetchQueryBuilder, Query, QueryPointsBuilder};

let client = Qdrant::from_url("http://localhost:6334").build()?;

client.query(
    QueryPointsBuilder::new("{collection_name}")
        .add_prefetch(PrefetchQueryBuilder::default()
            .query(Query::new_nearest([(1, 0.22), (42, 0.8)].as_slice()))
            .using("sparse")
            .limit(20u64)
        )
        .add_prefetch(PrefetchQueryBuilder::default()
            .query(Query::new_nearest(vec![0.01, 0.45, 0.67]))
            .using("dense")
            .limit(20u64)
        )
        .query(Query::new_fusion(Fusion::Rrf))
        // 注意：Rust SDK 当前不支持设置 k 值和 weights
        // 需通过 HTTP API 或等待 SDK 更新
).await?;
```

**重要限制**：
- Qdrant Rust SDK v1.x 的 `Query::new_fusion(Fusion::Rrf)` 不支持传递 k 值和 weights 参数
- 如需自定义 RRF 参数，必须使用 HTTP REST API

#### 2.2.3 Python 客户端参考

```python
from qdrant_client import QdrantClient, models

client = QdrantClient(url="http://localhost:6333")

client.query_points(
    collection_name="{collection_name}",
    prefetch=[
        models.Prefetch(
            query=models.SparseVector(indices=[1, 42], values=[0.22, 0.8]),
            using="sparse",
            limit=20,
        ),
        models.Prefetch(
            query=[0.01, 0.45, 0.67],
            using="dense",
            limit=20,
        ),
    ],
    query=models.FusionQuery(
        fusion=models.Fusion.RRF,
        k=60,  # Python SDK 支持 k 参数
        weights=[0.7, 0.3]  # Python SDK 支持 weights 参数
    ),
    limit=10
)
```

### 2.3 当前项目实现对比

**现有代码位置**: `src/storage/qdrant/operations/search.rs` 第 117-138 行

```rust
let mut body = serde_json::json!({
    "prefetch": [
        {
            "query": {
                "indices": sparse_vec.indices,
                "values": sparse_vec.values
            },
            "using": "sparse",
            "limit": (query.limit * 2).max(20)
        },
        {
            "query": query.vector,
            "using": "dense",
            "limit": (query.limit * 2).max(20)
        }
    ],
    "query": {
        "fusion": "rrf"  // ❌ 硬编码，无 k 值和 weights
    },
    "limit": query.limit,
    "with_payload": true
});
```

**需要改进的点**：
1. ✅ 已正确使用 `fusion: "rrf"`
2. ❌ 缺少 `k` 参数（应可配置，默认 60）
3. ❌ 缺少 `weights` 参数（应可配置，默认 `[0.5, 0.5]`）
4. ❌ prefetch limit 硬编码为 `limit * 2`，应从配置读取

### 2.4 实施建议

**优先级**: ⭐⭐⭐⭐⭐（最高优先级）

**实施步骤**：
1. 在 `QdrantFusionConfig` 中添加 `rrf_params` 字段
2. 定义 `RrfParams { k: u32, weights: Vec<f32> }` 结构体
3. 修改 `hybrid_search` 方法，从配置读取 k 值和 weights
4. 动态构建 JSON 请求体，包含可选的 k 和 weights 字段

**代码示例**：
```rust
// 在 hybrid_search 方法中
let rrf_config = if let Some(FusionStrategy::Rrf { params }) = &query.fusion_config.strategy {
    let mut rrf_obj = serde_json::json!({ "fusion": "rrf" });
    if let Some(k) = params.k {
        rrf_obj["k"] = serde_json::json!(k);
    }
    if let Some(ref weights) = params.weights {
        rrf_obj["weights"] = serde_json::to_value(weights).unwrap();
    }
    rrf_obj
} else {
    serde_json::json!({ "fusion": "rrf" })
};

body["query"] = rrf_config;
```

---

## 3. WeightedLinear（加权线性组合）策略

### 3.1 支持情况

**⚠️ 部分支持（需应用层实现）**

Qdrant **不直接支持**简单的加权线性组合（如 `final_score = 0.7 * dense_score + 0.3 * sparse_score`）。但可通过以下方式间接实现：

1. **方案 A**: 使用 RRF + weights 参数（推荐）
2. **方案 B**: 使用 Formula Query 自定义评分公式（复杂）
3. **方案 C**: 应用层后处理（灵活但性能较差）

### 3.2 实现方案对比

#### 方案 A：RRF + Weights（推荐）

**原理**：RRF 本质上是一种加权排名融合，通过 `weights` 参数可以控制各路召回的影响力。

**优势**：
- ✅ Qdrant 原生支持，性能最优
- ✅ 无需应用层后处理
- ✅ 支持稀疏+稠密混合场景

**劣势**：
- ⚠️ 不是真正的分数线性组合，而是基于排名的融合
- ⚠️ 无法精确控制最终分数的数值范围

**实现示例**：
```http
POST /collections/{collection_name}/points/query
{
    "prefetch": [
        { "query": {...}, "using": "sparse", "limit": 20 },
        { "query": [...], "using": "dense", "limit": 20 }
    ],
    "query": {
        "fusion": "rrf",
        "weights": [0.3, 0.7]  // 稀疏 30%，稠密 70%
    },
    "limit": 10
}
```

**适用场景**：
- 需要调整稀疏/稠密向量的相对重要性
- 对最终分数的绝对值不敏感
- 追求最佳性能

#### 方案 B：Formula Query（高级用法）

**原理**：使用 Qdrant 1.14.0+ 的公式查询功能，通过数学表达式自定义评分逻辑。

**优势**：
- ✅ 可实现真正的线性组合
- ✅ 支持复杂的条件判断和payload过滤
- ✅ Qdrant 服务端执行，减少网络传输

**劣势**：
- ⚠️ 实现复杂度高，需要理解公式语法
- ⚠️ 需要从 prefetch 中提取原始分数
- ⚠️ 仅适用于单路 prefetch + 公式重排序场景

**实现示例**：
```python
from qdrant_client import models

# 注意：此方案不适用于双路 prefetch 融合
# Formula Query 通常用于单路检索后的重排序
tag_boosted = client.query_points(
    collection_name="{collection_name}",
    prefetch=models.Prefetch(
        query=[0.2, 0.8, ...],  # 单路稠密向量
        limit=50
    ),
    query=models.FormulaQuery(
        formula=models.SumExpression(sum=[
            "$score",  # 基础分数
            models.MultExpression(mult=[
                0.5, 
                models.FieldCondition(
                    key="tag", 
                    match=models.MatchAny(any=["h1", "h2"])
                )
            ]),
        ])
    )
)
```

**局限性**：
- ❌ Formula Query 设计初衷是**单路检索后的分数调整**，而非多路融合
- ❌ 无法直接访问多个 prefetch 的独立分数进行线性组合
- ❌ 对于稀疏+稠密融合场景，此方案不适用

**结论**：**不推荐**用于 WeightedLinear 策略

#### 方案 C：应用层后处理

**原理**：分别执行两路独立查询，在客户端合并结果并计算加权分数。

**优势**：
- ✅ 完全可控，可实现任意融合逻辑
- ✅ 易于调试和测试
- ✅ 不受 Qdrant 版本限制

**劣势**：
- ❌ 性能较差（两次独立查询 + 客户端计算）
- ❌ 增加网络开销
- ❌ 需要手动处理去重和排序

**实现示例**：
```rust
// 伪代码示例
async fn weighted_linear_fusion(
    &self,
    query: SearchQuery,
    dense_weight: f32,
    sparse_weight: f32,
) -> Result<Vec<SearchResult>, QdrantError> {
    // 1. 执行稠密向量查询
    let dense_results = self.dense_search(query.clone()).await?;
    
    // 2. 执行稀疏向量查询
    let sparse_results = self.sparse_search(query.clone()).await?;
    
    // 3. 归一化分数到 [0, 1] 范围
    let normalized_dense = normalize_scores(dense_results);
    let normalized_sparse = normalize_scores(sparse_results);
    
    // 4. 合并结果（按 ID 去重）
    let mut merged = HashMap::new();
    for result in normalized_dense {
        merged.insert(result.id.clone(), (result.score * dense_weight, result.payload));
    }
    for result in normalized_sparse {
        let entry = merged.entry(result.id.clone())
            .or_insert((0.0, result.payload));
        entry.0 += result.score * sparse_weight;
    }
    
    // 5. 排序并返回 Top-K
    let mut final_results: Vec<_> = merged.into_iter().collect();
    final_results.sort_by(|a, b| b.1.0.partial_cmp(&a.1.0).unwrap());
    final_results.truncate(query.limit as usize);
    
    Ok(final_results.into_iter().map(|(id, (score, payload))| SearchResult {
        id, score, payload
    }).collect())
}
```

**适用场景**：
- 需要精确控制分数计算逻辑
- 对性能要求不高
- 实验性场景或 A/B 测试

### 3.3 实施建议

**优先级**: ⭐⭐⭐（中等优先级）

**推荐方案**：**方案 A（RRF + Weights）**

**理由**：
1. Qdrant 原生支持，性能最优
2. 虽然名为 "RRF"，但通过 weights 参数可实现类似加权的效果
3. 符合 Qdrant 的设计理念（基于排名融合而非原始分数）
4. 实施成本低，仅需扩展 RRF 配置

**实施步骤**：
1. 在配置中将 `strategy = "weighted_linear"` 映射到 RRF + weights
2. 将 `dense_weight` 和 `sparse_weight` 转换为 RRF weights 数组
3. 文档中明确说明：WeightedLinear 实际使用 RRF 实现

**配置映射示例**：
```toml
# 用户配置
[storage.qdrant.fusion]
strategy = "weighted_linear"

[storage.qdrant.fusion.weighted_params]
dense_weight = 0.7
sparse_weight = 0.3

# 内部转换为
{
    "fusion": "rrf",
    "weights": [0.3, 0.7]  // 注意顺序：sparse, dense
}
```

**备选方案**：如果未来需要真正的分数线性组合，再实施方案 C（应用层后处理）

---

## 4. ScoreBoost（阈值增益）策略

### 4.1 支持情况

**✅ 完全支持（通过 Formula Query）**

Qdrant 1.14.0+ 支持通过公式查询实现基于条件的分数增益，可用于实现 ScoreBoost 策略。

### 4.2 实现方式

#### 4.2.1 REST API 调用

```http
POST /collections/{collection_name}/points/query
{
    "prefetch": [
        {
            "query": [0.2, 0.8, ...],
            "limit": 50
        }
    ],
    "query": {
        "formula": {
            "sum": [
                "$score",
                {
                    "mult": [
                        1.5,  // boost_factor
                        {
                            "key": "entity_type",
                            "match": { "value": "function" }
                        }
                    ]
                }
            ]
        }
    },
    "limit": 10
}
```

**公式说明**：
- `$score`: 引用 prefetch 返回的原始分数
- `mult`: 乘法表达式
- `FieldCondition`: 匹配 payload 字段
- 当 `entity_type == "function"` 时，分数乘以 1.5

#### 4.2.2 Rust 实现（通过 HTTP API）

```rust
// 构建 ScoreBoost 请求体
let boost_formula = if query.score > threshold {
    serde_json::json!({
        "sum": [
            "$score",
            {
                "mult": [
                    boost_factor - 1.0,  // 额外增益部分
                    condition_object  // 触发条件
                ]
            }
        ]
    })
} else {
    serde_json::json!("$score")  // 无增益
};

let body = serde_json::json!({
    "prefetch": [...],
    "query": { "formula": boost_formula },
    "limit": query.limit
});
```

#### 4.2.3 应用场景

**场景 1：实体类型增益**
```toml
[storage.qdrant.fusion.boost_params]
threshold = 0.8
boost_factor = 1.5
source = "sparse"  # 当稀疏向量分数 > 0.8 时触发

# 对应公式：如果 sparse_score > 0.8，则 final_score *= 1.5
```

**场景 2：目录前缀增益**
```rust
// 优先返回 src/ 目录下的结果
{
    "mult": [
        0.3,
        {
            "key": "file_path",
            "match": { "wildcard": "src/*" }
        }
    ]
}
```

**场景 3：多条件组合增益**
```python
models.SumExpression(sum=[
    "$score",
    # 函数定义 +0.5
    models.MultExpression(mult=[
        0.5,
        models.FieldCondition(key="entity_type", match=models.MatchValue(value="function"))
    ]),
    # 类定义 +0.3
    models.MultExpression(mult=[
        0.3,
        models.FieldCondition(key="entity_type", match=models.MatchValue(value="class"))
    ]),
])
```

### 4.3 局限性

**限制 1：仅适用于单路 prefetch**
- Formula Query 设计用于单路检索后的重排序
- 无法直接访问多个 prefetch 的独立分数

**限制 2：条件必须基于 payload**
- 增益条件只能检查点的 payload 字段
- 无法基于向量相似度或其他运行时指标

**限制 3：复杂度限制**
- 公式嵌套深度有限制
- 过于复杂的公式可能影响查询性能

### 4.4 实施建议

**优先级**: ⭐⭐⭐⭐（较高优先级）

**实施策略**：**应用层实现为主，Formula Query 为辅**

**理由**：
1. ScoreBoost 通常需要跨路分数比较（如 "当稀疏分数 > 稠密分数时增益"）
2. Formula Query 无法直接访问多路 prefetch 的分数
3. 应用层实现更灵活，可支持复杂逻辑

**实施方案**：

**阶段 1：应用层实现（推荐）**
```rust
async fn score_boost_fusion(
    &self,
    query: SearchQuery,
    threshold: f32,
    boost_factor: f32,
    source: BoostSource,  // Sparse | Dense | Both
) -> Result<Vec<SearchResult>, QdrantError> {
    // 1. 执行混合搜索（RRF）
    let results = self.hybrid_search_with_rrf(query.clone()).await?;
    
    // 2. 应用层分数增益
    let boosted_results: Vec<_> = results.into_iter().map(|mut result| {
        let should_boost = match source {
            BoostSource::Sparse => result.sparse_score > threshold,
            BoostSource::Dense => result.dense_score > threshold,
            BoostSource::Both => result.sparse_score > threshold || result.dense_score > threshold,
        };
        
        if should_boost {
            result.score *= boost_factor;
        }
        result
    }).collect();
    
    // 3. 重新排序
    let mut final_results = boosted_results;
    final_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    
    Ok(final_results)
}
```

**阶段 2：简单场景使用 Formula Query（可选优化）**
- 当增益条件仅依赖 payload 时，可使用 Formula Query
- 减少应用层计算开销

**配置示例**：
```toml
[storage.qdrant.fusion]
strategy = "score_boost"

[storage.qdrant.fusion.boost_params]
threshold = 0.8
boost_factor = 1.5
source = "sparse"  # sparse | dense | both
condition_field = "entity_type"  # 可选，用于 Formula Query
condition_value = "function"     # 可选，用于 Formula Query
```

---

## 5. RelativeScoreFusion（相对分数融合）策略

### 5.1 支持情况

**❌ 不支持（需应用层实现）**

Qdrant **不提供**内置的分数归一化和相对融合功能。此策略必须完全在应用层实现。

### 5.2 实现方案

**原理**：将各路分数归一化到统一范围（如 [0, 1]），然后进行融合。

**常用归一化方法**：
1. **Min-Max 归一化**: `normalized = (score - min) / (max - min)`
2. **Z-Score 标准化**: `normalized = (score - mean) / std`
3. **Softmax 归一化**: `normalized = exp(score) / sum(exp(all_scores))`

**实现示例**：
```rust
fn relative_score_fusion(
    dense_results: Vec<SearchResult>,
    sparse_results: Vec<SearchResult>,
    normalization: NormalizationMethod,
    dense_weight: f32,
    sparse_weight: f32,
) -> Vec<SearchResult> {
    // 1. 归一化分数
    let norm_dense = normalize(&dense_results, normalization);
    let norm_sparse = normalize(&sparse_results, normalization);
    
    // 2. 合并结果
    let mut merged = HashMap::new();
    for (result, norm_score) in norm_dense {
        merged.insert(result.id.clone(), norm_score * dense_weight);
    }
    for (result, norm_score) in norm_sparse {
        let entry = merged.entry(result.id.clone()).or_insert(0.0);
        *entry += norm_score * sparse_weight;
    }
    
    // 3. 排序返回
    // ...
}

fn normalize(results: &[SearchResult], method: NormalizationMethod) -> Vec<(SearchResult, f32)> {
    match method {
        NormalizationMethod::MinMax => {
            let min = results.iter().map(|r| r.score).fold(f32::INFINITY, f32::min);
            let max = results.iter().map(|r| r.score).fold(f32::NEG_INFINITY, f32::max);
            let range = max - min;
            
            results.iter().map(|r| {
                let normalized = if range > 0.0 {
                    (r.score - min) / range
                } else {
                    0.5
                };
                (r.clone(), normalized)
            }).collect()
        },
        NormalizationMethod::ZScore => {
            // 实现 Z-Score 标准化
            // ...
        },
        NormalizationMethod::Softmax => {
            // 实现 Softmax 归一化
            // ...
        }
    }
}
```

### 5.3 实施建议

**优先级**: ⭐⭐（低优先级）

**理由**：
1. 实现复杂度高，需要维护归一化逻辑
2. 性能开销大（需遍历所有结果计算统计量）
3. RRF 已能满足大多数场景需求

**适用场景**：
- 需要对不同来源的分数进行精确比较
- 实验性研究或特殊业务需求
- 作为备选方案，在主策略效果不佳时使用

---

## 6. 综合对比与建议

### 6.1 策略支持汇总表

| 策略 | Qdrant 原生支持 | 实现难度 | 性能 | 推荐优先级 | 备注 |
|------|----------------|---------|------|-----------|------|
| **RRF** | ✅ 完全支持 | 低 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 默认策略，强烈推荐 |
| **WeightedLinear** | ⚠️ 通过 RRF+weights | 低 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | 映射到 RRF 实现 |
| **ScoreBoost** | ⚠️ Formula Query（受限） | 中 | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 推荐应用层实现 |
| **RelativeScoreFusion** | ❌ 不支持 | 高 | ⭐⭐⭐ | ⭐⭐ | 仅特殊场景使用 |

### 6.2 实施路线图调整

基于 Qdrant 支持情况，建议调整原设计文档第4阶段的实施计划：

#### 阶段 4.1：RRF 参数化（优先级：高）

**工作内容**：
1. 添加 `RrfParams { k: Option<u32>, weights: Option<Vec<f32>> }`
2. 修改 `hybrid_search` 方法，支持动态 k 值和 weights
3. 编写单元测试验证不同参数组合

**预计工作量**: 0.5 天

#### 阶段 4.2：WeightedLinear 映射实现（优先级：中）

**工作内容**：
1. 在配置解析层将 `weighted_linear` 映射到 RRF + weights
2. 转换 `dense_weight/sparse_weight` 为 RRF weights 数组
3. 文档说明此策略的实际实现方式

**预计工作量**: 0.5 天

#### 阶段 4.3：ScoreBoost 应用层实现（优先级：中高）

**工作内容**：
1. 实现应用层分数增益逻辑
2. 支持基于 sparse/dense 分数的条件判断
3. （可选）简单场景使用 Formula Query 优化

**预计工作量**: 1 天

#### 阶段 4.4：RelativeScoreFusion（优先级：低）

**工作内容**：
1. 实现 Min-Max 归一化
2. 实现分数合并逻辑
3. 性能基准测试

**预计工作量**: 1.5 天

**总工作量调整**：原计划 2 天 → 调整后 3.5 天

### 6.3 关键技术决策

**决策 1：WeightedLinear 策略的实现方式**
- **选择**: 映射到 RRF + weights
- **理由**: 性能最优，实施简单，符合 Qdrant 设计理念
- **风险**: 用户期望真正的线性组合，需在文档中明确说明

**决策 2：ScoreBoost 策略的实现方式**
- **选择**: 应用层实现为主
- **理由**: 灵活性高，支持跨路分数比较
- **风险**: 性能略低于服务端实现，但可接受

**决策 3：是否支持 Formula Query**
- **选择**: 仅在简单场景下使用（如基于 payload 的增益）
- **理由**: 学习曲线陡峭，适用范围有限
- **风险**: 代码复杂度增加，需维护两套逻辑

### 6.4 性能考量

**RRF 性能**：
- Qdrant 服务端执行，零额外开销
- prefetch limit 设置为 `limit * 2` 是合理折衷
- 建议通过配置允许调整倍数（1.5x - 3x）

**应用层融合性能**：
- 额外开销：~5-10ms（取决于结果数量）
- 网络开销：无额外请求（复用 hybrid search 结果）
- 内存开销：临时 HashMap 存储合并结果

**优化建议**：
1. 限制最大 prefetch limit（如不超过 200）
2. 缓存归一化统计量（如使用滑动窗口）
3. 异步执行融合计算（不阻塞主查询流程）

---

## 7. 代码实现指南

### 7.1 类型定义扩展

**位置**: `src/storage/qdrant/types.rs`

```rust
/// 融合策略枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum FusionStrategy {
    /// Reciprocal Rank Fusion
    Rrf {
        #[serde(default)]
        params: Option<RrfParams>,
    },
    /// Weighted Linear Combination (mapped to RRF + weights)
    WeightedLinear {
        params: WeightedParams,
    },
    /// Score Boost based on threshold
    ScoreBoost {
        params: BoostParams,
    },
    /// Relative Score Fusion (application-level)
    RelativeScoreFusion {
        params: RelativeParams,
    },
}

/// RRF 参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RrfParams {
    /// K value for RRF (default: 60)
    #[serde(default = "default_rrf_k")]
    pub k: Option<u32>,
    /// Weights for each prefetch source
    #[serde(default)]
    pub weights: Option<Vec<f32>>,
}

fn default_rrf_k() -> Option<u32> {
    Some(60)
}

/// 加权融合参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedParams {
    /// Weight for dense vector (0.0 - 1.0)
    #[serde(default = "default_dense_weight")]
    pub dense_weight: f32,
    /// Weight for sparse vector (0.0 - 1.0)
    #[serde(default = "default_sparse_weight")]
    pub sparse_weight: f32,
}

fn default_dense_weight() -> f32 {
    0.5
}

fn default_sparse_weight() -> f32 {
    0.5
}

/// 增益策略参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoostParams {
    /// Threshold to trigger boost
    pub threshold: f32,
    /// Boost factor (multiplier)
    #[serde(default = "default_boost_factor")]
    pub boost_factor: f32,
    /// Source to check threshold (sparse | dense | both)
    #[serde(default)]
    pub source: BoostSource,
}

fn default_boost_factor() -> f32 {
    1.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoostSource {
    Sparse,
    Dense,
    Both,
}

/// 相对分数融合参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelativeParams {
    /// Normalization method
    #[serde(default)]
    pub normalization_method: NormalizationMethod,
    /// Weight for dense vector
    #[serde(default = "default_dense_weight")]
    pub dense_weight: f32,
    /// Weight for sparse vector
    #[serde(default = "default_sparse_weight")]
    pub sparse_weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationMethod {
    MinMax,
    ZScore,
    Softmax,
}
```

### 7.2 配置结构体

**位置**: `src/config/modules/storage.rs`

```rust
/// Qdrant 融合配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantFusionConfig {
    /// Fusion strategy
    #[serde(default)]
    pub strategy: FusionStrategy,
    /// Prefetch multiplier (default: 2)
    #[serde(default = "default_prefetch_multiplier")]
    pub prefetch_multiplier: u32,
    /// Minimum prefetch limit (default: 20)
    #[serde(default = "default_min_prefetch_limit")]
    pub min_prefetch_limit: u32,
}

fn default_prefetch_multiplier() -> u32 {
    2
}

fn default_min_prefetch_limit() -> u32 {
    20
}

impl Default for QdrantFusionConfig {
    fn default() -> Self {
        Self {
            strategy: FusionStrategy::Rrf {
                params: Some(RrfParams {
                    k: Some(60),
                    weights: None,
                }),
            },
            prefetch_multiplier: 2,
            min_prefetch_limit: 20,
        }
    }
}
```

### 7.3 SearchOperations 重构

**位置**: `src/storage/qdrant/operations/search.rs`

```rust
async fn hybrid_search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, QdrantError> {
    let url = self.build_query_url();
    let filter = self.build_search_filter(query.directory_prefix.as_deref());
    let sparse_vec = query.sparse_vector.expect("sparse_vector should be present");
    
    // 获取融合配置
    let fusion_config = query.fusion_config.as_ref()
        .unwrap_or(&QdrantFusionConfig::default());
    
    // 计算 prefetch limit
    let prefetch_limit = (query.limit * fusion_config.prefetch_multiplier)
        .max(fusion_config.min_prefetch_limit);
    
    // 根据策略构建请求体
    let body = match &fusion_config.strategy {
        FusionStrategy::Rrf { params } => {
            self.build_rrf_request(&query, &sparse_vec, prefetch_limit, params, filter)?
        },
        FusionStrategy::WeightedLinear { params } => {
            // 映射到 RRF + weights
            let rrf_params = RrfParams {
                k: Some(60),
                weights: Some(vec![params.sparse_weight, params.dense_weight]),
            };
            self.build_rrf_request(&query, &sparse_vec, prefetch_limit, &Some(rrf_params), filter)?
        },
        FusionStrategy::ScoreBoost { params } => {
            // 先执行 RRF，再应用层增益
            let rrf_results = self.execute_rrf_search(&query, &sparse_vec, prefetch_limit, filter).await?;
            return self.apply_score_boost(rrf_results, params);
        },
        FusionStrategy::RelativeScoreFusion { params } => {
            // 应用层相对分数融合
            return self.execute_relative_fusion(&query, &sparse_vec, prefetch_limit, params, filter).await;
        },
    };
    
    // 发送请求并解析结果
    self.send_query_request(&url, body).await
}

fn build_rrf_request(
    &self,
    query: &SearchQuery,
    sparse_vec: &SparseVector,
    prefetch_limit: u32,
    rrf_params: &Option<RrfParams>,
    filter: Option<serde_json::Value>,
) -> Result<serde_json::Value, QdrantError> {
    let mut rrf_query = serde_json::json!({ "fusion": "rrf" });
    
    if let Some(params) = rrf_params {
        if let Some(k) = params.k {
            rrf_query["k"] = serde_json::json!(k);
        }
        if let Some(ref weights) = params.weights {
            rrf_query["weights"] = serde_json::to_value(weights)?;
        }
    }
    
    let mut body = serde_json::json!({
        "prefetch": [
            {
                "query": {
                    "indices": sparse_vec.indices,
                    "values": sparse_vec.values
                },
                "using": "sparse",
                "limit": prefetch_limit
            },
            {
                "query": query.vector,
                "using": "dense",
                "limit": prefetch_limit
            }
        ],
        "query": rrf_query,
        "limit": query.limit,
        "with_payload": true
    });
    
    if let Some(filter) = filter {
        body["filter"] = filter;
    }
    
    Ok(body)
}
```

---

## 8. 测试策略

### 8.1 单元测试

**测试用例清单**：

1. **RRF 参数化测试**
   - 默认 k 值（60）
   - 自定义 k 值（10, 100）
   - 自定义 weights（[0.5, 0.5], [0.7, 0.3]）
   - 空 weights（应使用默认等权重）

2. **WeightedLinear 映射测试**
   - dense_weight=0.7, sparse_weight=0.3 → weights=[0.3, 0.7]
   - 权重和不为 1.0 时的处理
   - 负权重的拒绝

3. **ScoreBoost 应用层测试**
   - 阈值触发增益
   - 阈值未触发
   - 多源触发（Both）

4. **配置合并测试**
   - 全局配置 + 项目配置覆盖
   - 部分字段覆盖
   - 缺失字段使用默认值

### 8.2 集成测试

**测试场景**：

1. **端到端混合搜索**
   - 使用真实 Qdrant 实例
   - 验证不同策略的结果差异
   - 性能基准测试（响应时间、吞吐量）

2. **配置热更新**
   - 运行时修改融合策略
   - 验证新策略立即生效
   - 无服务中断

3. **边界条件**
   - 空结果集
   - 单路无结果
   - 极端分数分布

### 8.3 性能基准测试

**测试指标**：
- P50/P95/P99 延迟
- 吞吐量（queries/sec）
- CPU/内存使用率

**对比基线**：
- 硬编码 RRF（当前实现）
- 可配置 RRF（k=60, weights=[0.5, 0.5]）
- 应用层 ScoreBoost
- 应用层 RelativeScoreFusion

---

## 9. 文档更新建议

### 9.1 用户文档

**新增章节**：融合策略选择指南

**内容要点**：
1. **RRF**: 默认推荐，适合大多数场景
2. **WeightedLinear**: 需要调整稀疏/稠密重要性时使用
3. **ScoreBoost**: 特定实体类型需要优先展示时使用
4. **RelativeScoreFusion**: 实验性场景，谨慎使用

**配置示例**：每个策略提供完整的 TOML 配置示例

### 9.2 API 文档

**新增接口**：
- `POST /api/search` 支持 `fusion_strategy` 参数
- 参数验证规则
- 错误码说明

### 9.3 开发者文档

**新增内容**：
- 融合策略架构设计
- 如何扩展新策略
- 性能调优建议

---

## 10. 总结

### 10.1 核心发现

1. **RRF**: Qdrant 完全支持，可立即实施
2. **WeightedLinear**: 通过 RRF + weights 间接实现，性能最优
3. **ScoreBoost**: 推荐应用层实现，灵活性高
4. **RelativeScoreFusion**: 需完全应用层实现，优先级低

### 10.2 实施建议

**立即执行**（阶段 4.1-4.2）：
- RRF 参数化（0.5 天）
- WeightedLinear 映射（0.5 天）

**短期规划**（阶段 4.3）：
- ScoreBoost 应用层实现（1 天）

**长期规划**（阶段 4.4）：
- RelativeScoreFusion（1.5 天，视需求而定）

### 10.3 风险提示

1. **Qdrant 版本兼容性**：确保生产环境 Qdrant >= 1.7.0
2. **Rust SDK 限制**：如需自定义 RRF 参数，必须使用 HTTP API
3. **性能回退**：应用层融合会增加 5-10ms 延迟
4. **配置复杂度**：提供合理的默认值，避免用户困惑

### 10.4 下一步行动

1. ✅ 完成本文档审查
2. 📋 更新 `sparse_dense_fusion_config_design.md` 实施路线图
3. 🔨 开始阶段 4.1 实施（RRF 参数化）
4. 🧪 编写单元测试框架
5. 📊 建立性能基准测试套件

---

**附录 A：Qdrant 官方文档引用**

1. Hybrid Search with RRF: https://qdrant.tech/documentation/concepts/hybrid-search/
2. Query API Reference: https://api.qdrant.tech/api-reference/search/query-points
3. Formula Query: https://qdrant.tech/documentation/concepts/formula-scoring/
4. Rust Client: https://github.com/qdrant/rust-client

**附录 B：相关代码文件清单**

- `src/storage/qdrant/types.rs` - 类型定义
- `src/storage/qdrant/operations/search.rs` - 搜索操作
- `src/config/modules/storage.rs` - 配置模块
- `src/orchestrator/query/types.rs` - 编排器配置
- `tests/integration_qdrant.rs` - 集成测试

**附录 C：术语表**

- **RRF**: Reciprocal Rank Fusion，倒数排名融合
- **Prefetch**: 预取，指在融合前先执行多路独立检索
- **Formula Query**: 公式查询，Qdrant 的自定义评分功能
- **Payload**: Qdrant 中点的元数据字段
