# 前后端契约统一与 OpenAPI 代码生成方案

## 1. 背景与问题

后端 cce-server 暴露 66 个 REST 端点，契约类型集中在 cce-api（13 文件 / 115 个类型）；前端手写 93 个 TS interface 镜像（13 文件），无任何机器可读契约。已证实的漂移（均为活缺陷，非理论风险）：

- cce-api 的 AggregatedSearchResponse、DiagnoseApiResponse、CompressResponse、KeywordSearchRequest/Response、MetricsHistoryResponse 等与线上 wire 形状脱钩，成为死模型；cce-cli 的 agg-search / tools compress / diagnose 命令按死模型反序列化，运行时必挂。
- tools compress/diagnose/keyword-search 三个端点的 wire 类型直接来自 cce-orchestrator，绕过契约层。
- 响应封装三轨并存：统一 ApiResult、per-handler untagged 枚举（SearchApiResponse、CallsApiResponse、GraphSuccess、RelationSuccess）、约 10 个 json! 手拼端点。
- 错误形状三轨并存：ErrorResponse、{error: string}、{success:false, message}。
- 前端 Diagnose 组件按无信封形状读响应（字段全 undefined）；QdrantProcessStatus 枚举标签形式前后端不匹配；create project 的 created_at 后端发 i64、契约与前端是 string；project list/get 的 record_to_config 丢失 extensions 等字段；metrics history 的 AggregatedMetric 缺 metric_type。
- 前端 api 层 13 文件中 10 个自 init 后从未随契约变更同步。

结论：引入 OpenAPI 文档 + 离线代码生成（效仿 wf-agent 方案），但前置条件是先把契约收敛为单一事实源，否则只会把混乱固化成文档。CCE 规模（66 端点）远小于 wf-agent（452 操作），可一次做全量类型化，不留 data: unknown 妥协；且无 WS/SSE，codegen 覆盖面即全部 API 面。

## 2. 总体架构

单一事实源 = cce-api 模型 + handler 注解；一条守卫链 = 快照测试；一条生成链 = openapi.json → schema.d.ts。

```
[cce-api 契约模型]（唯一 wire 真相，handler 只收发 cce-api 类型）
   │  utoipa derive（ToSchema / IntoParams）
   ▼
[cce-server openapi.rs]  #[utoipa::path] × 66 端点 + ApiDoc 聚合
   ├──► 运行时 GET /api-docs/openapi.json（仅 debug 构建，人工调试）
   └──► cargo test openapi_snapshot_matches
          ├─ 默认：与 frontend/openapi.json 逐字节比对，漂移即红
          └─ CCE_REFRESH_OPENAPI=1：刷新快照（提交入库）
                    │
                    ▼
[tools/openapi-codegen]（独立 npm 小包，TS5 隔离，不在 frontend 依赖树）
   npm run gen → schema.d.ts（本目录中间产物，不入库）→ cp 到
[frontend/src/lib/api/schema.d.ts]（提交入库）
                    │
                    ▼
前端 api/*.ts 引用 components['schemas'] 类型别名；client.ts 手写传输层保留
```

对应 wf-agent 的三项决策全部沿用：离线 golden-file 快照（部署环境不出网，不做 build.rs、不依赖起服务）；注解常驻编译不做 feature 门控；codegen 工具目录物理隔离（openapi-typescript 的 TS5 peer 与前端 TS6 冲突）。

## 3. Phase 0：契约收敛（先于 codegen）

### 3.1 信封统一
- 全部端点返回 ApiResult<T>（untagged：Error(ErrorResponse) | Success(T)），HTTP 状态由 status_for_code 集中映射；删除 SearchApiResponse、CallsApiResponse、GraphSuccess、RelationSuccess、SummaryApiResponse 别名等 per-handler 枚举，untagged 多形态合并为每端点一个具体 T。
- 错误形状统一为 ErrorResponse{success:false, error:{code,message,details}}；health/metrics/qdrant-admin 的 {error: string} 与 project 的 json! error_response 全部并入。error_codes 增补 CONFLICT（409）与 NOT_IMPLEMENTED（501）。
- index 端点的 206 Partial Content 取消：完成即 200，业务成败由 success 旗标承载；执行前置失败（非法 project_id、路径不存在、引擎错误）改返 ErrorResponse。
- tools 类端点保留"带内业务结果"形状（{success, result, error, relation_info?}，HTTP 恒 200），与 symbols/references/definition 既有模式对齐；compress 从 flatten 形态、keyword 从 data 字段统一改为 result 字段；batch compress 的 (path, x) 元组改为命名字段条目。

### 3.2 死模型修正（cce-api 重写）
- 删除 AggregatedSearchResponse、SearchResult、旧 CompressResponse、旧 DiagnoseApiResponse/DiagnoseIssue、旧 KeywordSearch* 死模型；agg-search 端点复用 SearchResponse。
- SearchRequest 删除 handler 从未消费的 file_extensions/entity_types/languages；ParseRequest 删除未消费的 language；IncrementalIndexRequest 删除未消费的 force_reindex。
- tools.rs 重写为镜像 orchestrator wire：CompressResult（entities/groups 以 JSON 值承载）、DiagnoseResult/AstNodeWire/DiagnosticEntry/PositionEntry/SpanEntry、KeywordSearchRequest（epoch 为 i64，term_operator 为 or/and 枚举）等；handler 用既有 to_api_model serde 转换模式对接。
- metrics.rs：MetricsHistoryResponse 更名为 AggregatedMetric 并补 metric_type；新增 MetricsCleanupResponse。
- 新增各缺位响应：StartWatchResponse/StopWatchResponse、DeleteProjectResponse/ProjectIndexResponse/ReloadProjectConfigResponse/UpdateProjectConfigResponse、DeleteFileResponse 补 file_path、DeadLetterRetryResponse 补 truncated_chunks、ClassificationStatsResponse、重试队列三响应已存在改用、project 创建/更新的 created_at 统一为 RFC3339 字符串。
- handler 内 Query/请求局部结构体（IndexQuery、CallChainQueryParams、CallChainDirectionParams、RelationFilterParams、HistoryQueryParams、CleanupQueryParams、ConfigReloadQuery、ClassificationQueryParams、UpdateConfigRequest 的 config 改 JSON 值承载）迁入 cce-api，成为契约的一部分。
- project 的 record_to_config 两份实现合并为一份完整解析（修复 list/get 丢 extensions/exclude_dirs 等字段）。

