# 错误码迁入 core、MCP 错误契约与 schema 约束改造方案

## 0. 文档状态与范围

- 状态：**第 3、4、5、6 节全部已实现并通过编译与单测**。落地状态见 0.1。
- 前置分析：`docs/analysis/cli-mcp-parameter-error-response-design-review.md`。
- 本轮用户决议：
  1. 错误码迁入 core 层；
  2. MCP 工具错误设计由本方案判断后决定；
  3. schema 按本方案修改，接受 schema 表达力局限；
  4. CLI 失败统一非零退出码——先出方案与错误码表格（首轮），下一阶段实现（本轮已完成，见第 6 节与 0.3）。
- 规范遵循：见 `AGENTS.md`。文档用简体中文，代码标识保留英文；设计正文以自然语言描述为主，仅给出接口签名级 diff，不贴完整实现。

### 0.1 落地状态（实现后同步）

| 章节 | 状态 | 落地说明 |
|---|---|---|
| 第 3 节 错误码迁入 core | 已完成 | 新增 `cce-types/src/error_codes.rs`（12 个常量，值不变）；门面引用 33 处已全部直连 `cce_types::error_codes`，`cce-api` 不再 re-export、不再依赖 cce-types（O2 落地） |
| 第 4.4 节 映射下沉 | 已完成 | `relation_error_parts` 落于 `cce-server-shared/services/relation.rs`；HTTP `relation_error` 改为调用它 |
| 第 4 节 MCP 结构化错误 | 已完成 | `tool_failure` / `tool_failure_with_details` / `relation_tool_error` 落地；26 处错误构造全部改为结构化 JSON；工具禁用（`ensure_enabled`）改为返回 `Option<CallToolResult>`，统一走 `isError` 通道（O3 落地为「单一通道」）；O4 落地：partial 改用成功通道、保留诊断 payload |
| 第 5 节 schema 约束 | 已完成 | 4 个 Args 文件共 30 处注解 + O5 的 7 处数组元素枚举注解；新增 3 个 schema 断言单测；修正 `sub_queries` 文档 1..10 → 1..16 与 direction 文案补 callees/callers |
| 第 6 节 CLI | 已完成 | 新增 `cce-cli/src/error.rs`（`CliError` + 分层退出码）；`main` 改显式 `process::exit`，统一 stderr 渲染（JSON 模式结构化信封）；126 处 `print_error` 中约 90 处失败路径改为返回 `Err`，保留信息性输出；新增 5 个单测（详见 0.3） |

落地时的事实修正（相对设计时的假设）：

- **rmcp 实际解析版本为 3.11.0**，非文档 F5 所述的 3.4（`Cargo.toml` 的 `"3.4"` 是 caret 语义）。`ContentBlock::Text(TextContent { text, .. })` 结构在本版本确认，单测据此断言。
- **index_project 的 partial 路径**（`project.rs`）保留原结果 payload 结构，未套用错误信封；O4 已在 0.2 重新拍板为「改用成功通道 + 保留诊断 payload」并落地（与 HTTP 的 success+`success:false` 对齐）。
- **fold 的 text 字段**未加 `length(min=1)`：fold 运行时不拒绝空文本，加约束会与运行时行为不一致，故按 5.4「以运行时为准」原则跳过。

### 0.2 开放点 O2 / O4 / O5 的后续拍板（本轮新增并落地）

用户要求对 O2、O4、O5 进一步分析合理设计、明确方向并完成修改。结论如下，三项均已落地：

**O2 — 彻底删除 cce-api 的 re-export（采纳）。**
理由：`AGENTS.md:3-4` 明确 No-backward-compatible，不为兼容牺牲架构。实测门面引用共 33 处（全部在 `cce-server` 的 30 个 src 与 3 个 tests，多为 `use cce_api::models::{..., error_codes}` 多行 brace 形式；`cce-server/Cargo.toml:30` 已依赖 cce-types），client 未引用错误码。re-export 的唯一作用是避免改这些 import，收益极小；保留它反而让 cce-api 被迫依赖 cce-types（唯一用途即此转发），并使"错误码真值源"出现两个可访问路径。改为：删除 `response.rs` 的 `pub use`，移除 cce-api 对 cce-types 的依赖，33 处 import 全部改为 `cce_types::error_codes`。真值源唯一收敛到 core。
（注：设计时 F2 估计为「30+ 文件」，首轮一度按行首单行形式低估为 12 处；逐文件核验后确认为 33 处，已全部切换，编译通过。）

