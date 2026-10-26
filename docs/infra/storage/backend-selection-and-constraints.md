# 存储后端选择与模式约束

## 1. 目的

本文档整理当前项目存储后端的选择逻辑，以及本地模式与服务端模式下的存储后端约束，便于后续对照配置、装配与运行时行为。

## 2. 存储后端概览

项目按数据访问模式拆分三类存储：

- 向量存储：语义搜索，支持内嵌引擎或 Qdrant 服务
- 全文存储：关键词搜索，支持 Tantivy 或 Elasticsearch 服务
- 关系存储：元数据、关系图、checkpoint、manifest，当前仅支持 SQLite

各后端通过契约接口统一读写语义，装配层按配置构造具体客户端，运行时通过枚举分发。

## 3. 配置入口

后端选择集中在数据库配置，由三个枚举控制：

- 向量后端：`local` 或 `qdrant`
- 全文后端：`local` 或 `remote`（Elasticsearch）
- 关系后端：`local` 或 `remote`（PostgreSQL，预留）

每个后端拥有独立参数块，避免配置互相污染。默认组合为 `local / local / local`。

## 4. 支持矩阵与约束

### 4.1 当前支持矩阵

当前实际支持的组合如下：

- `local / local / local`
- `qdrant / local / local`
- `local / local / remote`
- `qdrant / local / remote`

其中三个位置分别表示：向量后端 / 关系后端 / 全文后端。

### 4.2 硬性约束

- 关系后端当前必须为 `local`（SQLite）。`remote`（PostgreSQL）在配置校验阶段直接拒绝，因为相关写入与控制面尚未接线。
- 引擎元数据存储固定为 SQLite 客户端。项目注册、操作协调、checkpoint 清理、metrics 聚合、generation GC 等控制面依赖 SQLite。
- 向量后端可为 `local` 或 `qdrant`，切换需要重建索引。
- 全文后端可为 `local`（Tantivy）或 `remote`（Elasticsearch），切换需要重建索引。
- `allow_nonstandard_backends` 仅用于放行非预设组合，不能绕过关系 remote 的未接线限制。

### 4.3 切换成本

- 切换向量后端需要全量重建索引，不迁移旧数据。
- 切换全文后端需要全量重建索引，不迁移旧数据。
- 关系后端当前不可切换。

## 5. 本地模式与服务端模式

### 5.1 模式说明

当前代码没有独立的 `service_mode` 开关。所谓本地模式与服务端模式，是部署形态与客户端形态的组合：

- 本地模式：单进程运行，优先使用内嵌后端
- 服务端模式：运行 HTTP 服务，客户端通过 HTTP 访问

CLI 为纯 HTTP 客户端；`server` 与 `mcp` 是本地生命周期命令，业务命令一律走远程 API。

### 5.2 本地模式下的存储约束

- 向量：可使用内嵌 `local`，也可连接外部 Qdrant
- 全文：可使用内嵌 Tantivy，也可连接外部 Elasticsearch
- 关系：必须使用 SQLite
- 所有内嵌后端要求同进程与本地文件系统访问

### 5.3 服务端模式下的存储约束

- 向量：可使用内嵌 `local`，也可连接外部 Qdrant
- 全文：可使用内嵌 Tantivy，也可连接外部 Elasticsearch
- 关系：当前仍必须使用 SQLite，且该 SQLite 位于服务端进程所在环境
- PostgreSQL 关系后端尚未接线，不能在服务端模式下使用
- 远程 Qdrant/Elasticsearch 适合服务端模式下的外部化部署，但关系数据仍留在服务端本地 SQLite

## 6. 后端处理方式

### 6.1 向量模块

- 本地分支：内嵌 simvec，进程内调用，无断路器与重试
- 远程分支：Qdrant HTTP 客户端，具备断路器、重试、指标、进程管理、诊断与容量预设
- 装配层按配置构造对应客户端
- 运行时所有路径通过 `VectorStore` 枚举分发
- 状态接口通过后端中立诊断快照暴露健康与计数

### 6.2 全文模块

- 本地分支：Tantivy 进程内索引
- 远程分支：Elasticsearch HTTP 客户端，具备断路器、批量写入、刷新可见性、快照回读、计数与清理
- 启动时本地分支打开索引，远程分支确保索引存在
- 运行时所有路径通过 `FulltextStore` 枚举分发
- 状态接口通过后端中立诊断快照暴露健康与文档数

### 6.3 关系模块

- 当前仅 SQLite 分支可用
- PostgreSQL 分支为预留，配置与客户端实现可能部分存在，但装配层拒绝启用
- 元数据与控制面深度依赖 SQLite 特有能力
- 状态接口通过后端中立诊断快照暴露数据库大小

## 7. 术语约束

- `local` 表示内嵌存储或本地服务形态，不应混指主机、LLM 服务或命令形态
- `remote` 表示外部服务后端，不应与客户端远程访问语义混淆
- 向量后端使用具体品牌名 `Qdrant`
- 全文后端使用 `local / remote`，其中 remote 当前对应 Elasticsearch
- 关系后端使用 `local / remote`，其中 remote 预留对应 PostgreSQL

## 8. 运行时分发约束

- 三个存储句柄均为枚举，运行时通过模式匹配分发
- 新增方法必须同步维护所有分支
- 未接线分支应在装配阶段失败，不应在运行时静默降级
- 诊断与状态路径应使用后端中立快照，避免 downcast

## 9. 相关文档

- `docs/plan/storage-backend-selection-design.md`
- `docs/plan/remote-local-service-mode-analysis.md`
- `docs/storage/architecture-overview.md`