### 3.3 消费方同步
- cce-cli：client.search_aggregated 返回 SearchResponse；tools/agg_search/batch_compress/metrics 命令按新类型改写。
- cce-mcp 自带 schemars 类型暂不并入（独立协议面），列为后续观察项。

## 4. Phase 1：utoipa 文档层

- workspace 引入 utoipa 5（本地 cargo 缓存已有 5.5.0），依赖仅加在 cce-api 与 cce-server 两处。
- cce-api 全部 wire 模型加 ToSchema（查询参数结构加 IntoParams）；serde_json::Value 字段按自由对象文档化（config info、metrics/json、classification relations 等逃生舱，语义为"如实的未定型"）。
- 66 个 handler 加 #[utoipa::path]；openapi.rs 的 ApiDoc 聚合 paths/components/tags/security 为空（无鉴权面）。
- 快照测试 openapi_snapshot_matches：默认逐字节比对 frontend/openapi.json，CCE_REFRESH_OPENAPI=1 时写入。
- 路由一致性测试：解析 router.rs 的 .route( 注册集合，与 openapi.json 的 (method, path) 集合比对，拦截"只加路由不加注解"。
- debug 构建挂载 GET /api-docs/openapi.json（cfg!(debug_assertions)），release 无文档面，无 Swagger UI。

## 5. Phase 2：codegen 与前端切换

- 新建 tools/openapi-codegen：独立 npm 小包（openapi-typescript 7 + typescript 5），gen 脚本读 ../../frontend/openapi.json 输出 schema.d.ts，人工复制到 frontend/src/lib/api/schema.d.ts 入库（与 wf-agent 相同流程，node_modules 不入库）。
- 前端 13 个 api 文件删除手写 interface，改为从 schema.d.ts 导出类型别名（components['schemas']），apiClient（重试/退避/错误解包）保留手写——路径类型化调用（openapi-fetch）不在本期范围，收益主要是编译期路径检查，代价是重写传输层重试语义。
- 组件适配变更的 wire 形状：DiagnosisTool（result 信封）、CompressTool（result 信封）、keyword-search 请求（epoch 数值化、term_operator）、batch compress 条目、QdrantProcessStatus 内部标签形态、watch start/stop 响应。
- frontend-preview 由 sync 脚本重同步；mock 数据按新形状修正。

## 6. 验证与守护

- Rust 侧：cargo clippy --all-targets --all-features 一次集中验证 + cargo test（快照测试即契约测试）。
- 前端：svelte-check 一次。
- 日常纪律（写入文档）：改 handler/模型 → 跑快照测试刷新 openapi.json → codegen 重生成 schema.d.ts → 提交。快照漂移由 cargo test 拦截。

## 7. 明确不做

- 不做 build.rs 生成、不做起服务拉取文档、不装 Swagger UI/CDN。
- 不把 utoipa 引入 cce-types/cce-parser/cce-orchestrator 等领域层。
- 不迁移 openapi-fetch 类型化客户端（后续可评估）。
- 不合并 cce-mcp 的 schemars 类型面。
- 不处理向后兼容：wire 形状变更直接生效，前端/CLI 在同一变更内同步。

## 8. 落地记录

三阶段均已完成。快照现状：61 条 path（含 5 组同路径多方法复用），66 个操作，124 个 schema，13 个 tag。

- Phase 0 按 3.1–3.3 落地，cce-api 为唯一 wire 真相，server 与 CLI 消费方同步，cargo check 通过。
- Phase 1 落地时发现三处与计划不同的 utoipa 行为，均已处理：查询参数结构体必须显式标注参数位置（缺省为 path 而非 query）；IntoParams 不展开 serde flatten，RelationFilterParams 的三个字段内联进两个 CallChain 查询结构体，server 侧由参数重建过滤结构体；自递归的 AstNodeInfo 与 SymbolInfo 需加 no_recursion，否则文档构建栈溢出。
- Phase 2 按计划落地：tools/openapi-codegen 独立 npm 包，schema.d.ts 入库，前端 12 个 api 文件改为生成类型别名，传输层与重试语义保持手写。toolsApi 改为解开带内信封后返回 result，失败抛错；agg-search 返回 SearchResponse；前端删除 call_chain 展示、cache_storage 展示与 parse 语言输入框；preview 重同步并修正 mock 形状；同步脚本补上遗漏的 utils 目录。
- 验证：cargo clippy 全工作区通过（剩余告警均为计划外旧代码）；cargo test 相关 crate 通过；svelte-check 主前端与 preview 均为零新增问题。
- 遗留环境问题（非本变更引入，不在本期内修）：frontend 与 preview 的 cytoscape tarball 被 npm 镜像拒绝，GraphCanvas 的 8 个报错待依赖可安装后自然消除。
