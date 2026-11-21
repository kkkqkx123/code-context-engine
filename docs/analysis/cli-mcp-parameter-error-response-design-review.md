# CLI / MCP 参数错误返回数据与设计评审

## 0. 结论摘要

对项目三个对外入口（CLI、MCP、HTTP API）在参数错误等场景下的返回数据进行取证后，结论如下：

| 入口 | 错误返回形态 | 结构化程度 | 能否指导 LLM 纠错 | 是否精简 |
|---|---|---|---|---|
| CLI | clap 解析错误走 stderr + 退出码；业务错误多为纯文本 `print_error`，且大量路径吞掉失败返回 `Ok(())` | 低（纯文本为主） | 部分（有人类可读文案，但 JSON 模式下错误不成 JSON） | 是 |
| MCP | 两类并存：协议级 `McpError::invalid_params`（JSON-RPC error）与业务级 `CallToolResult::error`（isError=true + 纯文本 ContentBlock） | 低（错误内容是纯文本字符串，非 JSON 对象） | 部分（文案可读，但无字段定位、无错误码、约束不进 schema） | 是 |
| HTTP API | `ErrorResponse { success, error: { code, message, details } }` + 错误码常量表 | 高（结构化、带 code） | 是（已有机器可读 code 与可选 details） | 是 |

**核心判断**：现有设计在「精简」上做得好，但在「让 LLM 正确理解问题并自我纠错」上存在系统性缺口；三个入口各有一套错误模型，**应当提供一个统一的领域错误抽象**，但不应把它做成重抽象，而应收敛为一个共享的错误值对象 + 分层映射。详见第 6、7 节。

---

## 1. 取证范围与证据锚点

- CLI：`crates/app/cce-cli`，入口 `src/main.rs`、`src/cli.rs`，输出工具 `src/output.rs`，各命令 `src/commands/*`。
- MCP：`crates/app/cce-mcp`，入口 `src/main.rs`、`src/server.rs`，处理器 `src/handler*`。
- HTTP API：`crates/app/cce-api/src/models/response.rs`、`crates/app/cce-server/src/api/response.rs`、`crates/app/cce-client/src/error.rs`。
- 依赖版本：`rmcp = "3.4"`（`Cargo.toml:222`）。
- 文档规范遵循 `AGENTS.md`：正文使用简体中文；代码标识、字段名保留英文；不贴大段完整代码，仅引用 `file:line` 与最小代码片段。

---

## 2. CLI 的错误返回数据

### 2.1 参数解析层（clap）

- `main.rs:25` 使用 `Cli::parse()`，参数缺失、类型错误、未知子命令由 clap 直接处理：打印人类可读错误到 stderr，并以非零退出码终止进程。
- 例：`cli.rs:143-206` 中 `project_id: i64` 是必填参数；缺失时 clap 报 `the following required arguments were not provided: --project-id <PROJECT_ID>`。
- 特点：这一层**不经过项目代码**，错误信息由 clap 按终端习惯生成，含用法提示，对 LLM 可读性尚可，但格式与后续业务错误完全不同。

### 2.2 业务校验层（anyhow）

- `main.rs:23` 返回 `anyhow::Result<()>`，`cli.rs:159` 的 `execute` 同样返回 `anyhow::Result<()>`。
- 参数合法性校验用 `anyhow::bail!`，例如 `agg_search.rs:159-170`（至少一个子查询、权重范围）、`index.rs:163`（push 模式仅支持 `index run`）、`process.rs:26`（二进制覆盖路径不存在）。
- 这类错误冒泡到 `main`，由 Rust 运行时打印 `Error: <msg>` 到 stderr，退出码非零。文案是英文短句，能说明问题，但**没有错误码、没有字段路径**。

### 2.3 请求/服务错误层（print_error）——问题集中区

- 统一输出函数在 `output.rs:16-18`：`print_error` 仅 `eprintln!("{} {}", 红色图标, message)`，纯文本，无结构化载体。
- 关键事实：**绝大多数网络/服务错误在 `cce-cli` 中被打印后吞掉**。例如 `search.rs:130-133`：`Err(e)` 分支执行 `print_error(&format!("Search failed: {e}"))` 后返回 `Ok(())`；agg_search、entity、graph、project、storage 等命令同样如此（21 个命令文件共 126 处 `print_error`）。后果：CLI 进程**退出码为 0**，调用方（脚本或 LLM）无法用退出码判断失败，只能解析 stderr 文本。
- `--format json` 只影响成功输出：`output.rs:47-51` 的 `print_output` 在 Json 模式下打印 pretty JSON；但错误路径始终走 `print_error`（如 `search.rs:106-108` 与 `126-133` 的分叉）。因此 **JSON 模式下，失败返回的不是 JSON**，破坏了机器读者的契约一致性。
- 反例（正确做法的雏形）：`ClientError`（`cce-client/src/error.rs:33-76`）把失败分为 `Config / Unavailable / Unauthorized / Rejected / Protocol`，并提供 `status()`、`code()`、`details()`、`symbol_candidates()`、`is_retryable()` 等访问器（`error.rs:144-205`）。这套结构化信息在 CLI 层被 `print_error` 压平成一句文本，机器可读字段在展示层丢失。