**O4 — partial 结果不套错误信封，改用成功通道（采纳，修正原暂定决策）。**
关键事实：HTTP 侧 `handle_project_index` 对 `result.is_success()==false` 的 partial 返回 `ApiResult::Success(ProjectIndexResponse { success: false, ... })`（`handlers/project/indexing.rs:88-97`），即 HTTP 200、仅用 payload 内 `success` 布尔字段表达部分成功，从不使用 error 信封。MCP 侧却对同一语义用 `CallToolResult::error`（`project.rs:143-146`），三端语义不一致。若按原 O4 给 partial 套统一错误信封 `{"error":{code,...}}`，会丢弃 payload 中 `failed_files`/`errors` 等结构化诊断信息（这些都是给 LLM 纠偏的有用数据）。决策：partial 改用 `CallToolResult::success`，保留原 payload（含 `status:"partial"` 与 `errors` 明细），与 HTTP 的 success+`success:false` 语义对齐；错误通道专用于真正的执行失败（`Err(error)` 分支，仍为 `INTERNAL_ERROR` 结构化信封）。这样错误信封只表示"调用失败"，成功 payload（含 partial）保留完整诊断数据，二者解析规则互不干扰。

**O5 — 数组元素枚举用 `extend("items" = {...})` 补齐（采纳）。**
关键事实：`inner` 只接受 validation 类属性（`length`/`range`/`pattern` 等，`schemars_derive-1.2.3/src/attr/validation.rs:209`），`extend` 是顶层属性（`attr/mod.rs:124`），故 `inner(extend(...))` 编译报错；`inner(pattern(...))` 语法亦不通。隔离探针验证 `extend("items" = { "type":"string", "enum":[...] })` 在 `Option<Vec<String>>` 上正确生成 `"type":["array","null"],"items":{"type":"string","enum":[...]}`，保留 description 且覆盖 items，是表达数组元素枚举的正确方式。合法字面量取自运行时：`FileCategory::from_name`（`file_category.rs:118-126`：code/config/documentation/schema/other）与 `parse_exclude_content_type`（`services/search.rs:267-277`：test/tests、generated/generate、dependency/dependencies/vendor）。覆盖字段：`exclude_content_types`、`include_categories`、`exclude_categories`（在 `SearchArgs` / `AggregatedSearchArgs` / `FileSearchArgs` 三处重复定义）。局限照旧：注解只影响 schema、不改变 serde，运行时校验保留兜底（5.4）。

### 0.3 第 6 节 CLI 改造的落地说明（本轮新增）

基于用户推送后的新基线（`8c64c78b`，含前序全部改动）完成 CLI 阶段。相对第 6 节设计的具体落地差异：

