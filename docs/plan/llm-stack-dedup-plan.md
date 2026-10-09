# LLM 接入栈去重与收敛方案（llm-suite 与内部代理层）

针对 llm-suite（submodule）与 CCE 内部 LLM 桥接层（`cce-llm` 端口 + `cce-llm-client` 适配）的迁移残留进行治理：消除重复实现、删除死配置、统一三条调用路径（chat / embedding / rerank）的弹性与观测语义。

## 1. 背景

LLM 调用从 CCE 自建 HTTP 层迁移到 llm-suite 的工作只完成了一半：

- **chat** 走 `cce-llm-client/src/suite.rs` 的进程级 `LlmGateway`（熔断、限流、重试、指标齐全）。
- **embedding** 直连 `llm-embedding` provider，绕过 gateway；重试、健康计数、预处理器、批处理在 CCE 侧重写了一套（`services/embedding/`）。
- **rerank** 直连 `llm-rerank` provider，绕过 gateway；无重试，候选裁剪与 suite 内部逻辑重复（`services/rerank/`）。

结果是同一能力在 CCE 与 llm-suite 两侧各有一份实现，且部分配置迁移后已静默失效。本方案先给出完整问题清单（逐项带证据位置），再给出对应修改、实施顺序与验收标准。

> 实施注意：问题编号仅用于本文档跟踪，不得以编号形式写入代码注释；代码注释只描述意图，需要引用时引用文件/符号。

## 2. 完整问题清单

### A 组：CCE 与 llm-suite 两侧重复实现

