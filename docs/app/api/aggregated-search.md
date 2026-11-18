# 聚合搜索 API

## POST /api/search/aggregated

执行多路子查询检索，并把命中按统一对齐键合并成一份结果。与单查询端点不同，子查询由调用方显式给出，服务端不负责把一句自然语言自动拆分。

项目配置 `[search.aggregation].score_strategy` 控制同一对齐键上多条命中如何合并：

- `max`（默认）：只保留最强的一条贡献，不因多个弱命中叠加而抬升排序；
- `noisy_or`：按 `1 - Π(1 - score)` 累积一致性证据，结果保持在 `[0, 1]`。适合词面互补、需要累积证据的子查询组合，需显式开启。

策略通过共享查询服务进入执行层，并计入缓存键；修改策略后不会复用旧策略的结果。

合并分数的含义是"相对该子查询最佳命中的匹配强度 × 权重"：每条子查询的候选在合并前按其自身最大值缩放。单路召回（`vector` / `bm25`）与混合召回（`hybrid`）的内部刻度不同，缩放后两者的最佳命中都对齐到 1.0，`min_score` 才对所有子查询含义一致。

### 与单一查询端点的对比

| 场景 | 推荐端点 |
|---|---|
| 简单关键词或语义搜索 | `POST /api/search` |
| 多意图、词面互补的复杂查询 | `POST /api/search/aggregated` |

聚合不是全局增益开关。离线评测显示收益高度依赖子查询组合；同类语义子查询可能降低召回质量。建议先用少量子查询做评测，再决定是否启用。

### 请求

**方法**：`POST`

**Content-Type**：`application/json`

请求体主要字段：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `project_id` / `project_path` | number / string | 二选一 | 项目定位，不可同时提供 |
| `query` | string | 否 | 用户原始问题，作为合并后 rerank 的语义上下文；留空时回退为子查询文本拼接 |
| `sub_queries` | array | 是 | 子查询列表，1 到 10 条 |
| `limit` | number | 否 | 最终返回数量，默认 10，最大 100 |
| `min_score` | number | 否 | 合并后的全局分数阈值 |
| `directory_prefix` | string | 否 | 目录前缀过滤 |
| `exclude_content_types` | string[] | 否 | 排除 test、generated、dependency 等内容 |
| `include_categories` / `exclude_categories` | string[] | 否 | 文件分类过滤 |
| `include_patterns` / `exclude_patterns` | string[] | 否 | glob 路径过滤 |
| `enable_rerank` / `rerank_max_candidates` | bool / number | 否 | 重排覆盖 |
| `query_intent` | string | 否 | 应用到所有子查询的意图，留空则逐条自动识别 |
| `expand` | bool | 否 | 是否在合并后的 Top-N 上做关系扩展 |

`sub_queries` 每项：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `text` | string | 是 | 非空查询文本 |
| `query_type` | string | 否 | `vector`、`bm25`、`hybrid`，默认 `hybrid` |
| `weight` | number | 否 | 合并前的分数缩放，必须有限且在 `(0, 1]`，默认 1.0。`max` 策略下必须大于 `min_score`，否则该子查询的任何命中都过不了阈值，请求会被拒绝 |

调用示例：

```bash
curl -X POST "http://localhost:9000/api/search/aggregated" \
  -H "Content-Type: application/json" \
  -d '{
    "project_id": 1,
    "sub_queries": [
      { "text": "user authentication", "query_type": "vector", "weight": 1.0 },
      { "text": "login validate", "query_type": "bm25", "weight": 1.0 }
    ],
    "limit": 10
  }'
```

### 响应

响应中的 `items` 与单查询端点共用同一结果结构，主要字段包括 `file_path`、`code_chunk`、`start_line`、`end_line`、`entity_type`、`entity_names`、`entity_ids`、`stable_symbol_ids`、`score`、`sources`、`content_state`、关系扩展计数、`secondary_evidence` 和 `entity_scores`。

响应级附加字段：