---

## 3. MCP 的错误返回数据

### 3.1 两类错误通道并存

MCP 存在两条语义不同的错误路径，这是理解其返回数据的关键：

1. **协议级错误**：`Result<CallToolResult, McpError>` 中的 `Err(McpError)`，经 rmcp 框架转为 JSON-RPC error（`isError` 不存在于响应体，走协议错误对象）。项目内唯一使用点是 `handler.rs:74-82` 的 `ensure_enabled`：工具不在允许列表时返回 `McpError::invalid_params("tool '<name>' is disabled by configuration", None)`。此外，参数反序列化和 JSON Schema 校验失败由 `Parameters<T>`（`handler.rs:37`、各 `#[tool]` 签名）交给 rmcp 框架处理，通常也表现为协议级 invalid params；此路径在项目代码中无显式分支。

2. **业务级错误**：返回 `Ok(CallToolResult::error(...))`，即响应体 `isError: true`，正文是一个 `ContentBlock::text(纯文本字符串)`。项目内共 26 处此类返回（不含测试），分布在 `handler/support.rs`、search、relations、project、fold 各子模块。

### 3.2 参数错误的实际文案

- 缺失/类型错误：交给框架，文案来自 serde/rmcp，项目不控制（无 `#[validate]` 类注解，见 3.4）。
- 枚举值非法：`search.rs:183-193` 用共享 `SearchService::parse_search_sources`，错误信息为 `Invalid query type: <value>`（`cce-server-shared/src/services/search.rs:252-264`）；在 MCP 侧被包装为 `"search failed: {error}"`（`search.rs:189-191`）。同类还有 `Unknown exclude content type: <raw>`、`Unknown <scope> category: <raw>`、`Unknown query intent: <label>`（`services/search.rs:267-303`）。
- 数值范围非法：`search.rs:406-411` 对 `file_search` 的 `limit` 校验，文案 `file search failed: invalid limit {limit}: must be 1..=100`。
- 关系字段非法：`relations.rs:132-142` 对 `call_chain` 的 `direction`，文案 `call_chain failed: direction must be one of down, forward, up, or backward, got '<raw>'`。
- 结构形状非法：`project.rs:154-159` 对 `incremental_index` 的 `deleted`，文案 `` `deleted` must be empty, length 1, or match the number of files ``。
- 包装方式不统一：部分用 `support::tool_failure` / `tool_error`（`support.rs:11-21`，如 relations、fold_batch），部分直接内联 `CallToolResult::error(vec![ContentBlock::text(format!(...))])`（如 search），`aggregated_search` 还在函数内自定义局部闭包 `fail`（`search.rs:290-294`）。

### 3.3 错误正文不是结构化数据

这是与 LLM 交互最相关的事实：

- 所有业务错误的正文都是**单个文本块**，内容是英文句子（如 `search failed: Invalid query type: vectorx`），而不是形如 `{"error":{"code":"INVALID_INPUT","field":"query_type",...}}` 的 JSON 对象。
- 对比：成功结果的正文虽然是文本块，但内容是 `json!({...}).to_string()`（如 `search.rs:254-271`、`project.rs:77-85`）。也就是说**成功是 JSON 字符串、失败是普通句子**，消费者需要两套解析逻辑。
- 错误文案里已包含操作名（`<op> failed:`）和具体值/合法范围，人类读者可理解；但缺少稳定错误码、字段路径、期望值/实际值的分离字段，LLM 只能靠自然语言推断如何修正。

### 3.4 参数约束未进入工具 Schema

- 所有参数结构体均 derive `schemars::JsonSchema`（如 `search.rs:17`、`fold.rs:12`、`project.rs:18`），但全仓库没有任何 `#[schemars(range(...))]`、`#[schemars(length(...))]` 或其他取值约束注解（全局检索无命中）。
- 后果：`limit` 的 `1..=100`（`search.rs:407`）、`sub_queries` 的 1..10（文档描述，`search.rs:88`）、`direction` 的枚举集合（`relations.rs:38-39`）等约束**只存在于 doc 注释和运行时校验**中。LLM 在调用前看到的 schema 只有类型信息，无法预知合法取值，只能先犯错再从文本错误里学。
- 校验层级也存在差异：聚合搜索的子查询数量与权重校验在编排层（`cce-orchestrator/src/query/aggregation.rs:69-74`），MCP 侧只对空文本做了前置校验（`search.rs:296-300`），因此不同错误的文案来源并不一致。