| 编号 | 问题 | 证据 |
|---|---|---|
| A1 | token 估算两套且已漂移：`cce-utils::token_estimation`（`SYMBOL_FACTOR = 1.0`）与 `llm-suite/llm-token/src/estimation.rs`（`SYMBOL_FACTOR = 0.5`）逐行近同，同一文本在 CCE 批处理与 suite `count_tokens` 回退路径下估算结论不一致 | `crates/core/cce-utils/src/token_estimation.rs:16`；`crates/infra/llm-suite/crates/llm-token/src/estimation.rs:12`；消费点 `cce-llm-client/src/services/embedding/handler.rs:243` 与 `llm-suite/crates/llm-client/src/client.rs:283` |
| A2 | 预处理器两套：CCE `services/embedding/preprocessor.rs`（Prefix/Template/Nomic/Stella，占位符 `{text}`）与 `llm-embedding/src/preprocessor.rs`（None/Prefix/Template，占位符 `{{text}}`）。CCE 路径从不给 suite 配置预处理器（双重前缀风险被掩盖）；Nomic/Stella 在 CCE 侧只是把枚举映射成固定的 Prefix/Template 值 | `cce-llm-client/src/services/embedding/preprocessor.rs` 全文；`llm-suite/crates/llm-embedding/src/preprocessor.rs:13-61`；`handler.rs:64-84`（未设置 preprocessor）；`provider.rs:148-186` |
| A3 | 退避重试实现四处：`llm-common/src/retry.rs`（gateway 用）、`llm-chat-basic/src/retry.rs`、`cce-llm-client handler.rs:23-47,103-145`（CCE 第三套 `RetryPolicy` + backoff）、`cce-utils/src/retry.rs`（存储层用）。chat 重试在 suite 内、embedding 重试在 CCE 内、rerank 没有重试 | 各文件对应行 |
| A4 | 熔断器三套：`cce-circuit-breaker`（连续计数，存储服务）、`llm-client/src/circuit.rs`（失败率窗口，gateway）、`cce-llm-client provider.rs:23,33,128-136,195-196` 的 `consecutive_failures` 迷你熔断（阈值硬编码 3） | 各文件对应行 |
| A5 | 限流器两套：`llm-common/src/ratelimit.rs`（gateway 按 base_url 共享）与 `llm-chat-basic/src/ratelimit.rs`（自称从 CCE 移植，CCE 不使用） | `llm-suite/crates/llm-chat-basic/src/ratelimit.rs:1-4` |
| A6 | reqwest client 构造重复五处（builder + timeout + proxy + build 同一段逻辑）：`llm-embedding/openai_compatible.rs:71-80`、`llm-rerank/cohere.rs:26-37`、`llm-rerank/generative.rs:83-94`、`llm-chat-basic/client.rs:67-77`、`llm-gateway/gateway.rs:431-461`。`llm-proxy` 只统一了 proxy 构造，client 构造没有共享 helper | 各文件对应行 |
| A7 | `EmbeddingError` 与 `RerankError` 结构同构（Provider{status,message,retry_after_ms}/Timeout/Transport/Decode/InvalidRequest），`From<reqwest::Error>` 逐字相同 | `llm-embedding/src/error.rs`；`llm-rerank/src/error.rs` |
| A8 | 端点连接配置三份近同：`llm-embedding::EmbeddingConfig`、`llm-rerank::RerankConfig`、`llm-rerank::GenerativeChatEndpoint`（base_url/api_key/model/timeout/proxy/no_proxy/headers/query_params） | `llm-embedding/src/config.rs`；`llm-rerank/src/config.rs:10-38`；`llm-rerank/src/generative.rs:22-53` |
| A9 | 镜像类型 + 纯字段搬运转换：`RerankCandidate/RerankedCandidate/RerankResult/RerankRuntimeConfig/RerankFusionStrategy` 在 `cce_types`/`cce_config`/`cce_llm` 与 `llm-rerank` 各一份，`convert_*` 五个函数逐字段拷贝；`EmbeddingResult` 同构两份并逐 batch 拷贝 | `cce-llm-client/src/services/rerank/provider.rs:12-72`；`llm-rerank/src/provider.rs:1-47`（注释自认"ported from cce-types"）；`embedding/handler.rs:106-114`；`cce-llm/src/embedding.rs` |
| A10 | rerank 适配器三重复制：`GenerativeRerankProvider`、`CohereRerankProvider`、`DelegatingRerankProvider<P>` 三个实现的 `rerank/provider_name/is_available` 方法体完全一致 | `cce-llm-client/src/services/rerank/provider.rs:75-178` |
| A11 | rerank 候选裁剪重复：CCE `RerankRequestHandler::limit_candidates`（排序截断）与 suite `provider::limit_candidates` 做同一件事，请求被裁剪两次 | `cce-llm-client/src/services/rerank/handler.rs:122-144`；`llm-rerank/src/provider.rs:148-163`；`cohere.rs:125`、`generative.rs:171` |

### B 组：死代码与未生效配置

