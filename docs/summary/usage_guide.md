# Summary Boost 功能使用指南

## 概述

Summary Boost 功能通过文件级摘要的向量相似度检索，为匹配文件内的代码块提供排名提升（boost），从而提高大范围检索的精度。

## 架构设计

Summary Boost 采用增强器模式，在检索后对结果进行分数增益：

```
QueryOptions (sources.summary = true)
    │
    ▼
SearchPipeline
    ├── Retrieval → 获取代码块候选集
    ├── BM25 Enhancement (可选)
    ├── Summary Boost Enhancement ⭐
    │   ├── 生成查询向量
    │   ├── 检索匹配的摘要
    │   └── 对匹配文件的代码块应用增益
    ├── Relation Boost (可选)
    └── Post-Processing → 排序/过滤
```

## 配置说明

### SearchConfig 字段

```rust
pub struct SearchConfig {
    /// 是否启用摘要增益（默认 false）
    pub enable_summary_boost: bool,
    
    /// 增益系数（默认 1.2，即 20% 增益）
    pub summary_boost_factor: f32,
    
    /// 最小摘要相似度阈值（默认 0.4）
    pub summary_min_score: f32,
    
    /// 检索的摘要数量上限（默认 20）
    pub summary_top_k: usize,
}
```

### 推荐配置

```rust
// 保守配置（适合生产环境）
let config = SearchConfig {
    enable_summary_boost: true,
    summary_boost_factor: 1.2,  // 20% 增益
    summary_min_score: 0.5,     // 较高的质量阈值
    summary_top_k: 15,          // 适中的检索数量
    ..Default::default()
};

// 激进配置（适合实验性场景）
let config = SearchConfig {
    enable_summary_boost: true,
    summary_boost_factor: 1.3,  // 30% 增益
    summary_min_score: 0.4,     // 较低的质量阈值
    summary_top_k: 20,          // 更多的检索数量
    ..Default::default()
};
```

## 使用示例

### 1. 基本用法

```rust
use code_context_engine::orchestrator::query::{
    QueryCoordinator, QueryOptions, SearchSources, SearchConfig
};

// 创建查询选项
let options = QueryOptions {
    query: "user authentication logic".to_string(),
    sources: SearchSources {
        vector: true,
        bm25: true,
        summary: true,  // 启用摘要增益
        relation: false,
    },
    config: SearchConfig {
        enable_summary_boost: true,
        summary_boost_factor: 1.3,
        summary_min_score: 0.5,
        ..Default::default()
    },
    ..Default::default()
};

// 执行查询
let result = coordinator.search(&options).await?;
```

### 2. 通过 Searcher Builder 启用

```rust
use code_context_engine::orchestrator::query::Searcher;

let searcher = Searcher::builder(qdrant, embedder, bm25)
    .with_sqlite(sqlite)
    .with_assembler(assembler)
    .with_rerank(rerank_handler)
    .with_relation_boost(relation_searcher)
    .with_summary_boost()  // 启用摘要增益支持
    .build();
```

### 3. 检查结果中的增益信息

```rust
for item in result.items {
    if item.is_boosted {
        println!("Chunk {} was boosted", item.id);
        println!("Boost reason: {:?}", item.boost_reason);
        println!("Summary score: {:?}", item.metadata.get("summary_score"));
        println!("Original score: {:?}", item.metadata.get("original_score"));
        println!("Final score: {}", item.score);
    }
}
```

## 工作原理

### 1. 提取候选文件路径

从检索结果中提取所有唯一的文件路径。

### 2. 生成查询向量

使用嵌入模型将查询文本转换为向量。

### 3. 检索匹配的摘要

在摘要集合中搜索与查询向量相似的文件摘要，限制在候选文件范围内以提高效率。

### 4. 应用增益

对于摘要相似度超过阈值的文件，对其中的所有代码块应用乘法增益：

```rust
final_score = original_score × summary_boost_factor
```

### 5. 记录元数据

在 SearchResult 中记录增益相关信息：
- `is_boosted`: 标记是否被增益
- `boost_reason`: 增益原因描述
- `metadata.summary_score`: 摘要相似度分数
- `metadata.original_score`: 原始分数

## 性能考虑

### 延迟影响

- **基础延迟**: 约 30-80ms（取决于 `summary_top_k` 和嵌入模型速度）
- **优化建议**:
  - 降低 `summary_top_k`（如从 20 降至 10）
  - 实现摘要向量缓存（未来优化）
  - 并行执行摘要检索与代码块检索（未来优化）

### 内存开销

- 摘要向量存储在独立的 Qdrant collection 中
- 每个文件一个摘要向量（维度与代码块向量相同）
- 额外内存开销约为文件数量 × 向量维度 × 4 字节

## 调优建议

### 增益系数选择

