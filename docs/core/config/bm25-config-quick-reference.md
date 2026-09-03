# BM25 配置快速参考

## 配置文件路径

```toml
# config.toml 或 config.example.toml
```

## 配置结构

```toml
[database.bm25]
enabled = true
index_name = "code_index"

[database.bm25.algorithm]
k1 = 1.8   # 词频饱和度（代码搜索优化）
b = 0.4    # 长度归一化（代码搜索优化）

[database.bm25.field_weights]
title = 3.0       # 实体名称（最高优先级）
content = 1.0     # 描述文本（基础权重）
keywords = 2.0    # 提取的关键词（中等优先级）

[database.bm25.search]
default_limit = 10
max_limit = 100
enable_highlight = true

[database.bm25.index_manager]
writer_memory_budget = 50000000
reload_policy = "on_commit_with_delay"
```

## 参数调优速查表

### 场景推荐配置

| 场景 | k1 | b | title_weight | keywords_weight | 说明 |
|------|-----|-----|--------------|-----------------|------|
| **代码搜索（默认）** | 1.8 | 0.4 | 3.0 | 2.0 | 精确匹配优先，适合标识符搜索 |
| **通用搜索** | 1.2 | 0.75 | 3.0 | 1.5 | 平衡精确与召回 |
| **API 精确查找** | 2.0 | 0.3 | 5.0 | 1.5 | 强调名称匹配 |
| **功能语义搜索** | 1.2 | 0.7 | 3.0 | 2.5 | 重视描述和关键词 |
| **短文本** | 1.5-2.0 | 0.3-0.5 | 4.0 | 2.0 | 标题、名称等短字段 |
| **长文档** | 1.0-1.2 | 0.7-0.9 | 2.0 | 1.5 | 完整文章、文档 |

### 参数影响

#### k1（词频饱和度）
- **增大 k1**（1.5-2.0）：重复出现的词获得更多权重
  - 适合：专业术语多的领域（代码、法律、医学）
  - 效果：提高精确度，降低召回率
- **减小 k1**（0.8-1.2）：词频收益递减更快
  - 适合：通用文本、新闻
  - 效果：提高召回率，降低精确度

#### b（长度归一化）
- **增大 b**（0.7-1.0）：更强的长度惩罚
  - 适合：文档长度差异大的场景
  - 效果：短文档更容易排名靠前
- **减小 b**（0.0-0.5）：更弱的长度惩罚
  - 适合：文档长度相近的场景
  - 效果：长文档不会因长度被过度惩罚

## 架构说明

本项目使用 **本地 tantivy fork**（`crates/tantivy`，基于 v0.26.1）实现 BM25 全文搜索：

- **BM25 参数可配置**：通过 `Bm25Params` 结构体（定义在 `crates/tantivy/src/index/index_meta.rs`）支持自定义 k1 和 b 参数
- **字段权重**：通过 `BoostQuery`（定义在 `crates/cce_orchestrator/src/query/retrieval/bm25.rs`）实现字段级加权
- **配置定义**：`crates/cce_core/src/config/modules/storage.rs` 中的 `Bm25AlgorithmConfig` 和 `FieldWeights`
- **索引管理**：`crates/cce_infrastructure/src/storage/bm25/` 下的 `IndexManager` 负责索引生命周期

### 评分机制

```
最终评分 = Σ[BM25(term, field) × boost_factor]

其中 BoostFactor:
- title: 3.0（实体名称最重要）
- keywords: 2.0（提取的关键词次之）
- content: 1.0（描述和注释作为补充）
```

## 测试方法

### 1. 修改配置
```toml
[database.bm25.algorithm]
k1 = 1.5
b = 0.6
```

### 2. 重建索引
```bash
# 删除旧索引
rm -rf .cce/bm25_index

# 重新索引
cargo run -- index /path/to/codebase
```

### 3. 测试搜索
```bash
# 使用 CLI 测试
cargo run -- search "your query"
```

### 4. 评估结果
- 检查相关文档是否排名靠前
- 观察评分分布
- 对比不同参数的效果

## 监控指标

建议跟踪以下指标来评估参数效果：

1. **Precision@K**：前 K 个结果的相关性
2. **Mean Reciprocal Rank (MRR)**：第一个相关结果的排名
3. **Normalized Discounted Cumulative Gain (NDCG)**：整体排序质量
4. **用户反馈**：实际使用中的满意度

## 进阶技巧

### 渐进式调优
1. 先固定 `b=0.75`，调整 `k1`（1.0 → 1.2 → 1.5 → 1.8）
2. 选择最佳 k1 后，固定它，调整 `b`（0.5 → 0.6 → 0.75 → 0.9）
3. 微调最佳组合附近的值

### 文档分析
在调整参数前，先分析你的数据：
```rust
// 统计平均文档长度
// 统计词频分布
// 分析字段长度差异
```

## 相关源码位置

| 组件 | 路径 |
|------|------|
| BM25 配置定义 | `crates/cce_core/src/config/modules/storage.rs` |
| BM25 检索实现 | `crates/cce_orchestrator/src/query/retrieval/bm25.rs` |
| Tantivy BM25 参数 | `crates/tantivy/src/index/index_meta.rs` |
| Tantivy BM25 评分 | `crates/tantivy/src/query/bm25.rs` |
| 索引管理器 | `crates/cce_infrastructure/src/storage/bm25/` |
| 配置示例 | `config.example.toml` |