| 编号 | 问题 | 证据 |
|---|---|---|
| B1 | `llm-chat-basic`（1063 行）在整个 workspace 内零依赖方（含 suite 内部），是从 CCE 回搬的孤儿实现；其 `ChatResult`/`LlmChat` 与 `cce-llm` 的 `ChatResult`/`LlmClient` 同构 | `grep Cargo.toml` 无引用；`llm-chat-basic/src/lib.rs:1-6` |
| B2 | CCE 编译但运行时不可达的 suite 部分：`llm-tool-call`（1943 行，仅流路径使用）、`llm-codec` 的 anthropic/gemini_native/openai_response 三个 codec（约 2400 行，CCE 两处硬编码 `LlmFormat::OpenaiChat`）、`llm-client` 的 SSE 流/dead-loop/token_stream、`llm-config` 的 `ModelCatalog` 模型发现 | `suite.rs:215,243`（format 固定）；CCE 无 `generate_stream`/`count_tokens`/`list_models` 调用 |
| B3 | `cce-llm-client/Cargo.toml` 声明了 `llm-config` 直接依赖但代码零引用 | `cce-llm-client/Cargo.toml:15`；`grep llm_config src/` 无结果 |
| B4 | 死配置 `embedder.use_base64`：解析链完整（配置→Resolved）但迁移后无任何消费者，suite 编码格式固定 `"float"` | `cce-config/src/modules/embedder.rs:69`；`resolved.rs:230`；`llm-embedding/openai_compatible.rs:105` |
| B5 | 死配置 `retry_jitter` / `rate_limit_max_retries` / `rate_limit_max_delay_ms`：配置与 Resolved 层齐全，但 `suite.rs` 的转换函数从不消费，配置写了不生效 | `cce-config/src/modules/llm_models.rs:100-111`；`global/resolved.rs:46-50,181-183`；`suite.rs` 无引用 |
| B6 | chat 路径静默忽略 `endpoint_path`：embedding/rerank 经 `full_endpoint_url` 拼接，chat 直接把原始 base_url 交给 profile，codec 硬拼 `{base_url}/chat/completions`。provider 自定义 chat endpoint 路径（含 Azure `chat/completions?api-version=...` 示例）不再生效 | `llm_models.rs:593`（示例配置）；`suite.rs:240-247`（chat_profile 无 endpoint_path）；`llm-codec/src/codecs/openai_chat.rs:88-91` |
| B7 | `provider_definition()`/`chat_profile()` 中 `auth_type`、`api_version`、`no_proxy`、`model_discovery` 等恒为 None，CCE 配置层无对应入口（suite 侧字段成为摆设） | `suite.rs:207-223,240-266` |

### C 组：语义失真与一致性风险

| 编号 | 问题 | 证据 |
|---|---|---|
| C1 | `circuit_breaker_config` 把"连续 N 次失败断开、T 秒恢复"隐式改写成"min_samples=N + 固定 50% 失败率 + 固定单探测"，配置语义与文档不符且不可逆 | `suite.rs:169-181` |
| C2 | 429 处理不一致：chat 的 `ProviderError` 不带 `retry_after`，映射时硬编码 5000ms；embedding/rerank 映射尊重 provider 的 `retry_after_ms` | `suite.rs:382-391` 对比 `420-429,447-456` |
| C3 | `is_quota_message` 靠响应体子串猜测配额耗尽/限流，分类逻辑脆弱且分散在三个 mapper | `suite.rs:330-342` |
| C4 | `ContextLengthExceeded` 塌缩为 `HttpStatus{400}`，死信/截断恢复流程失去显式类型信号 | `suite.rs:371-374`；关联 `dead-letter-truncate-retry-design.md` 的 400+20015 判定 |
| C5 | chat 完成 token 用 `total - prompt` 推导，suite usage 本有 `completion_tokens` 字段 | `suite.rs:563-567`；`llm-types/src/llm/usage.rs:6` |
| C6 | embedding 指标把 `text.len()` 字节数当 token 数上报，与全局 `estimate_tokens` 口径不一致 | `provider.rs:123,131` |
| C7 | `ensure_chat_profile → ensure_provider_registered → register_provider_definition → clients.clear()` 全局清空客户端缓存；每次热更新重建任一模型都会打掉全部已缓存 client | `suite.rs:227-231,270-282`；`gateway.rs:106-116` |
| C8 | `TOKEN_SINK` 与 `global_gateway` 存在隐式启动顺序耦合：gateway 先于 sink 初始化则指标永久缺失，仅靠 engine.rs 的调用顺序保证 | `suite.rs:19-28,119-127`；`cce-server-shared/src/engine.rs:160-163` |
| C9 | 三条路径弹性保护不对等：chat 有熔断+限流+重试+统一指标；embedding 只有 CCE 自研重试+迷你健康计数；rerank 只有 handler 级超时 | `suite.rs:549`（gateway）、`handler.rs:103-135`（CCE 重试）、`rerank/handler.rs:61-108`（仅超时） |

### D 组：分层与观测