| 场景 | summary_boost_factor | 说明 |
|------|---------------------|------|
| 保守 | 1.1 - 1.2 | 轻微提升，避免排名剧烈变化 |
| 平衡 | 1.2 - 1.3 | 适度提升，推荐起始值 |
| 激进 | 1.3 - 1.5 | 显著提升，需仔细评估效果 |

### 阈值选择

| 场景 | summary_min_score | 说明 |
|------|------------------|------|
| 严格 | 0.6 - 0.7 | 仅高质量匹配 |
| 平衡 | 0.4 - 0.5 | 推荐起始值 |
| 宽松 | 0.3 - 0.4 | 更多匹配，可能包含噪声 |

### 检索数量选择

| 场景 | summary_top_k | 说明 |
|------|--------------|------|
| 快速 | 5 - 10 | 低延迟，适合实时查询 |
| 平衡 | 15 - 20 | 推荐起始值 |
| 全面 | 20 - 30 | 更高召回率，延迟增加 |

## 调试技巧

### 1. 启用详细日志

```rust
// 在环境变量中设置
RUST_LOG=code_context_engine::orchestrator::query::enhancement::summary_boost_enhancer=debug
```

### 2. 查看增益决策日志

```
DEBUG Applied summary boost chunk_id=... file_path=... original_score=0.65 boosted_score=0.78 summary_score=0.82
```

### 3. 检查元数据

```rust
// 检查哪些结果被增益
let boosted_count = result.items.iter().filter(|r| r.is_boosted).count();
println!("Boosted results: {} / {}", boosted_count, result.items.len());

// 分析增益分布
for item in &result.items {
    if let Some(summary_score) = item.metadata.get("summary_score") {
        println!("Chunk {}: summary_score={}", item.id, summary_score);
    }
}
```

## 常见问题

### Q1: 为什么我的结果没有被增益？

**可能原因**:
1. `enable_summary_boost` 设置为 `false`
2. `sources.summary` 未设置为 `true`
3. 摘要相似度低于 `summary_min_score` 阈值
4. 文件中没有生成摘要或摘要未索引

**解决方法**:
- 检查配置项是否正确设置
- 降低 `summary_min_score` 阈值
- 确认摘要已正确生成并存储到 Qdrant

### Q2: 增益效果不明显怎么办？

**建议**:
1. 提高 `summary_boost_factor`（如从 1.2 提高到 1.3）
2. 降低 `summary_min_score` 以匹配更多文件
3. 增加 `summary_top_k` 以检索更多摘要
4. 检查摘要质量，可能需要改进摘要生成算法

### Q3: 与 BM25 增益冲突怎么办？

当前设计使用乘法叠加：
```rust
final_score = base_score × consensus_boost × summary_boost_factor
```

如果发现排名失真，可以考虑：
1. 降低其中一个增益系数
2. 改为取最大值策略（需要修改代码）
3. 禁用其中一个增益功能

### Q4: 性能开销太大怎么办？

**优化方案**:
1. 降低 `summary_top_k`（最有效的优化）
2. 提高 `summary_min_score` 以减少后续处理
3. 仅在必要时启用（通过 `sources.summary` 控制）
4. 等待未来的缓存优化

## 最佳实践

1. **渐进式启用**: 先在测试环境中启用，观察效果后再推广到生产环境
2. **A/B 测试**: 对比启用/禁用时的检索质量和用户满意度
3. **监控指标**: 跟踪增益比例、平均延迟、用户点击率等指标
4. **定期调优**: 根据实际使用情况调整配置参数
5. **质量控制**: 定期检查增益结果的合理性，避免过度增益

## 故障排除

### 问题：编译错误 "cannot find function `search_summaries_with_paths`"

**原因**: QdrantClient 扩展方法未正确添加

**解决**: 确保 `src/storage/qdrant/client.rs` 中包含 `search_summaries_with_paths` 方法

### 问题：运行时错误 "summary collection not found"

**原因**: 摘要集合未在 Qdrant 中创建

**解决**: 
1. 确认索引阶段已正确执行
2. 检查 Qdrant 中是否存在摘要集合
3. 重新运行索引流程

### 问题：增益效果不符合预期

**排查步骤**:
1. 检查日志确认增益逻辑是否被执行
2. 验证摘要相似度分数是否合理
3. 调整配置参数并重新测试
4. 检查摘要生成质量

## 后续优化方向

1. **缓存机制**: 缓存高频查询的摘要检索结果
2. **并行执行**: 摘要检索与代码块检索并行执行
3. **动态增益**: 基于查询类型自动调整增益系数
4. **多粒度摘要**: 支持文件级、模块级、类级摘要
5. **LLM 评估**: 使用 LLM 评估摘要相关性替代纯向量相似度

## 相关文档

- [query_integration_plan.md](./query_integration_plan.md) - 集成方案设计
- [implementation_guide.md](./implementation_guide.md) - 实施指南
- [文件摘要索引与查询集成设计.md](./文件摘要索引与查询集成设计.md) - 完整设计文档
