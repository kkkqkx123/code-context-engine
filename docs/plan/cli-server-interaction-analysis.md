# CLI 客户端服务端交互问题分析与修改方案

## 现状概述

CLI（`crates/app/cce-cli`）是纯 HTTP 客户端，与服务端通过 REST API 解耦。分层结构：

- 入口层 `cli.rs`：clap 参数解析与命令分发。
- 契约层 `cce-api`：请求/响应模型、URL 路径常量。
- 传输层 `cce-transport`：`SharedHttpClient`，负责 base URL 规范化、Bearer token 解析（显式参数优先于 `CCE_API_TOKEN` 环境变量）、统一超时、状态码检查。
- 客户端层 `cce-cli/src/client.rs`：`ApiClient` 包装 `SharedHttpClient`，提供约 30 个强类型方法。

服务端生命周期由 `commands/server.rs` 独立管理（PID 文件 + 日志重定向 + 健康检查等待 + 优雅退出），业务命令不隐式拉起服务。

总体评价：分层清晰、契约共享、边界干净，服务生命周期管理健壮。以下为需要修正的问题清单与对应方案。

## 问题清单与修改方案

### 1. 命令分发风格不一致

**现象**：`cli.rs::execute` 中，index / search / agg_search / project / graph / watch / config 命令先构造 `ApiClient` 再传入 handler（`execute_with_client(cmd, &client, ...)`）；而 entity / storage / tools / qdrant / metrics / status / gateway / summary 只传 `&self.server` 字符串，由各 handler 内部自行构造客户端。

**问题**：同一能力两种写法，新增命令时容易选错模式；客户端构造逻辑分散在各 handler 中，难以统一调整（如注入 token）。

**方案**：统一为"入口构造客户端、handler 接收客户端"模式。`cli.rs::execute` 对所有业务命令先构造一次 `ApiClient` 再传入；将 entity / storage / tools / qdrant / metrics / status / gateway / summary 的 handler 签名改为接收 `&ApiClient`（内部若需要原始 server 字符串仅用于展示，可另行传递）。`commands/server.rs` 因涉及本机进程管理、不依赖 HTTP 客户端，保持现状。

### 2. 同步长阻塞的索引命令

**现象**：`index run` 发送一次 HTTP 请求并等待服务端完成整个索引流水线，共享超时 300 秒。大仓库索引时 CLI 挂住数分钟无进度反馈；超时后 CLI 报错，但服务端可能仍在执行。

**问题**：300 秒是实际可用性上限；CLI 已具备分批推送能力（`SharedHttpClient` 的 ingest manifest / batch / commit / event 方法，供 gateway 使用），但 index 命令未复用，形成两套并存的索引路径。

**方案**：为 `index run` 提供基于 ingest 分批 API 的任务化执行路径：CLI 本地扫描与分块后，通过 manifest → batch → commit 三阶段推送，每批即时返回，CLI 输出逐批进度；同步单请求路径保留为小目录的快捷方式（或根据目录规模自动选择）。超时配置从传输层常量改为可按命令覆盖。

### 3. 错误语义依赖字符串嗅探

**现象**：`ApiClient::map` 在将 `HttpTransportError` 映射为用户可读错误时，主路径之外还保留了一个对原始错误文本做 `contains("404")` / `contains("not found")` 判断的兜底分支。

**问题**：字符串嗅探脆弱，服务端错误信息措辞变化即失效；`HttpTransportError` 已携带结构化的 `status` 字段，无需解析文本。

**方案**：删除字符串嗅探兜底分支，统一以 `HttpTransportError::status` 判定错误类别（404 → not found，5xx → server error，其余 → transport error）；非 `HttpTransportError` 的错误直接透传 anyhow 链。补充针对 `map` 的单元测试覆盖各状态码。

### 4. 后台进程管理健壮性

**现象**：`server start` 在 spawn 后 `std::mem::forget(child)` 使服务进程脱离 CLI 生命周期；`process_alive` 仅检查 PID 是否存活，不校验进程身份。

**问题**：忘记 wait 导致子进程 zombie 资源由 init 回收（可行但非最佳）；PID 被操作系统复用给无关进程时，`stop` / `status` 会基于过时 PID 文件误判。

**方案**：
- 用 spawn 后立即 `tokio::spawn` 一个 wait 任务替代 `std::mem::forget`，仅用于回收子进程退出状态，不影响服务脱离 CLI 生命周期继续运行。
- 写 PID 文件时同时记录进程启动时间戳（`/proc/<pid>/stat` 或平台等价物），`process_alive` 校验 PID 与启动时间匹配后才认为 PID 文件有效；不匹配时按 stale PID 处理。
- 在 docs 中注明该取舍及平台差异。

### 5. ApiClient 薄包装样板

**现象**：`ApiClient` 对 `SharedHttpClient` 的 get / post / put / delete / delete_with_body 全部为逐字透传的包装方法，除错误映射 `map` 外无附加语义。

**问题**：约 50 行纯样板，新增传输方法需同步改两处。

**方案**：命令层直接使用 `SharedHttpClient`，将错误映射逻辑提取为 `SharedHttpClient` 上的扩展 trait（或独立的自由函数），`ApiClient` 类型取消。与问题 1 的统一改造一并完成，避免二次翻动调用点。

## 实施顺序建议

按依赖关系排序：

1. 问题 3（错误映射）——独立小改动，先行。
2. 问题 5 + 问题 1（取消 ApiClient、统一分发模式）——合并为一次重构，问题 3 的扩展 trait 为其铺垫。
3. 问题 4（进程管理健壮性）——独立改动。
4. 问题 2（索引任务化）——工作量最大、涉及 CLI 与服务端协议配合，最后实施；实施前需单独评审 ingest 分批路径与现有 index 路径的行为差异（缓存、断点续传、关系图构建）。

## 验证方式

- `cargo clippy --all-targets --all-features` 通过。
- CLI 契约测试（`client.rs` 内 stub server 测试）与 `cli.rs` 解析测试全部通过。
- 问题 2 落地后，对同一目录分别走同步路径与分批路径，比对索引结果统计（文件数、实体数、向量数）一致。
- 问题 4 落地后，手工验证 `server start/stop/status/restart` 全流程及 stale PID 场景。