- **入口机制**：`main` 从 `-> anyhow::Result<()>` 改为无返回，显式 `process::exit(status)`。设计中的 `print_error_and_fail` 实际落地为 `error.rs` 的 `CliError` + 三个辅助构造（`fail` / `fail_input` / `fail_client`），返回 `anyhow::Error` 供 `?` / `return Err` 冒泡；`main` 统一渲染，避免在每个命令里重复打印。
- **退出码来源**：取 `ClientError::is_retryable()` / `code()`（`cce-client/src/error.rs:149,165`）映射——retryable/`SERVICE_UNAVAILABLE` → 4，服务端 `code` 为 `ENTITY_NOT_FOUND` → 3、`INVALID_*`/`AMBIGUOUS_SYMBOL` → 2，其余 → 1；本地输入校验用 `INVALID_INPUT` → 2。退出码分层按 6.2 建议值落地（O6 以此为准，可再调整）。
- **错误码获取路径修正**：6.2 原写「CLI 经 `cce_api::models::error_codes` 获取」。因 O2 已删除 cce-api re-export，CLI 直连 `cce_types::error_codes`（`cce-cli/Cargo.toml` 新增 cce-types 依赖）。
- **失败路径改造范围**：126 处 `print_error` 中，约 90 处为真实失败分支，改为返回 `Err(CliError)`；约 20 处为信息性输出予以保留——诊断明细（`errors` 数组遍历、config 校验的 Error 级告警、索引的 circuit breaker 警告）、合法状态显示（status/health 的「不健康」是正常结果）、逐项部分失败（summary/batch compress 的单条错误）。这类输出不构成命令失败，不影响退出码。
- **JSON 模式错误**：stderr 输出 `{"error":{"code","message"}}`，stdout 仍只承载成功 payload。原 health check / status 在 JSON 模式把失败写成 stdout 的 `success:false` 对象，本轮统一收敛到 stderr 错误信封（避免一处失败两种解析规则）。
- **工具链**：新基线 `Cargo.toml` 将 `rust-version` 提升到 1.97，本地以 `cargo +1.97.0` 完成 fmt/clippy/test（环境默认 1.93 不满足）。
- **验证**：`cargo +1.97.0 test -p cce-cli` 39/39 通过（含 5 个新增退出码/错误渲染断言）；`clippy -p cce-cli --all-targets` cce-cli 自身零告警零错误；`fmt` 已执行。

## 1. 事实依据（带 file:line）

| 编号 | 事实 | 证据 |
|---|---|---|
| F1 | 错误码当前定义在 API 层，12 个 `&'static str` 常量 | `crates/app/cce-api/src/models/response.rs:68-82` |
| F2 | 错误码经 `cce_api::models::error_codes` 被 30+ 文件引用，覆盖 server handlers、server tests、client | 全局 `error_codes` 引用（见分析文档） |
| F3 | `cce-api` 仅依赖 serde/serde_json/utoipa，**不依赖** `cce-types` | `crates/app/cce-api/Cargo.toml:10-13` |
| F4 | `cce-mcp`、`cce-server`、`cce-server-shared` 均已依赖 `cce-types` | 各自 `Cargo.toml`：`cce-mcp:28`、`cce-server:30`、`cce-server-shared:23` |
| F5 | `cce-mcp` 依赖 `schemars = "1.0"`，实际解析为 1.2.3；rmcp 声明 `"3.4"`，实际解析为 3.11.0 | `crates/app/cce-mcp/Cargo.toml:25`、`Cargo.toml:222` |
| F6 | MCP 工具参数结构体共 12 个，全部 derive `JsonSchema`，但无任何取值约束注解 | `handler/{search,fold,relations,project}.rs` 各 Args 定义 |
| F7 | MCP 业务错误共 26 处 `CallToolResult::error`/`tool_failure`，正文为纯文本句子 | `handler/support.rs:19-21` 及各子模块 |
| F8 | MCP 唯一协议级错误为工具禁用，返回 `McpError::invalid_params` | `handler.rs:74-82` |
| F9 | HTTP 已有结构化 `ErrorResponse { success, error: { code, message, details? } }` 和错误码表 | `cce-api/src/models/response.rs:24-82`；server 映射 `api/response.rs:42-68` |
| F10 | client 侧 `parse_error_body` 解析 `ErrorResponse`，分离 code/message/details | `cce-client/src/error.rs:249-264` |
| F11 | 子查询上限 `MAX_SUB_QUERIES = 16`，但 mcp-api 文档写 1..10 | `cce-orchestrator/src/query/aggregation.rs:23` vs `docs/app/server/mcp-api.md` |
| F12 | schemars 1.2.3 支持 `range(min,max)`、`length(min,max)`、`extend("enum")=...`；**不支持** `exclusive_min` | 隔离探针实测（见第 5 节） |

## 2. 设计总览与分层决策

三端共享同一领域层，但错误表达各自为政（F7/F8/F9）。本方案确立一条主线：

> **错误码是真值源，收敛到 core（`cce-types`）；传输层只做映射，不再各自定义码值。**

分层归属：

