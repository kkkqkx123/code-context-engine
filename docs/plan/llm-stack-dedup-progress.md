# LLM 接入栈去重迁移进度与剩余任务

本文档承接 `llm-stack-dedup-plan.md`(下称"方案")的第 4 节实施顺序,记录已完成的修改与剩余工作。问题编号(A1/B5/…)仅指方案中的清单条目;代码注释不引用编号。

## 一、已完成(编译/测试已验证)

### 阶段1:llm-suite submodule(全部完成,`cargo check` + `cargo test -p llm-embedding -p llm-rerank -p llm-gateway -p llm-common -p llm-token --lib` 通过)

- **估算归一(A1 部分)**:`llm-token/src/estimation.rs` 的 `SYMBOL_FACTOR` 0.5 → 1.0,注释说明保守方向;两处受影响的测试期望值已同步更新。CCE 侧 `cce-utils::token_estimation` 改薄转发的动作**未做**(见剩余任务)。
- **HTTP helper(A6)**:`llm-proxy` 新增 `build_http_client(timeout_secs, proxy, no_proxy)` 与 `ProxyError::ClientBuild` 变体;五处重复构造(embedding、cohere、generative、gateway,chat-basic 未删)已改为调用它。gateway 的错误分类简化为统一映射 `LlmError::ProxyError`(行为差异:无 proxy 时构建失败也归为 ProxyError,语义可接受)。
- **错误合并(A7)**:新增 `llm-common/src/provider_error.rs`(`ProviderError` 含 Config/InvalidRequest/Provider{status,message,retry_after_ms}/Decode/Transport/Timeout/CircuitOpen 变体 + `is_retryable`/`retry_after_ms`/`is_network_failure` 方法 + `ResilienceError` trait);`llm-embedding::EmbeddingError` 与 `llm-rerank::RerankError` 改为 re-export 别名。`llm-common` 补了 thiserror/reqwest 依赖。
- **retry 执行器扩展(A3 部分)**:`llm-common/src/retry.rs` 新增 `delay_for_attempt_with_floor` 与 `execute_with_retry_floor`(支持按错误取 Retry-After 下限),原 `execute_with_retry_observed` 保持兼容。
- **chat URL 修正(B6 suite 侧)**:`llm-codec/codecs/openai_chat.rs` 的 `build_request` 在 profile URL 已含 `/chat/completions` 时不再重复拼接,自定义路径与 Azure api-version 查询串生效。
- **注册精准失效(C7)**:gateway 的 `register_provider_definition`/`remove_provider_definition` 不再 `clients.clear()`,改为 `evict_clients_for_provider`(按 profile→provider_id 关联精准删除)。
- **共享弹性组件(A3/A4/A5 基础设施)**:新增 `llm-client/src/resilience.rs`(`Resilience` 栈:limiter + breaker + retry,`execute()` 统一驱动,含测试);gateway 新增 `shared_breaker(key, config)` / `shared_limiter(key, rps, burst)` 访问器,embedding/rerank 可注册进与 chat 同一注册表。
- **chat `ProviderError` 补 `retry_after_ms` 字段(C2 数据来源)**:构造点(client.rs、error.rs 测试)已同步。

### 阶段2:cce-config / cce-llm(全部完成,`cargo check -p cce-llm -p cce-config` 通过)

- **死字段删除(B4/B5)**:`ProviderConfig` 删除 `retry_jitter`/`rate_limit_max_retries`/`rate_limit_max_delay_ms`;`EmbedderConfig` 删除 `use_base64`;`ResolvedLlmConnection`/`ResolvedEmbeddingConfig` 同步删除传递;构造点与测试(llm_models.rs ×4、global/tests.rs ×2、embedder.rs)已全部更新。
- **熔断配置直通(C1)**:`CircuitBreakerConfig` 字段改为 min_samples/failure_ratio/open_duration_ms/half_open_probes(与 suite 语义一致),默认值 10/0.5/60000/1。
- **no_proxy 入口(B7 部分)**:`ProviderConfig` 新增 `no_proxy: Vec<String>`,经 `ResolvedLlmConnection`/`ResolvedEmbeddingConfig` 透传(no_proxy 只在 provider 级入口;模型级未加)。
- **embedding 端口(D1 的 cce-llm 半边)**:`cce-llm/src/embedding.rs` 新增 `EmbeddingProvider` 端口 trait(embed/embed_one/dimension/model_name/is_healthy,RPITIT 无 dyn),`EmbeddingResult` 保留。
- **LlmError 新变体(C4)**:`cce_llm::LlmError` 新增 `ContextLengthExceeded(String)` + 构造函数 + error_code 分支 + is_permanent 分类。