---

## 4. HTTP API 的错误返回数据（对比基线）

- 统一错误信封：`ErrorResponse { success: bool, error: ErrorDetail { code, message, details? } }`（`cce-api/src/models/response.rs:24-65`）。
- 错误码常量表：`INVALID_REQUEST / INVALID_INPUT / ENTITY_NOT_FOUND / ... / AMBIGUOUS_SYMBOL`（`response.rs:68-82`）。
- 服务端组装：`ApiResult` 在 `into_response` 中把错误映射为带 HTTP 状态码的 `ErrorResponse`（`cce-server/src/api/response.rs:27-48`）。
- 客户端消费：`ClientError::from_failure` 解析错误信封，分离 `code / message / details`，并按状态码分类（`cce-client/src/error.rs:206-264`）。注释明确指出 `details` 与 `message` 分离的原因：让调用方无需重新解析自然语言即可拿到候选 ID（`error.rs:62-68`、`174-184`）。
- 启示：HTTP 侧已经实践了「结构化错误 + 机器可读字段 + 展示与数据分离」的正确方向，且正是为了让调用方（含自动化客户端）能据此行动。MCP 与 CLI 没有复用这套成果。

---

## 5. 设计合理性评估

### 5.1 做得合理之处

- **不让失败静默变成空成功**：MCP 的 `tool_error` 注释明确「不可用索引不被替换为空成功负载」（`support.rs:6-10`）；这是面向 LLM 的正确选择，避免 LLM 把「索引不可用」误判为「没有结果」。
- **错误文案含操作名与合法范围**：如 `must be 1..=100`、`one of down, forward, up, or backward`，对自我纠错友好。
- **成功负载字段复用 API DTO 名称**：`handler.rs:9-11` 说明 SearchResultItem 等字段与 HTTP 一致，降低跨入口认知成本。
- **HTTP 侧结构化错误设计成熟**：code + message + details 三元组是良好的最小结构。

### 5.2 不合理 / 有风险之处

| 编号 | 问题 | 证据 | 对 LLM 的影响 |
|---|---|---|---|
| A | 错误内容非结构化（MCP） | `support.rs:19-21`、search 各错误分支 | LLM 需从句子里抽取字段，无法稳定程序化处理 |
| B | 参数约束未进 schema | 全局无 schemars 约束注解；`search.rs:407` 运行时才校验 | LLM 首次调用易失败，增加往返与 token |
| C | 错误通道不统一（MCP） | `ensure_enabled` 用协议错误，业务校验用 isError=true（`handler.rs:76` vs `support.rs:20`） | 同一类「参数/权限问题」表现为两种协议形态，消费者难以统一处理 |
| D | 错误前缀与措辞不一致 | `search failed:` / `file search failed:` / `aggregated_search failed:`（`search.rs`）、`call_chain failed:`（`relations.rs:138`）、无前缀的 `` `deleted` must be... ``（`project.rs:157`） | LLM 难以用统一规则归因 |
| E | CLI 吞掉失败（退出码 0） | `search.rs:130-133` 等 126 处 `print_error` | 脚本/LLM 无法用退出码判断成败 |
| F | CLI JSON 模式下错误非 JSON | `output.rs:47-51` 仅成功走 JSON；错误恒走 `print_error` | 破坏机器读者契约，需混用 stdout/stderr 解析 |
| G | 三入口三套错误模型 | CLI `print_error` / MCP `CallToolResult::error` / HTTP `ErrorResponse` | 领域错误语义重复实现，易漂移 |
| H | 结构化信息在展示层丢失 | `ClientError` 有 `code()/details()`（`error.rs:144-192`），但 CLI 压平为文本 | 机器可读字段（含候选实体 ID）退化为句子 |

### 5.3 关于「精简」与「可理解」的平衡

- 现状偏向极简：错误正文通常只有一句英文，token 开销低。
- 缺失的是**最小必要结构**，而非更多文字。可在不增加冗余的前提下，把同一句话拆成 `code + message + field + expected/actual` 四个短字段；总 token 与现在相当，但机器可解析性显著提升。
- 约束前置（补 schema 注解）比优化错误文案更能减少总消耗：调用前就知道合法值，可避免整轮失败往返。

---

## 6. 是否需要统一抽象：需要，但应是薄抽象

### 6.1 判断：需要

理由：

1. 三个入口共享同一个领域层（`SharedContext` / `SearchService` / relation service），同一领域错误却被表达成三种互不相识的形态（第 4、5 节），违反「领域语义单一来源」。
2. 已有反证说明统一是有价值的：`cce-api::ErrorResponse` 与 `cce-client::ClientError` 已经为 HTTP 链路提供了 code/details 分离的能力（`response.rs:23-65`、`error.rs:31-76`），并为此写了专门的测试（`error.rs:310-334` 验证 details 不被揉进 message）。MCP/CLI 没有理由重造一套更弱的。
3. 参数错误是 LLM 高频触发场景；统一抽象能一次性解决 A/C/D/G 四类问题。