| 层 | 职责 | 本次动作 |
|---|---|---|
| core / `cce-types` | 持有唯一错误码表 | 新增 `error_codes` 模块 |
| `cce-api` | wire contract 门面，转发 core 错误码 | re-export，保持现有 30+ 引用点不变（零破坏） |
| `cce-server` | HTTP 映射（错误码 → HTTP 状态 + ErrorResponse） | 不改逻辑，仅依赖来源切换到 core |
| `cce-mcp` | MCP 映射（错误码 → 结构化 tool error）+ schema 约束 | 本次实现 |
| `cce-cli` | 失败非零退出码 | 下一阶段（第 6 节仅方案） |

关于 No-backward-compatible（`AGENTS.md:3-4`）：re-export 不是为兼容旧调用方，而是维持 `cce-api` 作为 wire contract 门面的架构职责（其 `lib.rs:3-8` 明确只放 transport facts）。因此 server/client 继续从 `cce_api::models` 取码是分层结果，不是历史包袱。

## 3. 决策一：错误码迁入 core 层

### 3.1 决策

- 新建 `crates/core/cce-types/src/error_codes.rs`，把 F1 的 12 个常量原值迁入，模块公开为 `cce_types::error_codes`。
- 在 `cce-types/src/types.rs` 或 `lib.rs` 用 `pub mod error_codes;` 暴露。选择放在 `lib.rs` 顶层（与 `build_system`/`path` 同级），因为错误码是全域 wire 语义，不属于 `types::*` 领域类型集合。
- `cce-api` 新增 `cce-types` 依赖；在 `models/response.rs` 删除本地 `error_codes` 定义，改为 `pub use cce_types::error_codes;`。这样 F2 的全部引用点路径不变，零改动。
- `cce-server`、client、tests 无需改 import（仍走 `cce_api::models::error_codes`）。
- `cce-mcp` 新增/改用 `cce_types::error_codes`（它已依赖 cce-types，F4）。

### 3.2 接口签名级 diff

```diff
+// crates/core/cce-types/src/error_codes.rs (new)
+/// Stable wire error codes shared by HTTP, MCP and CLI surfaces.
+pub mod error_codes { /* 12 个 const，值不变 */ }

// crates/core/cce-types/src/lib.rs
+pub mod error_codes;

// crates/app/cce-api/Cargo.toml
+cce-types = { version = "0.1.0", path = "../../core/cce-types" }

// crates/app/cce-api/src/models/response.rs
-pub mod error_codes { /* ... */ }
+pub use cce_types::error_codes;
```

### 3.3 约束与验证点

- 12 个常量**值必须逐字不变**（F1），因为它们是 wire 契约，client 与前端按值判断。
- 不新增 `dyn`、不使用 `unwrap`（`AGENTS.md:15-22`）。
- 验证：`cargo check -p cce-api -p cce-server -p cce-client -p cce-mcp` 全绿；错误码测试（`query_error_codes_stable` 等）保持通过。

## 4. 决策二：MCP 工具错误设计（本方案判断）

### 4.1 判断结论

**采用单一结构化错误对象 + 单一错误通道**，替换当前"协议错误 + 纯文本 isError 并存"的双形态（F7/F8）：

- 所有工具执行期的失败（参数校验、服务不可用、实体查找失败、写入失败）统一返回 `CallToolResult::error`，内容是**单个结构化 JSON 文本块**，形态与成功响应一致（成功也是 JSON 文本块，见分析文档 3.3）。
- 工具被配置禁用（`ensure_enabled`，F8）从 `McpError::invalid_params` 改为同一结构化错误对象，使 LLM 面对的所有工具失败都走同一解析路径。
- 参数反序列化失败（缺字段/类型错）由 rmcp 框架在 `Parameters<T>` 层处理，属于协议级 `invalid params`，项目不拦截——这是框架行为，保留。

### 4.2 结构化错误对象形状

复用 HTTP 的错误信封语义（F9），但去掉 HTTP 专属的顶层 `success`（MCP 已用 `isError` 表达成败），得到最小结构：