| 编号 | 问题 | 证据 |
|---|---|---|
| D1 | 分层泄漏：`cce-orchestrator` 直接依赖 `llm-embedding`，并把 suite 的 `EmbeddingProvider` trait 用作泛型端口；而 `cce-llm` 只有 chat/rerank 端口，没有 embedding 端口 | `cce-orchestrator/Cargo.toml:26`；`cached_embedder.rs:33,59,97-100`；`cce-llm/src/lib.rs`（无 embedding trait） |
| D2 | 三条 LLM 观测管道命名/维度不统一：`llm_gateway_*`（sink）、`EmbeddingMetrics`、`RerankMetrics` | `suite.rs:61-116`；`cce-metrics`；`engine.rs:204-213` |

## 3. 修改方案

### 3.1 端口归位（D1）

- `cce-llm` 新增 embedding 端口 trait（embed/embed_one/dimension/model_name/is_healthy），风格与 `RerankProvider` 一致（RPITIT，无 dyn、无 async-trait）。
- `cce-orchestrator` 的 `CachedEmbedder` 泛型约束改绑 `cce-llm` 端口；测试 mock 类型在 CCE 侧实现该端口（mock 数据经 `cce-llm-client` 转发提供），删除 `cce-orchestrator/Cargo.toml` 对 `llm-embedding` 的直接依赖（B2 相关编译面随之收窄）。
- `cce-llm-client/Cargo.toml` 删除未引用的 `llm-config` 直接依赖（B3）。

### 3.2 弹性收敛到单一来源（A3/A4/A5、C9）

- suite 侧改造：`llm-embedding` 与 `llm-rerank` 的 provider 支持注入可复用的 resilience 组件（直接消费 `llm-common` 的 ratelimit、`llm-client` 的 circuit、`llm-common` 的 retry 执行器）；`llm-common::retry` 的延迟计算扩展为可按错误结果取值（支持 rate-limit 的 retry-after 下限），供三条路径共用。
- CCE 侧删除：`SuiteEmbeddingTransport` 的 `RetryPolicy/backoff_ms/retry_delay` 重试循环（A3 第三套）、`OpenAICompatibleProvider` 的 `consecutive_failures/HEALTH_FAILURE_THRESHOLD` 迷你熔断（A4 第三套）——`is_healthy` 改为查询注入的 breaker 状态。
- rerank provider 补上与其他两条一致的注入；`RerankRequestHandler` 保留超时与指标，删除重复的 `limit_candidates`（A11，改由 suite 单点裁剪）。
- gateway 与 embedding/rerank 的 breaker/limiter 以 base_url 为键共享同一注册表（进程级 gateway 已具备；embedding/rerank 注册进同一注册表，恢复"同一 provider 共享保护"的原语义）。

### 3.3 熔断/限流配置直通（B7 部分、C1）

- `cce-config` 的 `CircuitBreakerConfig` 字段改为与 suite 语义一致（min_samples、failure_ratio、open_duration_ms、half_open_probes），删除 `circuit_breaker_config` 转换函数；`rate_limit` 配置改为 requests_per_second + burst 直传，删除 `rate_limit_config` 的换算与 10000 上限魔数。
- 删除无消费者的 `retry_jitter / rate_limit_max_retries / rate_limit_max_delay_ms`（B5）与 `use_base64`（B4）字段及其默认函数、Resolved 传递和文档条目；不做兼容读取。
- `no_proxy`：在 `ProviderConfig`/model 级增加 `no_proxy` 列表字段并透传（打通 B7 中已有 suite 能力却无入口的一段）；`auth_type`/`api_version` 保持 None 不造入口（suite 类型留给其他宿主，不算冗余）。

### 3.4 chat endpoint_path 打通（B6）

- `chat_profile` 的 `base_url` 改为 `full_endpoint_url(base_url, endpoint_path)` 的产物，codec 不再重复拼 `/chat/completions`：suite 侧 `LlmCodec::build_request` 以 profile 提供的完整 URL 为准（缺省才用内置路径）。修复后自定义路径与 Azure api-version 查询串恢复生效。