## 二、进行中 / 半完成(代码已改,尚未收尾)

- **cce-llm-client/src/services/embedding/handler.rs**:`RetryPolicy`/`backoff_ms`/`retry_delay` 第三套重试已删除,`SuiteEmbeddingTransport` 已改为持有注入的 `Resilience`;`from_resolved` 已翻译 preprocessor(Nomic/Stella → Prefix/Template,`{text}`→`{{text}}`)并透传 no_proxy。**但引用的 `crate::suite::{circuit_breaker_config 参数已变、rate_limit_config、embedding_resilience}` 尚未在 suite.rs 中落地/调整,当前 cce-llm-client 编译不通过**。
- **cce-llm-client/src/suite.rs**:`circuit_breaker_config` 转换函数仍在(其入参 `cce_config::modules::CircuitBreakerConfig` 字段已变,函数体用的 `failure_threshold`/`recovery_timeout_secs` 已不存在 → 编译错误);`rate_limit_config` 仍是 per-minute→rps 换算;测试 `connection()` 构造体仍含已删除的 `retry_jitter` 等字段。
- suite 侧 `crates/llm-client/src/circuit.rs` 此前有未提交的宿主改动(M 状态),与本迁移无冲突但注意提交时区分。

## 三、剩余任务(按执行顺序)

### 1. 收尾 suite.rs(阶段3a,优先,恢复编译)

- 修 `circuit_breaker_config`:入参字段已直通化,函数体改为逐字段拷贝(min_samples/failure_ratio/open_duration_ms/half_open_probes;`enabled=false` 返回 None),并同步测试 `disabled_breaker_yields_no_config`。
- 新增 `embedding_resilience(resolved) -> Resilience` / `rerank_resilience(connection) -> Resilience`:按 `base_url::provider_id` key 从 `global_gateway().shared_breaker/shared_limiter` 取共享组件,`rate_limit` per-minute→rps 的换算移入此 helper(删 `rate_limit_config` 公开函数与 10000 魔数;`burst = rps.ceil()`);retry 预算用 `llm_common::RetryPolicy { max_retries, base_delay_ms, exponential_backoff: true }`。
- 统一错误映射(A7/C2/C3/C4 剩余):三个 `map_*_error` 共享一个内部 status→变体函数;`is_quota_message` 移入单点;chat 429 用 `ProviderError.retry_after_ms` 替代硬编码 5000;`SuiteError::ContextLengthExceeded` 映射到 `LlmError::ContextLengthExceeded`(不再塌缩为 HttpStatus{400},测试 `chat_error_mapping_covers_contract_variants` 同步)。
- `chat_profile` 的 `base_url` 改为 `full_endpoint_url(base_url, endpoint_path)`(B6 CCE 侧);`chat_profile`/`provider_definition` 透传 `no_proxy`(B7)。
- C5:`SuiteChatClient::chat` 的 completion_tokens 直取 `usage.completion_tokens`。
- C8:`init_global_token_metrics` 扩展为 `init_gateway(registry)`(一次性构造 gateway + sink,`global_gateway()` 未初始化时返回错误或 panic 明确提示;调用方传播 `LlmError::config`)。同步更新 `lib.rs` 导出与 engine.rs 调用点。
- 测试 `connection()` 构造体去掉已删字段、补 `no_proxy`;`embedding/handler.rs` 顶部 import 与 `circuit_breaker_config` 调用对齐。
- 删除 `cce-llm-client/Cargo.toml` 的 `llm-config` 直接依赖(B3)。

### 2. embedding 收尾(阶段3b)

- 删除 `services/embedding/preprocessor.rs` 整文件及 `services/embedding.rs` 中的 `pub(crate) mod preprocessor`。
- `services/embedding/provider.rs`:删除 `preprocess_texts` 与 `preprocessor` 字段(预处理已下沉 suite);删除 `consecutive_failures`/`HEALTH_FAILURE_THRESHOLD` 迷你熔断,`is_healthy()` 改为查询 transport 注入的 breaker(`transport.resilience().is_breaker_open()` 取反;for_testing 无 breaker 时恒 true);token 上报 `token_count` 改用 `estimate_tokens` 聚合(C6),与 provider usage 分开;`classify_error` 保留但错误分类入参已是统一映射结果(D2 部分)。
- 同步该文件的测试(`test_global_config` 构造、metadata 测试)。

