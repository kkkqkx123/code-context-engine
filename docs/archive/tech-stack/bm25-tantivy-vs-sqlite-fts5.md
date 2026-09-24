# BM25 全文检索选型分析：Tantivy vs SQLite FTS5

## 结论

继续使用 Tantivy（vendored fork），不迁移到 SQLite FTS5。

## 背景说明

项目中 BM25 全文检索由 `crates/infra/cce-storage-bm25` 提供，底层为 vendored 的 tantivy fork（upstream 0.27 + `feat/add-configurable-k1-b` 分支，经 `[patch.crates-io]` 重定向到 `crates/tantivy` 子模块）。本文记录"是否应改用 SQLite 原生 FTS5"的评估结论。

## 选型依据

### 项目对 Tantivy 的依赖深度

评估时逐项核对了当前实现，依赖能力远超"能做全文搜索"这一基本需求：

| 依赖点 | 当前用法 | FTS5 可替代性 |
|---|---|---|
| 自定义分词器 | `MixedTokenizer`（jieba 中文分词 + 代码感知切分，见 `cce-text`），通过 tantivy `Tokenizer` trait 注册为 `mixed`，title/content/keywords 三字段共用 | 不可行。FTS5 自定义分词器须以 C 实现并编译进 SQLite，Rust 侧 jieba 逻辑需跨 FFI 重写 |
| BM25 参数可调 | fork 新增 `IndexSettings.bm25_params`，k1/b 全局可配（代码搜索场景 b=0.6） | 不可行。FTS5 仅支持按列配置 `bm25()` 权重，k1/b 固定 |
| 零得分污染过滤 | epoch/project/category 过滤以 `BoostQuery(0.0)` 包裹，过滤约束不改变 BM25 排名，保证与离线基准评分器逐分对齐及跨 epoch 排名一致 | 部分可行。FTS5 可在 SQL 层过滤，但得分语义需自行重建 |
| 多字段加权与 term 级 operator | `field_weights` 按 field 加权、`TermOperator`（AND/OR）、split-token 降权、raw+clean 双表单查询 | 部分可行。FTS5 无原生多字段加权查询，需手写评分逻辑 |
| 高亮 | `highlight.rs` 基于 tantivy 位置信息生成高亮片段 | 部分可行。FTS5 有 `highlight()`/`snippet()`，但换分词器后行为不一致 |
| 评分可复现性 | 离线差分测试（e2e `bm25_parity`）要求基准评分器与生产检索逐分一致（idf 全局 doc 数、fieldnorm 量化等） | 不可行。迁移等于重写离线评分器与全部 parity 测试 |

其中"k1/b 可调 + 多字段加权 + 自定义分词"正是 fork tantivy 的直接原因，说明 tantivy 已是深度定制组件而非随意选型。

### FTS5 优势的价值评估

FTS5 的真实优势为：少一个存储引擎、与 SQLite 元数据同库事务、部署简单。逐一对照本项目：

- **同库事务收益低**：BM25 索引是可重建的派生数据（索引工作流已有 content-hash 缓存校验与 checkpoint 机制），无需与元数据强一致；epoch 机制已承担索引视图一致性。
- **部署简化有限**：tantivy 为纯 Rust 嵌入式库，无外部服务（外部依赖是 Qdrant 而非 tantivy），迁移 FTS5 不减少运维负担，仅少几个 vendored 子模块。
- **规模不构成约束**：代码库索引的文档量级（万至十万 chunk）两者均轻松承载；多字段加权、过滤聚合等场景下 FTS5 反而需手写更多逻辑。

### 重新考虑 FTS5 的触发条件

仅当以下条件同时成立时才值得重新评估：

1. 放弃 jieba/代码混合分词，或愿意维护 C 实现的 FTS5 tokenizer；
2. 接受 BM25 k1/b 固定不可调；
3. 愿意重写离线 parity 评分体系及全部回归测试；
4. 强需求：BM25 索引与关系索引/缓存须在同一 SQLite 文件内原子更新。

当前四条均不成立。

## 已知维护成本

vendored tantivy fork 是当前方案的主要维护成本：

- 升级 upstream 时需 rebase fork，并保持工作区 `Cargo.toml` 中声明的 tantivy 版本与 fork 自身 `Cargo.toml` 版本一致，否则 `[patch.crates-io]` 静默失效、构建回落到 crates.io 版本（缺少 `IndexSettings.bm25_params`）；
- `[patch.crates-io]` 中除 tantivy 外还防御性列出了 8 个子 crate（columnar、sstable、stacker 等），升级时需一并核对；
- 上述风险由 e2e 的 bm25 parity 回归测试守卫。