### 3.5 估算与预处理器归一（A1/A2）

- token 估算唯一实现收敛到 `llm-token::estimation`：`cce-utils::token_estimation` 改为薄转发（`estimate_tokens` 等公开符号不动，45 个消费点零改动），保留 `truncate_to_token_budget` 等 CCE 特有工具函数；`SYMBOL_FACTOR` 统一为 1.0（死信分析后收紧低估的方向），suite 内 0.5 的旧值随之消失。
- 预处理器唯一实现收敛到 `llm-embedding`：suite 的 `PreprocessorConfig` 增加模型常量映射能力（Nomic 四类前缀、Stella 两类模板即可由现有 Prefix/Template 表达，在 CCE 配置→suite 配置的映射表完成，suite 不引入模型名）。CCE 删除 `services/embedding/preprocessor.rs` 整文件与 `OpenAICompatibleProvider::preprocess_texts`；`SuiteEmbeddingTransport::from_resolved` 把 `cce-config::PreprocessorConfig` 翻译为 suite 配置。占位符统一为 `{{text}}`，同步更新配置参考文档（不做 `{text}` 兼容）。

### 3.6 类型与转换收敛（A7/A8/A9/A10、C5）

- suite 侧：合并 `EmbeddingError` 与 `RerankError` 为一个能力层错误（如 `llm-common` 下的 provider 错误），chat 的 `ProviderError` 变体补充 `retry_after_ms` 字段（修复 C2 的数据来源）；`EmbeddingConfig`/`RerankConfig`/`GenerativeChatEndpoint` 抽出共同连接段（同一 endpoint struct），builder 方法与 client 构造 helper（见 3.7）配合。
- CCE 侧：`cce-llm`/`cce_types` 的领域镜像**保留**（防腐层，端口不依赖 submodule 类型），但转换改为每个边界类型一对 `From` 实现替代自由函数；`rerank/provider.rs` 删除 `GenerativeRerankProvider`/`CohereRerankProvider` 两个重复包装，合并为单个委托适配器 + 两个构造函数保留 `endpoint()/config()` 访问差异（工厂返回类型不变）。
- `SuiteChatClient` 直接取 `usage.completion_tokens`（C5）。
- 错误映射：三个 `map_*_error` 共享一个"status → 变体 + 429 分类"内部函数；`is_quota_message` 子串启发式移入该共享函数（保留启发式本身，标注局限，单一修改点）（C3）；`ContextLengthExceeded` 在 `cce_llm::LlmError` 增加独立变体（或 `HttpStatus` 之外携带分类码），死信/截断路径改为显式匹配（C4）。

### 3.7 suite HTTP 与孤儿清理（A6、B1）

- `llm-proxy`（或 `llm-common`）新增 `build_http_client(timeout, proxy, no_proxy)` 共享构造，五处 client 构造段改为调用它；gateway 处的 ProxyError 分类保留。
- 删除 `llm-chat-basic` crate：workspace members、README 表格、`docs/archive/dynamic.md` 中"pinned event stream"条目；README 提及 wf-agent 用其作简单客户端，删除前需确认该宿主已走 gateway 路径（未确认则此项挂起，不阻塞其余条目）。
- B2 的不可达能力（tool-call、非 OpenAI codec、流式、ModelCatalog）**不删除**：llm-suite 是多宿主共享工具库，这些是其对外契约的一部分；CCE 不启用即成本主要是编译时间，通过 3.1 收窄 orchestrator 依赖已消除最大一块。

### 3.8 装配健壮性（C7/C8）

- gateway：`register_provider_definition` 不再无条件 `clients.clear()`——按 provider id 关联的 profile 前缀精准失效对应缓存；`remove_provider_definition` 同理。
- 初始化顺序改为显式不变量：`init_global_token_metrics` 扩展为一次性构造并持有 gateway 与 sink 的唯一入口（`init_gateway(registry)`），`global_gateway()` 对未初始化返回错误（调用方传播 `LlmError::config`），不再依赖"谁先被调用"的默契；CLI/MCP/e2e 入口同步补启动调用。