```
{ "error": {
    "code": "INVALID_INPUT",        // 来自 core error_codes，稳定可判定
    "message": "search failed: Invalid query type: xyz",  // 人类/LLM 可读
    "details": "<可选，机器可读附加信息>"                  // 沿用 HTTP details 语义
} }
```

字段设计理由：

| 字段 | 是否必填 | 理由 |
|---|---|---|
| `code` | 必填 | LLM 可稳定分支判断，不依赖解析自然语言；值来自 core，与 HTTP 一致 |
| `message` | 必填 | 保留现有可读文案（含操作名、具体值、合法范围），指导纠错 |
| `details` | 可选 | 对齐 HTTP `ErrorDetail.details`（如歧义符号候选 ID 列表），沿用既有语义，不新增概念 |

字段精简性：相较现状（纯文本一句），只多一个 `code`；不引入 `field`/`expected`/`actual` 等更细字段——合法范围已写在 `message` 里（如 `must be 1..=100`），重复会膨胀 token，违背"信息尽量精简"。

### 4.3 错误码映射表（MCP 内部）

| MCP 场景 | code | 依据 |
|---|---|---|
| `search`/`aggregated_search` 的 query type / content type / category / intent 非法 | `INVALID_INPUT` | 对应 HTTP search handler 同类错误用 `INVALID_INPUT`（`search.rs:100` 等） |
| `file_search` 的 project_id<=0 / query 空 / limit 越界 | `INVALID_INPUT` | 参数语义错误 |
| `call_chain` direction 非法 | `INVALID_INPUT` | 同上 |
| `incremental_index` deleted 形状非法 | `INVALID_INPUT` | 同上 |
| 工具被 enabled_tools 禁用 | `INVALID_REQUEST` | 请求不被接受，非参数值本身错误 |
| relation 服务失败（callees/callers/call_chain） | 按 `RelationServiceError` 映射，见 4.4 | 与 HTTP 映射对齐 |
| 其他服务/执行失败 | `INTERNAL_ERROR` | 兜底 |

### 4.4 RelationServiceError → 错误码映射（与 HTTP 对齐）

| RelationServiceError 变体 | code | HTTP 现有映射证据 |
|---|---|---|
| `InvalidInput` | `INVALID_REQUEST` | `cce-server/src/api/response.rs:44-46` |
| `Project` / `Execution` | `INTERNAL_ERROR` | `response.rs:47-49` |
| `UnknownSeed` | `ENTITY_NOT_FOUND` | `response.rs:50-54` |
| `AmbiguousSeed` | `AMBIGUOUS_SYMBOL` + details=候选 JSON | `response.rs:55-62` |
| `Unavailable` | `SERVICE_UNAVAILABLE` | `response.rs:63-66` |

该映射当前在 HTTP 层实现（`cce-server`，F9）。为避免 MCP 复制一份导致漂移，本方案把映射逻辑抽成纯函数下沉到 `cce-server-shared`（`RelationServiceError` 的定义地 `services/relation.rs:33`），HTTP 与 MCP 共同调用：

- 新增 `cce-server-shared` 中的 `relation_error_parts(error) -> (&'static str, String, Option<String>)`，返回 (code, message, details)。
- `cce-server/src/api/response.rs:42-68` 的 `relation_error` 改为调用它并构造 `ErrorResponse`。
- `cce-mcp/src/handler/support.rs:11-16` 的 `tool_error` 改为调用它并构造结构化 MCP 错误。

这是"统一抽象"的落点：一个纯函数、无 `dyn`、无状态，仅映射语义（符合 `AGENTS.md:20-22`）。

### 4.5 接口签名级 diff

```diff
// cce-server-shared (new, e.g. services/relation/error_contract.rs)
+pub fn relation_error_parts(
+    error: RelationServiceError,
+) -> (&'static str, String, Option<String>);

// cce-mcp/src/handler/support.rs
-pub fn tool_error(operation: &str, error: RelationServiceError) -> Result<CallToolResult, ...> {
-    Ok(tool_failure(format!("{operation} failed: {error}")))
-}
-pub fn tool_failure(message: String) -> CallToolResult {
-    CallToolResult::error(vec![ContentBlock::text(message)])
-}
+pub fn tool_failure(code: &'static str, message: String) -> CallToolResult;
+pub fn tool_failure_with_details(
+    code: &'static str, message: String, details: String,
+) -> CallToolResult;
+pub fn relation_tool_error(
+    operation: &str, error: RelationServiceError,
+) -> Result<CallToolResult, rmcp::ErrorData>;
```

