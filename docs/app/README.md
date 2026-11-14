# 应用层目录说明

`crates/app/` 只放产品可执行与装配代码，各 crate 的职责边界如下：

| crate | 职责 |
|---|---|
| `cce-api` | 线协议契约：请求/响应模型、错误码、OpenAPI 描述 |
| `cce-client` | 面向外部的 HTTP 客户端 |
| `cce-server` | HTTP 传输层：路由、参数解码、响应映射 |
| `cce-server-shared` | 应用服务层与组合根：领域用例、引擎装配、生命周期引导 |
| `cce-orchestrator` | 索引、查询与热更新编排 |
| `cce-mcp` | MCP 传输层，工具参数翻译与服务调用 |
| `cce-gateway` | 网关推送/消费边界 |
| `cce-cli` | 本地运维入口（进程监管、内嵌网关）与 HTTP 客户端 |
| `cce-admission` | 远程部署形态的准入中间件（feature 门控） |
| `cce-plugin-runtime` / `cce-plugin-sdk` | 插件宿主运行时与原生插件 ABI |

## 例外

- `cce-e2e-tests` 是测试资产，以 git 子模块形式登记在应用目录下。它不参与产品构建，
  也不随应用层职责调整而移动；需要区分产品 crate 与测试资产时以本说明为准。
- 基准支撑 crate 位于 `crates/bench/`（`cce-bench-kit`），不属于应用层。