### 3.9 观测统一（D2、C6）

- embedding/rerank 指标命名向 `llm_gateway_*` 家族对齐（`llm_*_requests_total{service,model,status}`、延迟直方图），`EmbeddingMetrics`/`RerankMetrics` 的采集点保留在 CCE 层（批/候选粒度不同），但错误分类改用 3.6 的统一映射结果而非本地 `classify_error` 表。
- embedding 的 token 上报改用 `estimate_tokens` 聚合（C6）与 provider usage 分开两个字段，不再混用字节数。

## 4. 实施顺序

依赖关系决定分四步，每步收敛后即编译验证一次（合并 clippy/fmt 运行，不单独编译）：

1. **llm-suite submodule**：3.5 估算归一、3.7 HTTP helper 与 chat-basic 删除（挂起项除外）、3.6 错误合并与 chat URL 修正、3.2 retry 执行器扩展、3.8 provider 注册精准失效。
2. **cce-config / cce-llm**：3.3 配置直通与死字段删除、3.1 embedding 端口、3.6 `LlmError` 新变体。
3. **cce-llm-client**：3.2 弹性注入与本地实现删除、3.5 预处理器删除、3.6 转换与适配器合并、3.4 endpoint_path 打通、3.8 初始化入口、3.9 指标口径。
4. **消费方**：`cce-orchestrator`（3.1 端口切换、依赖删除）、`cce-server-shared`（3.8 启动调用）、`cce-e2e-tests`/集成测试适配；同步更新 `docs/core/config/global-config-reference.md`、`docs/infra/llm/retry-and-rate-limit.md`、llm-suite `README.md`、`docs/archive/dynamic.md`。

## 5. 验收标准

- `cargo clippy --all-targets --all-features` 与 `cargo fmt` 通过。
- grep 检查：`llm-chat-basic` 零引用；`cce-orchestrator` 不再出现 `llm_embedding::` 路径；`RetryPolicy/backoff` 在 embedding 桥接层消失（仅 suite 一份）；`consecutive_failures` 在 `cce-llm-client` 消失；`SYMBOL_FACTOR` 唯一定义；`use_base64/retry_jitter/rate_limit_max_*` 全链删除；`preprocessor.rs` 仅 suite 一份。
- 行为测试：`cargo test -p cce-llm-client -p cce-orchestrator --lib`（重试预算、rate-limit 下限、候选裁剪单点、429 retry_after 透传、chat URL 含 endpoint_path、ContextLength 显式变体）；suite 侧 `cargo test -p llm-embedding -p llm-rerank -p llm-gateway --lib`（错误合并后映射、http helper、注册精准失效）；`cce-e2e-tests` 的 rerank_workflow 与 embedding 集成测试走 stub 验证装配链。

## 6. 风险与取舍

- **共享 submodule 的跨宿主影响**：llm-suite 同时被 wf-agent 使用；错误合并、chat-basic 删除、估算因子变更需在该宿主验证窗口内进行，否则按 3.7 挂起策略处理。
- **估算口径变化**：批处理预算文本（chat 无关，embedding 主路）从 0.5 系数统一到 1.0 会使代码密集文本估算变大、批次变小——方向上更安全（少触发 provider 400），但吞吐可能下降，需基准确认。
- **熔断配置语义重定义**：现有 TOML 的 `failure_threshold` 数值含义变化，无兼容迁移（按项目 no-backward-compat 规则处理，配置需重写）。
- **不做的事**：不引入"第二套更优实现"过渡；不改 cce-circuit-breaker 存储服务用法（语义不同、消费方不同，A4 中仅删除 LLM 桥接层内的第三套）。
