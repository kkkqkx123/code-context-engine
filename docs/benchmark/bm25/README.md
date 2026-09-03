# BM25 文档目录

本文档目录涵盖项目 BM25 全文搜索模块的配置、实现和变更历史。

## 文档列表

| 文档 | 说明 | 状态 |
|------|------|------|
| [配置快速参考](./bm25-config-quick-reference.md) | BM25 参数说明、调优速查表、配置示例 | ✅ 最新 |
| [参数优化变更记录](./BM25参数优化变更记录.md) | k1=1.8/b=0.4 优化的变更历史 | ✅ 最新 |
| [实施总结](./IMPLEMENTATION_SUMMARY.md) | 两阶段实施（BoostQuery + Tantivy fork）的技术细节 | ✅ 最新 |

## 相关源码位置

| 组件 | 路径 |
|------|------|
| BM25 配置定义 | `crates/cce_core/src/config/modules/storage.rs` |
| BM25 检索实现 | `crates/cce_orchestrator/src/query/retrieval/bm25.rs` |
| Tantivy BM25 参数 | `crates/tantivy/src/index/index_meta.rs` |
| Tantivy BM25 评分 | `crates/tantivy/src/query/bm25.rs` |
| 索引管理器 | `crates/cce_infrastructure/src/storage/bm25/` |
| 配置示例 | `config.example.toml` |
| BM25 热更新处理器 | `crates/cce_orchestrator/src/hot_update/processors/bm25.rs` |
| BM25 检索策略 | `crates/cce_orchestrator/src/query/retrieval/strategies/bm25.rs` |

## 架构概览

```
用户配置 (config.toml)
    ↓
Bm25Config { algorithm: { k1, b }, field_weights }
    ↓
IndexSettings.bm25_params = Some(Bm25Params { k1, b })
    ↓
本地 Tantivy fork (crates/tantivy/)
    ├─ index_meta.rs: Bm25Params
    └─ query/bm25.rs: 使用 Bm25Params.k1, Bm25Params.b 计算评分

字段加权 (BoostQuery) 在检索层叠加:
    final_score = BM25(term, field) × field_weight
```

## 关键默认值

| 参数 | 默认值 | 说明 |
|------|--------|------|
| k1 | 1.8 | 词频饱和度，代码搜索优化 |
| b | 0.4 | 长度归一化，减少短实体惩罚 |
| title_weight | 3.0 | 实体名称权重 |
| content_weight | 1.0 | 描述文本权重 |
| keywords_weight | 2.0 | 关键词权重 |
