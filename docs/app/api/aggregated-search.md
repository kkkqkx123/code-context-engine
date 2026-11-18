# 聚合搜索 API

## POST /api/search/aggregated

执行多路子查询检索，并把命中按统一对齐键合并成一份结果。与单查询端点不同，子查询由调用方显式给出，服务端不负责把一句自然语言自动拆分。

项目配置 `[search.aggregation].score_strategy` 控制同一对齐键上多条命中如何合并：

- `noisy_or`（默认）：按 `1 - Π(1 - score)` 累积一致性证据，结果保持在 `[0, 1]`；
- `max`：只保留最强的一条贡献。

策略通过共享查询服务进入执行层，并计入缓存键；修改策略后不会复用旧策略的结果。

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
| `weight` | number | 否 | 合并前的分数缩放，必须有限且非负，默认 1.0 |

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
| `sub_queries_count` | number | 实际执行的子查询数量；单查询为 1 |
| `from_cache` | bool | 是否命中共享结果缓存 |
| `relation_epoch` / `relation_stale` | number / bool | 关系快照版本与新鲜度 |

结果项附加字段：

| 字段 | 类型 | 说明 |
|---|---|---|
| `contributing_sub_queries` | number | 有多少条子查询在该结果的对齐键上产生了命中；单查询为 0 |

全部子查询失败时请求返回错误，不再返回 `success: true` 的空结果。部分子查询失败时请求成功，并通过 `failed_sub_queries` 暴露降解情况。

### 执行语义

1. 所有子查询共享同一索引 epoch view 和全局过滤条件。
2. 每条子查询以关闭阈值、放大的候选上限运行完整检索管线；最多 4 条并发执行。
3. 权重在合并前缩放分数，随后按统一对齐键折叠，并按配置的策略累积证据。
4. 合并后应用一次全局阈值、排序和数量限制。
5. 关系扩展只作用于最终 Top-N，避免每个子查询重复扩展。
6. 整个聚合请求受项目配置的搜索超时约束；超时会取消尚未完成的子查询。
