# 混合检索归一化设计分析

## 问题定义

混合检索需要融合来自不同算法的分数：
- **向量路径**：余弦相似度，范围通常 [0, 1]
- **BM25 路径**：TF-IDF 变体，无界正值

**核心问题**：两种分数量纲不同，不能直接加权求和。归一化的目标是将它们映射到可比较的公共空间。

---

## 现有设计分析

### 当前方案：路径内 Min-Max 归一化

```rust
// 每条路径独立归一化到 [0, 1]
let normalized_vector = minmax_normalize(&vector_scores);  // [0.0, 1.0]
let normalized_bm25 = minmax_normalize(&bm25_scores);      // [0.0, 1.0]

// 加权融合
fused_score = alpha * norm_vector + beta * norm_bm25;
```

### 优点

1. **简单高效**：O(n) 复杂度，无需额外计算
2. **保留序关系**：路径内的相对排序不变
3. **路径隔离**：一条路径的分数分布不影响另一条

### 缺陷

| 问题 | 描述 | 影响 |
|------|------|------|
| **单结果失真** | 单结果路径归一化为 1.0 | 无法区分"唯一结果"和"多个结果中的最优" |
| **分布信息丢失** | 只保留相对顺序，丢失绝对置信度 | 向量 0.92 vs BM25 15.0 的置信度差异被抹平 |
| **离群值敏感** | 一个异常高分会压缩其他分数的归一化范围 | 归一化后的分数分布集中在低端 |
| **结果数依赖** | 同样 0.8 的原始分，在 10 结果路径和 100 结果路径中归一化值不同 | 跨查询不可比 |

---

## 替代方案对比

### 方案 1：Z-Score 标准化

```rust
// 使用历史分数的均值和标准差
z_score = (raw_score - mean) / std_dev;
normalized = sigmoid(z_score);  // 映射到 [0, 1]
```

**优点**：
- 考虑分数分布，跨查询可比
- 单结果不会强制为 1.0

**缺点**：
- 需要维护历史分数统计（均值、标准差）
- 冷启动问题：新索引无历史数据
- 计算开销增加

**适用场景**：有稳定查询负载的生产环境

### 方案 2：Rank-Based 融合 (RRF)

```rust
// 完全避免分数比较，只用排名
reciprocal_rank = 1.0 / (rank + k);
fused_score = alpha * rr_vector + beta * rr_bm25;
```

**优点**：
- 完全规避分数分布问题
- 实现简单，无需统计信息
- 对离群值不敏感

**缺点**：
- 丢失分数幅度信息（top1=0.99 和 top1=0.51 的置信度差异被忽略）
- 排名相近的结果区分度降低

**适用场景**：分数量纲差异极大且无法校准的场景

### 方案 3：Learned Calibration（学习式校准）

```rust
// 使用标注数据学习每个路径的校准函数
calibrated_vector = sigmoid(a_v * raw_vector + b_v);
calibrated_bm25 = sigmoid(a_b * raw_bm25 + b_b);
```

**优点**：
- 理论上最优：学习真实的相关性概率
- 跨路径分数可比（都映射到 P(relevant)）

**缺点**：
- 需要标注数据集
- 需要定期重新训练
- 工程复杂度高

**适用场景**：有充足标注数据的大规模生产系统

### 方案 4：改进的 Min-Max（当前方案的折中优化）

```rust
// 单结果路径使用固定中性值，而非 1.0
normalized = if results.len() > 1 {
    minmax_normalize(scores)
} else {
    vec![0.5]  // 或基于原始分数的某种映射
};
```

**优点**：
- 实现简单
- 缓解单结果失真问题

**缺点**：
- 0.5 仍是魔法数字
- 未解决分布信息丢失问题

---

## 设计建议

### 短期（当前系统）

**保持现有 Min-Max 方案**，原因：

1. **权重配置已提供调优手段**：`vector_weight` 和 `bm25_weight` 允许用户根据实际效果调整路径重要性
2. **问题影响有限**：单结果场景在实际查询中占比较低
3. **无免费午餐**：任何归一化方案都有 trade-off，Min-Max 的简单性是优势

**建议补充**：
- 在配置中添加注释，说明单结果路径的行为
- 提供 `min_score` 阈值过滤低置信度融合结果

### 中期（如有需求）

**引入 Z-Score + Sigmoid 方案**：

```rust
pub struct ScoreNormalizer {
    vector_stats: RunningStats,  // 滑动窗口均值/方差
    bm25_stats: RunningStats,
}

impl ScoreNormalizer {
    fn normalize(&self, score: f32, path: Path) -> f32 {
        let stats = match path {
            Path::Vector => &self.vector_stats,
            Path::Bm25 => &self.bm25_stats,
        };
        let z = (score - stats.mean) / stats.std_dev;
        sigmoid(z)  // 映射到 [0, 1]
    }
}
```

**前提条件**：
- 需要分数统计收集机制
- 需要验证 Z-Score 确实改善检索质量（A/B 测试）

### 长期（大规模生产）

**考虑学习式校准**：
- 收集用户反馈（点击/采纳）作为标注信号
- 定期训练校准模型
- 将融合分数解释为 P(relevant)

---

## 结论

当前 Min-Max 方案是**工程上的合理折中**：
- 简单性 > 理论最优
- 配置权重已提供足够的调优空间
- 真正的问题是跨路径分数分布差异，这需要架构级的校准方案，而非参数调整

**不需要修改当前实现**。如果未来有检索质量瓶颈，再考虑引入 Z-Score 或学习式校准。
