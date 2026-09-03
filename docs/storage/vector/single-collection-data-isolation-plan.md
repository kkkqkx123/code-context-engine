# 单集合数据隔离重构实施计划

## 目标

本文档用于把 [单集合数据隔离架构设计](./single-collection-data-isolation-design.md) 落到可执行的重构计划上。

确认的总方向是：

- 由“每个工作空间一个主集合 + 一个 summary 集合”改为“单一集合 + payload 逻辑隔离”
- 统一使用固定集合名 `cce_vectors`
- 通过 `group_id` 做项目/租户隔离
- 通过 `type` 区分 `chunk` 与 `summary`
- 将 summary 数据写入同一集合，不再维护独立 summary 集合
- Qdrant 仅保留向量存储与最小路径隔离字段，元数据查询走 SQLite

## 这次重构的边界

### 需要做的

- 调整 Qdrant payload 数据结构
- 调整集合创建逻辑
- 调整 summary 写入、删除、搜索逻辑
- 调整检索层过滤条件
- 收敛元数据查询职责到 SQLite
- 调整配置结构以支持 `payload_m`
- 补齐迁移路径与回归测试

### 暂不做的

- 不引入新的向量数据库后端
- 不改变现有 BM25 和 SQLite 的职责边界
- 不做跨后端统一抽象重构

## 当前问题清单

1. 目前 `summary` 相关数据仍然依赖独立集合，导致集合数量和维护成本随工作空间增长。
2. 现有 payload 缺少 `group_id`，无法做显式项目隔离。
3. 检索层仍然尝试承担元数据过滤职责，导致职责边界不清。
4. HNSW 配置不支持 `payload_m`，无法表达文档中要求的单集合过滤优化方案。
5. 现有写入、删除、搜索路径分散在主集合和 summary 集合两套实现里，重构时容易出现行为不一致。

## 需要修改的模块

### 1. `cce_core` 配置层

需要补齐 HNSW 配置字段，并让项目级覆盖配置能传递该字段。

- `crates/cce_core/src/config/modules/storage.rs`
  - 为 `HnswConfig` 增加 `payload_m`
  - 让各个 preset 的默认值与 `m` 保持一致
- `crates/cce_core/src/config/project.rs`
  - 为 `HnswConfigOverride` 增加 `payload_m`
- `crates/cce_core/src/config/global.rs`
  - 确保项目配置合并时不会丢失 `payload_m`

### 2. `cce_infrastructure` 向量存储层

需要把 payload 与集合管理收敛到单集合模型。

- `crates/cce_infrastructure/src/storage/qdrant/types.rs`
  - 为 `Payload` 增加 `group_id`
  - 为 `Payload` 增加 `type`
  - 删除 `summary_text`、`pattern_info`、`entity_type`、`file_extension`、`language` 等冗余字段
- `crates/cce_infrastructure/src/storage/qdrant/client.rs`
  - 去掉基于 workspace 生成集合名的逻辑
  - 改为固定集合名 `cce_vectors`
  - 移除 summary 集合相关字段与辅助方法
  - 保留现有客户端的生命周期、健康检查、指标和熔断逻辑
- `crates/cce_infrastructure/src/storage/qdrant/operations.rs`
  - 删除 `SummaryOperations`
  - 将 summary 写入、删除、搜索合并到主集合操作里
  - 创建集合时支持 `payload_m`
- `crates/cce_infrastructure/src/storage/qdrant/retrieval.rs`
  - 为检索过滤增加 `group_id`
  - 增加 `type` 过滤
  - 只保留路径相关过滤和内容排除规则，元数据查询移交 SQLite

### 3. `cce_orchestrator` 索引和查询层

需要把 summary 相关流程切换到单集合模式。

- `crates/cce_orchestrator/src/index/storage_coordinator.rs`
  - 初始化时只创建一个 Qdrant 集合
  - `store_summaries()` 改为向主集合写入 `type = summary`
  - `remove_file_from_summary()` 改为按 `file_path + type = summary` 删除
- `crates/cce_orchestrator/src/query/boost/summary.rs`
  - summary boost 查询改为搜索主集合
  - 搜索时附加 `group_id` 与 `type = summary`
- `crates/cce_orchestrator/src/query/retrieval/strategies/dense.rs`
  - dense 检索必须显式带上项目隔离条件
  - 只保留路径过滤，不再把元数据过滤压到 Qdrant

## 需要先确认的关键点

这部分不是实现细节，而是避免“看起来改完了，实际上检索失效”的关键决策。

1. `group_id` 的生成规则是否固定为工作空间路径哈希，还是改由项目注册表或配置提供。
2. 是否允许旧集合与新集合在迁移窗口内并存。
3. 迁移时是否要求一次性全量迁移，还是支持分批迁移与回滚。
4. 元数据查询接口是否需要立即切换到 SQLite 过滤，还是先保留现有输入参数但不再走 Qdrant。

## 实施顺序

### 第一阶段：数据结构与配置

- 增加 `payload_m`
- 收缩 payload 结构
- 定义 `group_id` 生成方式
- 调整集合名为固定值

### 第二阶段：写入与删除路径

- 合并 summary 写入逻辑
- 统一删除逻辑
- 保证写入 payload 的字段一致

### 第三阶段：检索路径

- 改造 dense 检索过滤
- 改造 summary boost 搜索
- 清理对元数据 payload 字段的依赖
- 清理对旧 summary 集合的依赖

### 第四阶段：迁移与清理

- 设计旧集合到新集合的导入流程
- 校验索引后删除旧集合
- 清理无用代码与测试用例

### 第五阶段：回归验证

- 单元测试覆盖 payload、配置、过滤构造
- 集成测试覆盖写入、搜索、删除、summary boost
- 迁移测试覆盖旧数据导入

## 验收标准

满足以下条件后，重构可以视为完成：

- 只存在一个业务集合 `cce_vectors`
- summary 数据不再依赖独立集合
- 所有查询都能按 `group_id` 做显式隔离
- 现有 summary boost 与向量检索结果一致
- 配置层能正确传递 `payload_m`
- 相关测试通过，且没有遗留的 summary 集合调用点

## 风险与说明

- 目前设计文档没有把所有检索过滤字段的存储策略说完整，所以实现前必须先确认过滤语义。
- 如果只保留 `group_id`、`type` 和路径过滤，而不在 SQLite 侧补齐元数据查询，部分高级筛选能力会退化。
- 迁移阶段需要明确旧集合是否允许保留一段时间，否则回滚成本会变高。