### 4.6 影响面与验证点

- 26 处业务错误构造点需传入 code（F7）。它们是机械替换，message 文案保持不变，降低 LLM 可观察行为的变化幅度。
- `index_project` 的部分成功（partial）路径返回结构化 payload（`project.rs`）——O4 已定案：改用 `CallToolResult::success` 通道、保留 payload 内的 `status` 与 `errors` 诊断字段，仅真正的执行失败走结构化错误信封（见 0.2）。
- 验证：新增单元断言——`tool_failure` 产出的文本可被 `serde_json` 解析且 `error.code` 非空；relation 映射与 HTTP 映射逐变体一致。

## 5. 决策三：schema 约束注解

### 5.1 决策

为 F6 的 12 个 Args 结构体补充 schemars 注解，让 LLM 在调用前就知道合法范围，减少失败往返。注解值**以运行时校验为准**，不以文档为准（F11：文档写 1..10，代码是 16，取 16）。

### 5.2 约束清单

| 结构体 | 字段 | 注解 | 运行时依据 |
|---|---|---|---|
| `SearchArgs` | `project_id` | `range(min = 1)` | 项目 ID 正整数语义 |
| `SearchArgs` | `limit` | `range(min = 1, max = 100)` | 与 file_search 一致的上限约定（`search.rs:407`） |
| `SearchArgs` | `query_type` | `extend("enum"=["vector","bm25","hybrid"])` | `parse_search_sources`（`services/search.rs:256-260`） |
| `SearchArgs` | `query_intent` | `extend("enum"=["semantic","keyword","hybrid","entity"])` | `parse_query_intent`（`services/search.rs:293-297`） |
| `AggregatedSubQueryArg` | `text` | `length(min = 1)` | 空文本拒绝（`search.rs:298`） |
| `AggregatedSubQueryArg` | `query_type` | 同 SearchArgs | 同上 |
| `AggregatedSubQueryArg` | `weight` | `range(min = 0.0, max = 1.0)` | 实际 (0,1]；开区间无法表达，见 5.4 |
| `AggregatedSearchArgs` | `sub_queries` | `length(min = 1, max = 16)` | `MAX_SUB_QUERIES=16`（F11） |
| `AggregatedSearchArgs` | `limit` / `query` 同 SearchArgs | 同上 | |
| `FileSearchArgs` | `project_id` | `range(min = 1)` | `search.rs:395` |
| `FileSearchArgs` | `query` | `length(min = 1)` | `search.rs:401` |
| `FileSearchArgs` | `limit` | `range(min = 1, max = 100)` | `search.rs:407` |
| `KeywordSearchArgs` | `project_id` | `range(min = 1)` | 项目 ID 语义 |
| `KeywordSearchArgs` | `term_operator` | `extend("enum"=["or","and"])` | `KeywordOperator::parse` |
| `CallChainArgs` | `direction` | `extend("enum"=["forward","down","callees","backward","up","callers"])` | `CallDirection::parse`（`relation.rs:251-252`） |
| `CallChainArgs` | `max_depth` | `range(min = 0)` | 0 表示无限制（relation_support.rs:27） |
| `FoldArgs` / `FoldBatchItemArgs` | `mode` | `extend("enum"=["detailed","minimal"])` | fold 模式描述 |
| `IncrementalIndexArgs` | `files` | `length(min = 1)` | 空变更无意义 |

