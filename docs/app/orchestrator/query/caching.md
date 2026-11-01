# 查询缓存与容错设计

## 概述

查询路径的缓存与容错由四个组件承担：查询结果缓存、查询侧嵌入缓存、能力协商与重试队列。实现均位于 `crates/app/cce-orchestrator/src/query/`。

## QueryResultCache（cache.rs）

- 基于 `moka::future::Cache` 缓存**完整查询结果**，键为查询文本 + 全量搜索配置指纹。
- 配置指纹（`config_fingerprint`）把 `SearchConfig` 的全部子配置序列化为 JSON、**递归排序对象键后哈希**，保证相同配置必然得到相同键（map 迭代顺序按实例随机，不能直接哈希）。
- 缓存键还纳入 `QueryFilter`（epoch 视图），不同视图互不命中。
- 注意：查询文本的嵌入**不在这里缓存**。

## CachedEmbedder（cached_embedder.rs）

查询侧嵌入在 searcher 级别记忆化：所有查询侧 embedder 消费者共享同一个 `CachedEmbedder` 实例，避免相同查询文本重复调用嵌入服务。这是嵌入缓存的唯一位置，与结果缓存分层。

## IndexCapabilities（capabilities.rs）

coordinator 执行查询前做能力协商，避免对已禁用的索引发起请求导致运行期错误：

- `has_vectors` / `has_bm25` / `has_relations`：三类索引可用性。
- `relation_propagation_enabled`：跨文件依赖传播是否开启（`track_cross_file_deps`）；关闭时调用链查询可能缺跨文件边。
- `relation_propagation_depth`：热更新依赖传播深度上限（0 = 无限）；有限深度下，超出深度的传递性调用者不会被重新解析，深层调用链可能带陈旧/缺失的跨文件链接。

## RetryQueue（retry_queue.rs）

服务不可用时的查询进度保全：

- 召回路径在**重试全部耗尽**后，把 `QueryOptions` 入队；队列有界（默认 1024），满时丢最旧条目（尽力而为，客户端需重发）。
- 排空条件是**外部恢复信号**（如熔断器转为半开/闭合），由 `drain_ready` 取出待处理查询重新执行，而非定时轮询。
- 每条目记录入队时间与重试次数，超过最大重试次数后丢弃。

## 与熔断器的关系

cce-circuit-breaker（infra 层）监控向量 / 嵌入 / rerank 等远程服务的健康度；Reranker 在 provider 不健康时直接跳过重排（见 ranking.md），RetryQueue 则在恢复信号到达时回放积压查询，两者共同保证部分服务故障时查询路径只降级、不中断。
