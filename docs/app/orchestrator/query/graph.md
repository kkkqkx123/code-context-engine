# 关系图查询服务设计

## 概述

`query/graph` 是一条与语义检索**相互独立**的图检索路径：回答邻域、路径、连通分量、导出等图形态的问题。它复用 `RelationSearcher` 的遍历原语，但不做任何文本评分；语义搜索结果也不会嵌入图数据（graph.rs 模块文档，`graph.rs:1-6`）。

- 代码位置：`crates/app/cce-orchestrator/src/query/graph/{model.rs, service.rs}`
- 数据源：`RelationSearcher` 持有的 `RelationIndexView`（LayeredSnapshotIndex 快照视图，见 tools/module-design.md 对数据源的说明），只读。

## 数据模型（model.rs）

节点-链接结构：扁平节点表 + 扁平边表，是在内存服务、HTTP graph 端点与未来离线消费者之间的共享契约。

- `GraphNode`：稳定 id（符号 id 优先，实体 id 兜底）、label、kind、source_file/source_location，可选 scoped_name 与 signature。
- `GraphEdge`：source/target、relation（稳定关系类型字符串）、**domain**（`call` / `dependency` / `structural` / `reference` / `template` / `other`，权威粗分类）、confidence、call_context、is_external、weight（关系强度 × 调用热度，缺省 1.0）、cfg_condition（条件编译谓词）、callsite（调用点字节区间，用于区分同一实体对之间的多次调用）、edge_id（对 `(source, target, relation[, callsite])` 的内容摘要，跨方向产生的同一条边可被识别为同一条）。
- `Confidence`（`confidence_of`，model.rs:40-50）：
  - `EXTRACTED` —— 源码显式声明（直接调用、import）；
  - `INFERRED` —— 解析阶段推导（方法分派、跨文件链接）；
  - `EXTERNAL` —— 指向项目外索引范围。

## GraphService 能力（service.rs）

包装 `Arc<RelationSearcher>` 构造（service.rs:42-50），主要查询：

- `ego_graph(root, depth, direction)` —— 单实体 ego 邻域，`GraphDirection::{Forward, Backward, Both}` 控制边方向；单次操作节点上限 `MAX_GRAPH_NODES = 10_000`（service.rs:39）。
- `shortest_path`（含 `shortest_path_with_options`）—— 两实体间最短路径；**不可达返回 `Ok(None)` 而非报错**（可达性本身是问题答案）。`GraphFilter` 约束的是遍历过程而不只是返回边：被过滤拒绝的边不会被跨越，因此 `domains=call` 得到的是纯调用路径，而非事后裁剪（service.rs:94-100）。
- 导出子图（induced subgraph）、连通分量（`PagedComponents`）、全量导出——分页结果见 `GraphPagination`。

路径查询细节由 `RelationSearcher` 提供（`PathQueryOptions`），图服务只做组合与物化。

## 与其他子系统的关系

- `query/relation_searcher.rs`：底层遍历原语（邻居枚举、路径选项），GraphService 的唯一依赖。
- `query/capabilities.rs`：`IndexCapabilities` 声明关系索引是否可用、跨文件依赖传播是否开启及其深度上限；coordinator 在执行前据此拒绝不支持的图查询，避免运行期错误。传播深度有限时，超过深度的传递性调用者可能带陈旧边，深层调用链可能缺跨文件链接（capabilities.rs:37-44）。
- HTTP 层：节点-链接契约直接映射到 `docs/app/api/graph.md` 描述的对外端点。