`exclude_content_types`、`include_categories` / `exclude_categories` 的合法字面量集合由共享服务枚举决定（`parse_exclude_content_type`、`FileCategory::from_name`）。`query_type`、`query_intent`、`direction`、`term_operator`、`mode` 等标量字段用 `extend("enum")` 注解。数组内元素的枚举经实测采用 `extend("items" = { "type":"string", "enum":[...] })`（`inner(extend(...))` 不被 schemars 支持，见 0.2 / 5.4），已补齐，覆盖上述三个数组字段在 `SearchArgs` / `AggregatedSearchArgs` / `FileSearchArgs` 的全部定义。

### 5.3 实测验证（探针结果）

隔离探针（`schemars 1.2.3`）确认：

- `range(min=1,max=100)` + `Option<usize>` → 生成 `"type":["integer","null"],"minimum":1,"maximum":100`（穿透 Option）。
- `length(min=1,max=16)` + `Vec` → `"minItems":1,"maxItems":16`。
- `extend("enum"=[...])` + `Option<String>` → 生成 `"enum":[...]`。
- `exclusive_min` → 编译报错 `unknown item in schemars range attribute`。

### 5.4 接受的 schema 局限

- 开区间：`weight` 运行时是 `(0,1]`（`aggregation.rs:89`），schema 只能表达闭区间 `[0,1]`；`min_score < 1.0`（`aggregation.rs:80`）同理。运行时校验保留，错误对象兜底。
- 跨字段约束：`deleted` 向量长度须等于 `files`（`is_valid_deleted_shape`，`support.rs:35-37`）无法用 schema 表达，保留运行时校验。
- 数组元素枚举：`schemars` 1.2.3 的 `inner` 只接受 validation 类属性，`extend` 为顶层属性，`inner(extend(...))` 编译报错；改用 `extend("items" = {...})` 覆盖元素 schema 后已补齐（0.2 / 5.2）。局限仍在：注解只作用于生成的 schema，不改变 serde 反序列化，运行时 `parse_*` 校验保留兜底。
- 这些局限由第 4 节的结构化错误对象覆盖：schema 用于"调用前预防"，错误对象用于"调用后纠偏"。

## 6. CLI 失败非零退出码（已实现，落地差异见 0.3）

### 6.1 问题事实

- `main.rs:23` 返回 `anyhow::Result<()>`，但绝大多数命令错误路径在 `print_error` 后返回 `Ok(())`（`commands/` 下 126 处 `print_error`），导致进程退出码仍为 0。
- `--format json` 只影响成功输出（`output.rs:47-51`），错误恒为 stderr 纯文本，机器读者无法用它判断成败。

### 6.2 改造方案（要点）

1. 引入 CLI 层统一错误出口：`print_error_and_fail(code, message) -> anyhow::Error`，替代 `print_error(...) ; Ok(())` 模式，使失败冒泡到 `main` 返回非零退出码。
2. 保持 stdout/stderr 契约：成功输出走 stdout；错误文案走 stderr。JSON 模式下，错误额外在 stderr 输出结构化 `{"error":{...}}`（不污染 stdout，避免破坏只解析 stdout 的调用方）。
3. 退出码分层（建议值，待 O6 拍板）：参数错误 2、未找到 3、服务不可用 4、其他 1。
4. 错误码复用 core 表，CLI 直连 `cce_types::error_codes`（O2 删除 re-export 后；落地时修正，见 0.3）。

### 6.3 错误码表格（三端统一，供 CLI 阶段使用）

| code | 常量位置 | 语义 | HTTP 状态 | MCP 用途 | CLI 建议退出码 |
|---|---|---|---|---|---|
| `INVALID_REQUEST` | core | 请求整体不被接受 | 400 | 工具禁用/deleted 形状 | 2 |
| `INVALID_INPUT` | core | 参数值非法 | 400 | query type/limit/direction 等 | 2 |
| `AMBIGUOUS_SYMBOL` | core | 符号匹配多个实体 | 400 | relation AmbiguousSeed | 2（提示候选） |
| `ENTITY_NOT_FOUND` | core | 实体/项目不存在 | 404 | relation UnknownSeed | 3 |
| `CONFLICT` | core | 资源冲突 | 409 | 暂未使用 | 1 |
| `SERVICE_UNAVAILABLE` | core | 后端不可用 | 503 | relation Unavailable | 4 |
| `NOT_IMPLEMENTED` | core | 未实现 | 501 | 暂未使用 | 1 |
| `INDEX_NOT_INITIALIZED` | core | 索引未初始化 | 500（默认） | 暂未使用 | 4 |
| `PARSE_ERROR` | core | 解析失败 | 500（默认） | 暂未使用 | 1 |
| `STORAGE_ERROR` | core | 存储失败 | 500（默认） | 暂未使用 | 1 |
| `QUERY_ERROR` | core | 查询失败 | 500（默认） | search 执行失败候选 | 1 |
| `INTERNAL_ERROR` | core | 内部错误 | 500 | Project/Execution 兜底 | 1 |