| 字段 | 类型 | 说明 |
|---|---|---|
| `success` | bool | 是否成功 |
| `total` | number | 本次返回的结果数量 |
| `elapsed_ms` | number | 服务端执行耗时 |
| `sources_used` | string[] | 所有子查询涉及的检索来源 |
| `failed_sub_queries` | string[] | 失败的子查询文本；部分失败时仍返回其余结果 |
| `planned_sub_queries` | number | 请求提交的子查询数量；单查询为 1 |
| `executed_sub_queries` | number | 本次真正进入检索管线的子查询数量；聚合缓存命中时为 0 |
| `succeeded_sub_queries` | number | 贡献了候选列表的子查询数量 |
| `sub_query_cache_hits` | number | 其中由子查询缓存直接满足的数量 |
| `from_cache` | bool | 聚合结果是否命中共享结果缓存；不反映子查询层缓存 |
| `relation_epoch` / `relation_stale` | number / bool | 关系快照版本与新鲜度 |
| `sub_query_stats` | array | 逐子查询明细，见下表；单查询与"整体命中缓存"的聚合响应为空数组 |

结果项附加字段：

| 字段 | 类型 | 说明 |
|---|---|---|
| `contributing_sub_queries` | number | 有多少条子查询在该结果的对齐键上产生了命中；单查询为 0 |

`sub_query_stats` 每项：

| 字段 | 类型 | 说明 |
|---|---|---|
| `index` | number | 在请求子查询列表中的位置（从 0 开始） |
| `text` | string | 实际执行的查询文本 |
| `status` | string | `succeeded`（有候选）、`empty`（跑完但无命中）、`failed`（报错）、`timed_out`（被子查询截止时间切断） |
| `candidates` | number | 对齐、合并前召回的候选数 |
| `contributed` | number | 最终列表里由该子查询贡献的条数（合并、阈值、数量限制之后） |
| `elapsed_ms` | number | 该子查询的墙钟耗时 |
| `error` | string? | `failed` 与 `timed_out` 的原因 |

`candidates` 大而 `contributed` 为 0，说明该子查询召回的候选都被更强的邻居在合并时吃掉；`candidates` 为 0 且状态是 `empty`，说明意图本身没命中。这是调权重与判断"要不要继续拆子查询"的直接依据。

全部子查询失败或超时，请求返回错误，不再返回 `success: true` 的空结果。部分子查询失败或超时时请求成功，并通过 `failed_sub_queries`（文本）与 `sub_query_stats`（明细）暴露降解情况；这种结果不会写入聚合缓存，后续相同请求会重新尝试失败的子查询。

### 执行语义

1. 所有子查询共享同一索引 epoch view 和全局过滤条件。
2. 重复子查询按文本、来源和权重折叠；零权重与超过 1 的权重被拒绝；`max` 策略下权重不大于阈值 `min_score` 的子查询在计划阶段被拒绝（它必然零贡献）。
3. 每条子查询以关闭阈值、放大的候选上限运行召回管线，关闭重复计数、rerank 和结果过滤；最多 4 条并发执行。
4. 合并前把每条子查询的候选按其自身最大值缩放，使各子查询的最佳命中都对齐到 1.0——单路召回与混合召回的原始刻度不同，只有先对齐才能比较。
5. 权重在合并前缩放分数并统一到 `[0, 1]`，随后按统一对齐键折叠，并按配置的策略累积证据；开启 chunk 分组时对齐键优先使用 chunk id，避免同一段代码因代表实体不同而重复出现。
6. 对合并后的唯一列表执行一次 rerank 与结果过滤，然后应用一次全局阈值、排序和数量限制；排序在同分时让完整正文优先于引用，与单查询端点一致。
7. 关系扩展只作用于最终 Top-N，避免每个子查询重复扩展；展开结果按关系快照版本缓存。
8. 子查询受项目搜索超时的约束，但会预留一段预算给合并后的重排、阈值与扩展：先到期的子查询被单独判为 `timed_out`，其余子查询的结果照常合并返回（`success: true` + `failed_sub_queries`）。只有整单在请求级超时仍未完成时才返回超时错误。
9. 子查询失败不进入重试队列：单条子查询脱离它所属的合并没有意义，重放只会写入调用方不会用到的缓存；部分失败的响应本身就是一次有效答复。