### 6.2 建议的形态：领域错误值对象 + 分层映射（薄抽象）

建议引入一个**与传输无关的领域错误值对象**（置于 core 或 server-shared 层），最小字段集：

- `code`：稳定错误码，复用或扩展 `error_codes` 常量（`response.rs:68-82`）。
- `message`：一句人类/LLM 可读说明。
- `field`：出错的参数名（可选）。
- `expected` / `actual`：合法范围与收到值（可选，参数错误时填充）。
- `retryable`：是否可重试（对齐 `ClientError::is_retryable` 的语义，但由领域层给出，而非靠 HTTP 状态码猜）。
- `details`：机器可读附加数据（如候选实体 ID 列表，对齐现有 `details` 字段）。

映射规则（保持各入口协议习惯，只统一来源）：

- HTTP：领域错误 → 现有 `ErrorResponse`（几乎零改动，仅把领域错误码灌入 `code`）。
- MCP：领域错误 → 单个 JSON 文本块 `{"error":{...}}` + `CallToolResult::error`；协议级权限拒绝仍可用 `invalid_params`，但要在文档中固定「何时用协议错误、何时用 isError」的判定规则，消除 C。
- CLI：领域错误 → JSON 模式打印结构化 error 到 stdout，文本模式 `print_error` 到 stderr；失败返回非零退出码，修正 E/F。

### 6.3 为什么不推荐重抽象

- `AGENTS.md:20-22` 要求最小化 `dyn` 动态分发、优先确定性类型；因此不应设计错误 trait 对象或多层抽象继承。
- 一个 `enum`/struct 值对象 + 三个纯函数式映射即可，无需泛型层级。
- 保持「No-backward-compatible」前提（`AGENTS.md:3-4`）：可直接替换现有纯文本错误返回，无需兼容旧格式。

---

## 7. 面向 LLM 的改进优先级

| 优先级 | 动作 | 收益 | 关联问题 |
|---|---|---|---|
| P0 | 为参数结构体补充 schemars 取值约束（limit 范围、枚举值、数组长度） | 调用前即可知道合法值，直接减少失败往返 | B |
| P0 | MCP 业务错误正文改为结构化 JSON 错误对象（code/field/expected/actual/message） | LLM 可稳定解析并自我纠错 | A、D |
| P1 | 固定「协议错误 vs isError」判定规则，统一错误前缀 | 消除同类问题两种形态 | C、D |
| P1 | CLI 失败返回非零退出码，JSON 模式下错误也输出 JSON | 恢复机器读者契约 | E、F |
| P2 | 引入领域错误值对象，三入口分层映射 | 单一语义来源，长期可维护 | G、H |

关于精简：上述改动不追求更长文案。`code + field + expected + actual` 总长度与现有句子相当；约束前置还会压缩总体交互轮次。

---

## 8. 待拍板开放点

1. **错误码体系归属**：是复用 `cce-api::error_codes`（`response.rs:68-82`）并迁入 core 层，还是新建独立错误码表？若复用，需决定是否把 `INVALID_INPUT` 细分为 `INVALID_FIELD` / `OUT_OF_RANGE` / `INVALID_ENUM`。
2. **MCP 协议错误边界**：`ensure_enabled` 的「工具被配置禁用」应继续用 `invalid_params`（协议错误），还是也改为 `isError=true` 的应用错误？需要固定规则并写入 `docs/app/server/mcp-api.md`。
3. **CLI 退出码契约**：大量命令当前吞掉失败（`Ok(())`）。是否统一改为非零退出码？这会影响现有脚本，`AGENTS.md:3-4` 允许不兼容，但需确认调用方范围。
4. **schema 约束的粒度**：schemars 的 `range` / `length` 是否覆盖所有运行时校验（如 `deleted` 形状、`sub_queries` 1..10、weight (0,1]）？部分约束（数组间长度依赖）无法用注解表达，是否接受「schema 表达静态约束 + 错误对象表达跨字段约束」的分工？
5. **结构放置位置**：领域错误值对象放 `cce-types`（core）还是 `cce-server-shared`？前者更通用但需要错误码表下沉，后者改动小但 CLI 需增加依赖。

---

## 9. 参考

- 工具清单：`docs/app/server/mcp-api.md`
- CLI/API 对比：`docs/app/api/cli-api-comparison.md`
- 错误模型：`crates/app/cce-api/src/models/response.rs`、`crates/app/cce-client/src/error.rs`、`crates/app/cce-server/src/api/response.rs`
- MCP 处理器：`crates/app/cce-mcp/src/handler.rs` 及 `handler/` 子模块
- 项目规范：`AGENTS.md`