HTTP 状态映射依据 `cce-server/src/api/response.rs:71-82`；未在 match 中的码落到 500。

## 7. 实施顺序

1. 第 3 节：错误码迁入 core（新增模块 → cce-api re-export → cce-mcp 改 import）。
2. 第 4.4 节：relation 映射下沉到 cce-server-shared，HTTP 接入。
3. 第 4.2/4.3/4.5 节：MCP 结构化错误对象 + 26 处替换。
4. 第 5 节：schema 注解。
5. 编译与测试验证（第 8 节）。
6. 同步本文档状态与第 6 节（下一阶段）。

## 8. 验证计划

- `cargo check -p cce-types -p cce-api -p cce-client -p cce-server-shared -p cce-server -p cce-mcp`
- `cargo clippy --all-targets --all-features`（`AGENTS.md:64`）
- `cargo fmt`（`AGENTS.md:67`）
- 相关单测：错误码稳定性测试、schema 约束断言、relation 映射一致性断言、MCP 工具数量测试（`handler.rs:267`）。

## 9. 风险与缓解

| 风险 | 缓解 |
|---|---|
| 错误码值变更破坏 wire 契约 | 逐字迁移，不改值；保留稳定性测试 |
| re-export 造成 cce-api → cce-types 依赖方向争议 | 依据 lib.rs 门面职责说明（第 2 节），并列 O1 |
| MCP 错误形态变化影响 LLM 客户端 | message 文案保持不变，仅增 `code`；结构化可被现有文本读者忽略 |
| schema 注解值与运行时不一致 | 以运行时代码为准（F11 取 16）；开区间局限明示（5.4） |

## 10. 待拍板开放点

- **O1**：错误码真值源放 `cce-types` 顶层模块，还是其 `types::error` 子模块？本方案选顶层（全域 wire 语义）。
- **O2**：是否彻底删除 cce-api 的 re-export、把门面引用全改为 `cce_types::error_codes`？**已拍板：彻底删除**（见 0.2，33 处引用全部切换，cce-api 移除 cce-types 依赖，编译通过）。
- **O3**：`ensure_enabled`（工具禁用）改用结构化错误后，是否仍需保留一条协议级 `invalid_params` 通道供客户端区分"权限拒绝"？**已定：单一通道**（首轮已落地）。
- **O4**：`index_project` 的 partial 成功（payload + error）是否套用统一错误信封？**已拍板：不套信封，partial 改用成功通道**（见 0.2），与 HTTP 的 success+`success:false` 对齐，保留诊断 payload。
- **O5**：数组元素级枚举（`exclude_content_types`、`categories`）如何补齐？**已拍板：用 `extend("items" = {enum})`**（见 0.2），因 `inner(extend(...))` 语法不支持。
- **O6**：CLI 退出码分层取值（2/3/4/1）是否与现有脚本/调用方兼容？下一阶段拍板。

## 11. 参考

- 分析文档：`docs/analysis/cli-mcp-parameter-error-response-design-review.md`
- MCP 工具清单：`docs/app/server/mcp-api.md`
- 错误码定义：`crates/app/cce-api/src/models/response.rs:68-82`
- HTTP 映射：`crates/app/cce-server/src/api/response.rs`
- MCP 错误构造：`crates/app/cce-mcp/src/handler/support.rs`
- 子查询上限：`crates/app/cce-orchestrator/src/query/aggregation.rs:23`
- 项目规范：`AGENTS.md`
