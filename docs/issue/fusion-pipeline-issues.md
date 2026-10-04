# Fusion 后流水线设计问题分析

## 1. 完整流水线概览

```
Fusion → Glob Filter → Score Normalization → Summary Boost → SQLite Enrichment → Rerank → Sort → Result Filter → Threshold → Relation Annotation
```

## 2. Assembly 模块重命名（已完成）

### 现状

该模块已完成重命名，当前公开类型和模块路径均以关系标注职责命名：`RelationAnnotator`、`RelationAnnotationConfig`、`AnnotatedResult`、`AnnotationMetadata`、`AnnotationError` 与 `query::annotation`。

该模块的实际职责是：

- **不做图遍历**：调用方自行解析 forward/backward expansion units，annotator 只接收现成的 `Vec<ExpandedUnit>`
- **不做图结构输出**：输出是纯文本拼接（`AnnotatedResult.annotated_content`），不是图结构
- **实际职责**：去重、预算裁剪、按结构顺序拼接、添加关系标注注释（`// [calls] name (file:lines)`）

### 已完成修改

| 项目 | 结果 |
|------|------|
| 模块命名 | `assembly` 已改为 `annotation` |
| 类型命名 | 不再使用 `Assembly` / `Assembled` 作为模块语义 |
| 注释清理 | 不再把该模块描述为 SPSR 图组装 |
| 未使用字段 | `ExpandedUnit.depth` 已移除 |

## 3. SQLite Enrichment：附加文本分析

### 附加的文本内容

`materialize()` 函数对每个结果附加以下信息：

| 字段 | 来源 | 内容 |
|------|------|------|
| `file_path` | ChunkRecord | 文件绝对路径 |
| `start_line` / `end_line` | ChunkRecord | 行号范围 |
| `kind` | ChunkRecord | chunk 类型（function/class/method 等） |
| `name` | ChunkRecord + entity_ids | 实体名称 |
| `content` | 磁盘源文件 | 完整源码文本（或降级为引用行） |
| `snippet` | 磁盘源文件 | 同 content |
| `content_state` | 计算 | Full / Reference(OverLimit) / Reference(FileMissing) / Reference(FileLevel) |
| `entity_ids` | ChunkRecord | 实体 ID 列表（回退填充） |
| `truncated` | ChunkRecord | 是否被截断 |

### 合理性分析

**合理的部分**：
- 从磁盘读取最新内容而非使用索引快照，保证内容时效性
- 降级机制（文件缺失/超限→引用行）避免返回空内容
- token 预算控制防止超大文件撑爆响应

**不合理的部分**：

1. **`snippet` 与 `content` 完全重复**：`materialize()` 中 `snippet = content.clone()`，没有独立价值
2. **`file_path` 覆盖 payload 携带的值**：如果 payload 已有 file_path（来自索引），用 ChunkRecord 覆盖可能引入不一致
3. **降级为 file reference 时 score 未调整**：用户看到的"文件级命中"和"chunk 级命中"在分数上无法区分
4. **`name` 选择逻辑过于简单**：`choose_entity_name()` 只取第一个非空名称，多实体场景下可能选错

### 修改方案

1. **移除 `snippet` 字段**：API 响应中未使用，内部也不需要两份相同数据
2. **file_path 优先使用 payload 值**：仅在 payload 缺失时从 ChunkRecord 填充
3. **降级时降权**：file reference 结果 `score *= 0.8`，让 chunk 级命中自然排前
4. **多实体名称拼接**：多实体时 `name = names.join(", ")` 而非取第一个

## 4. 查询相关信息在最终结果中的暴露

### 内部 SearchResult 字段分类

| 类别 | 字段 | 是否应暴露 |
|------|------|------------|
| **用户需要** | `id`, `file_path`, `content`, `start_line`, `end_line`, `kind`, `name`, `entity_ids` | 是 |
| **调试用** | `score`, `original_score`, `vector_score`, `bm25_score` | 仅 `score` |
| **纯内部** | `sources`, `is_boosted`, `boost_reason`, `metadata`, `pattern_info`, `category`, `segment_id`, `snippet`, `truncated` | 否 |

### API 响应现状

`SearchResultItem` 已做了较好的裁剪，只暴露 9 个字段。但仍有问题：

1. **`score` 暴露**：用户无法理解 fusion/boost 后的分数含义，不同查询间分数不可比
2. **`source` 暴露**：`"hybrid"` / `"vector"` / `"bm25"` 是内部策略名，用户不关心
3. **`entity_ids` 暴露**：内部 ID，用户无法使用

### 修改方案

1. **移除 `score` 字段**：搜索结果按相关性排序即可，绝对分数无意义
2. **移除 `source` 字段**：或改为用户可理解的描述（如 "语义+关键词"）
3. **移除 `entity_ids`**：或改为 `entity_names`（用户可理解的实体名列表）
4. **保留 `content_state`**：用户需要知道内容是完整代码还是引用

## 5. 流水线架构问题

### 5.1 双重归一化冲突

`apply_score_normalization` 在 fusion 之后又做了一次归一化，但 fusion 内部的 WeightedMinMax 已将分数映射到 `[0, w_v + w_b]`。外部再归一化会：
- 破坏 fusion 算法精心构造的分数分布
- RRF 的 `min_score` 语义被改变

**方案**：让外部 normalization 只在非 fusion 路径执行，或让 fusion 算法输出已归一化的分数。

### 5.2 Boost 语义不一致

- Summary boost: `score × (1 + add)` — 乘性
- ResultFilter boost: `score += boost` — 加性

**方案**：统一为乘性，或在统一 boost aggregator 中处理。

### 5.3 ScoreSorter 缺少 tie-breaking

`score_sorter.rs` 只有 `b.score.partial_cmp(&a.score)`，没有 tie-breaking。fusion 内部的 `sort_fused_by_score` 有完整 tie-breaking 链。

**方案**：复用 fusion 的 tie-breaking 逻辑（`entity_id → segment_id → chunk_id`）。

### 5.4 Enrichment 降级后无二次去重

`dedup_by_chunk_id` 在 fusion 阶段按 chunk id 去重，但 enrichment 阶段可能将同一 file 的不同 chunk 都降级为 file reference，产生重复。

**方案**：enrichment 后增加按 file_path 的去重步骤。

## 6. 修改优先级

| 优先级 | 项目 | 原因 |
|--------|------|------|
| P0 | 移除 API 响应中的 `score` | 用户不可理解，且不同查询间不可比 |
| P0 | ScoreSorter 增加 tie-breaking | 影响结果稳定性 |
| P1 | 解决双重归一化冲突 | 影响分数语义正确性 |
| P1 | 统一 boost 语义 | 影响结果一致性 |
| P1 | Relation Annotation 重命名 | 影响代码可维护性 |
| P2 | Enrichment 降级降权 | 影响结果排序质量 |
| P2 | 移除 snippet 字段 | 减少内存/序列化开销 |
| P2 | Enrichment 后二次去重 | 边界情况去重 |