### 3. rerank 收尾(阶段3c)

- `services/rerank/handler.rs`:删除 `limit_candidates`(A11,单点裁剪交给 suite provider 内部的 `limit_candidates`);保留超时与指标;注入 resilience 可选(handler 级超时已存在,如注入则经 `DelegatingRerankProvider` 传递)。
- `services/rerank/provider.rs`:删除 `GenerativeRerankProvider`/`CohereRerankProvider` 两个重复包装(A10),保留 `DelegatingRerankProvider<P>` 单个委托 + `From` 转换对替代自由函数(A9);工厂返回类型不变——在 `factory.rs` 用类型别名 `pub type GenerativeRerankProvider = DelegatingRerankProvider<llm_rerank::GenerativeRerankProvider>`(cohere 同理)保持 `services/rerank.rs` 的 `GenerativeRerankRequestHandler` 等类型与 `ProductionRerankHandler` 不变;`endpoint()`/`config()` 访问差异用 trait 或直接在工厂处丢弃(确认无外部消费后可删)。
- `rerank_endpoint_config`/`generative_chat_endpoint` 补 no_proxy 透传。

### 4. 消费方适配(阶段4)

- `cce-orchestrator`:`cached_embedder.rs` 泛型约束从 `llm_embedding::EmbeddingProvider` 改绑 `cce_llm::EmbeddingProvider`(需适配 embed 返回错误类型差异——suite provider 返回 `EmbeddingError`,cce-llm 端口返回 `LlmError`;在 cce-llm-client 的 `OpenAICompatibleProvider` 上实现 cce-llm 端口作桥接);删除 `cce-orchestrator/Cargo.toml` 对 `llm-embedding` 的直接依赖;测试 mock 类型在 CCE 侧实现端口(mock 数据经 cce-llm-client 转发或本地构造)。
- `cce-server-shared/engine.rs`:`init_global_token_metrics` → `init_gateway(registry)` 调用点同步;CLI/MCP/e2e 入口补启动调用(方案 3.8)。
- `crate::suite::embedding_resilience` 中 retry 预算来源:`ResolvedEmbeddingConfig.max_retries`/`retry_delay_ms`。
- e2e 测试(`rerank_workflow`、embedding 集成)走 stub 验证装配链。

### 5. CCE 侧 token 估算薄转发(A1 剩余)

- `cce-utils/src/token_estimation.rs` 改为转发 `llm_token::estimation::estimate_tokens`(workspace 已有 llm-token 依赖路径可加),`estimate_tokens` 等公开符号签名不变;保留 `truncate_to_token_budget`/`TruncationResult` 等 CCE 特有函数。45 个消费点零改动。

### 6. 文档与验收

- 更新 `docs/core/config/global-config-reference.md`(CircuitBreakerConfig 新字段、no_proxy、删除的死字段)、`docs/infra/llm/retry-and-rate-limit.md`(弹性收敛说明)、llm-suite `README.md`(chat-basic 状态)、`docs/archive/dynamic.md`(确认无新增 dyn;chat-basic 未删除故其条目暂留)。
- 删除 `llm-chat-basic`(B1)**挂起**:README 提及 wf-agent 宿主,需先确认其已走 gateway 路径,不阻塞其余条目。
- 验收命令:`cargo clippy --all-targets --all-features`、`cargo fmt`、`cargo test -p cce-llm-client -p cce-orchestrator --lib`、suite 侧 `cargo test -p llm-embedding -p llm-rerank -p llm-gateway --lib`。
- grep 验收:`RetryPolicy/backoff` 在 cce-llm-client 消失、`consecutive_failures` 消失、`SYMBOL_FACTOR` 唯一(suite 0.5 值已改 1.0,CCE 薄转发后即唯一)、`use_base64/retry_jitter/rate_limit_max_*` 全链删除、`preprocessor.rs` 仅 suite 一份、`llm_embedding::` 不再出现在 orchestrator。

## 四、风险备忘

- 熔断配置语义已重定义(failure_threshold 含义变化),现有 TOML 需重写(方案 6 节,按 no-backward-compat 处理)。
- chat `ProviderError` 增加 `retry_after_ms` 字段是 suite submodule 的破坏性变更,wf-agent 宿主同步时注意构造点。
- llm-chat-basic 删除挂起,待 wf-agent 确认。
- submodule 提交顺序:先提交 llm-suite,再在主仓更新 submodule 指针 + CCE 侧改动,避免 CI 拉到不一致状态。
