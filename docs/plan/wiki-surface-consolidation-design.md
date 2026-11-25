# Wiki 功能入口收敛设计

## 背景与命题

 Wiki Context Package（WCP）导出与增量失效 diff 目前通过 HTTP 端点对外提供，CLI 以 HTTP 客户端身份封装；MCP 侧另有一套以 wiki 命名的只读画像与调用预算；前端不消费 wiki。上一轮分析指出三处异义、注释失真、HTTP 面预算未对称等若干工程与认知问题。

本设计回应一个明确的架构命题：**wiki 功能是否应该仅通过 CLI 提供**。提出方论据是：MCP 主要面向 AI agent，而 AI agent 需要的是细粒度回查工具而非一次性数据包；CLI 更适合自动化工作流，也更适合被 skill、脚本与 CI 编排调用。

本文先辨析命题中「仅通过 CLI」的两种含义，给出结论，再把收敛过程中暴露的其余问题一并纳入设计方案。遵循 `AGENTS.md`：无向后兼容包袱、文档用简体中文、方案以自然语言为主、优先长期可维护性。

## 一、命题辨析：两种「仅 CLI」

「仅通过 CLI 提供 wiki」有两种截然不同的解读，必须先拆开。

| 解读 | 含义 | 可行性 |
|---|---|---|
| 强含义 | 删除 HTTP 端点，CLI 在进程内直接调用 `cce-server-shared::services::wiki` 导出 | 与 CLI 分层约定冲突，且丢失远程 server 场景 |
| 弱含义 | CLI 是 wiki 导出的唯一公开入口与文档化契约；HTTP 端点降级为内部传输；MCP 不承担导出职责 | 符合现有架构，改动小，收益明确 |

### 强含义为何不成立

CLI 的分层在入口文件中有明确声明：它同时是 HTTP 客户端与本地进程管理入口，业务命令保持为纯 HTTP 客户端，领域行为归属共享应用服务。`cce-cli` 的依赖清单只有 `cce-api` 与 `cce-client`，并不依赖 `cce-server-shared` 或 `cce-orchestrator`。要让它 in-process 调用导出服务，必须把整套 `SharedContext`（元数据、BM25、向量、关系运行时、指标缓存）拉进 CLI 进程，等于把 CLI 变成第二个 server。这既违背分层，又复制了状态装配逻辑，与「无向后兼容包袱、优先长期可维护」的规范背道而驰。

此外，远程 server 是现有部署形态：`--server` 指向远端地址，CLI 只是调用方。删除 HTTP 端点会直接切断远程使用路径。

### 弱含义为何成立

AI agent 通过 MCP 拿到的是 `search`、`read_file`、`graph_ego` 等细粒度工具，用于生成文档时逐步回查；它没有能力也不应该消费一个动辄数万实体的 WCP 大包。WCP 的真实消费者是离线文档流水线、CI 任务、以及被 skill 或脚本编排的自动化步骤——这些场景的共同入口正是 CLI。CLI 既支持结构化 JSON 输出（`--format json`）也支持落盘（`--output`），天然适配 skill 解析与产物入库。

项目当前没有内置 skill 机制，也不需要新增：skill 属于消费侧编排层，CLI 的稳定参数与 JSON 输出已经是它所需的契约。

## 二、结论：收敛后的职责表

| 能力 | 公开入口 | 其他 surface 的处理 |
|---|---|---|
| WCP 导出（context） | `cce wiki context`，唯一公开入口 | HTTP 端点保留为 CLI 的内部传输，不再作为独立文档化能力对外承诺 |
| 增量失效 diff | `cce wiki diff`，唯一公开入口 | 同上 |
| 文档生成期回查 | MCP 只读工具面（含画像与预算） | 与导出正交，保留，但去除 wiki 命名 |
| HTTP 工具面 | 普通 API | 不承担 wiki 职责，因此不纳入 wiki 预算 |
| 前端 | 不提供 | 在设计文档中显式声明，避免被误判为漏做 |

一句话概括：**wiki 导出收敛到 CLI；wiki 回查留在 MCP 并正名；HTTP 退居传输层；前端明确缺席。** 这不是削弱能力，而是让每个 surface 只做自己擅长的事。

## 三、术语统一：消除三处 wiki 异义

当前 wiki 一词在三个 surface 指向三种东西，是认知混乱的主要来源。统一方案如下。

- **导出侧**继续叫 WCP 导出（`wiki context` / `wiki diff`），语义锁定为「确定性数据包」。
- **MCP 侧**的 `wiki_read` 画像更名为 `read_only`（wire label 同步改为 `read_only`），语义锁定为「只读工具集合 + 强制调用预算」。它只是恰好被文档生成流程使用，并非 wiki 专属。
- **文档**中新增术语表，明确「WCP 导出」「只读画像」「受限工具面」三者关系，禁止用 wiki 统称。

更名影响面：`cce-config` 的 `McpProfile` 枚举、`cce-mcp` 的画像解析与处理器、预算上下文的注释、`mcp-api.md`、既有设计文档与单测断言。按无兼容包袱原则直接改名，不保留别名。

## 四、注释与文档一致性修复

两处表述需要修正，它们不改变行为但会长期误导维护者。

- `cce-server-shared/src/services/wiki.rs` 顶部注释称服务「shared by the HTTP and CLI surfaces」。实际直接调用方只有 HTTP handler，CLI 经 HTTP 间接到达。改为准确描述：服务由 HTTP 面承载，CLI 与任何 HTTP 调用方经该面使用。
- 设计文档 `repowiki-data-supply-design.md` 称通道 A「本地导出，不引入外呼」。实际它是 HTTP 端点并走长任务超时。改为：通道 A 由 HTTP 端点提供，经 CLI 以本地或远程方式调用，导出过程不触发外部 LLM 调用。

## 五、MCP 只读画像的定位与默认策略

只读画像与调用预算是通道 B 的治理机制，服务于 AI agent 生成文档时的有限步回查。它与 WCP 导出正交，不应因导出收敛到 CLI 而删除；删除它等于让 agent 回查重新变为无界。

但默认策略应调整。当前 `profile` 默认 `full`（全开），是历史行为遗留。既然项目明确无向后兼容包袱，且 MCP 是可选启用面，默认改为 `read_only` 更安全：显式 `full` 才放宽。预算在只读画像下强制开启的规则保持不变。

预算计数核心已是传输无关设计（无 `rmcp` 依赖），本设计不改其结构，只更新命名与文档表述。

## 六、HTTP 面预算对称化问题的消解

上一轮将「HTTP 工具面无 wiki 预算」列为防御缺口。收敛后该缺口不再成立：wiki 回查的职责已明确归属 MCP 只读画像，HTTP 工具面不再承担 wiki 语义，因此不需要为它叠加 wiki 预算。

需要在文档中写清这条边界，防止后来者把「HTTP 工具面无预算」再度误判为遗漏。若将来出现非 MCP 的、确实需要 wiki 回查的下游，再按已传输无关的预算核心扩展到 HTTP 面，属于独立演进项，不纳入本次范围。

## 七、双 epoch 与前端缺席的文档固化

WCP 的双 epoch 是已落地的权衡：`files` 与类型实体取自元数据 generation，依赖图与指标取自关系快照，`epoch_aligned` 标志用于告知下游是否跨代。本设计不改代码，但在收敛后的导出契约文档中明确「下游一次生成 run 钉住一个包版本，`epoch_aligned=false` 时自行决定重试或降级」，把职责归属写死。

前端不消费 wiki 是定位使然——前端服务人类操作界面，WCP 是机器契约。在 `docs/frontend` 或架构文档中补一条声明：wiki/WCP 不作为前端功能，避免被误认为待补需求。

## 八、影响面与实施顺序

| 区域 | 改动 |
|---|---|
| `docs/plan/` 本文 | 确立收敛结论与术语表 |
| `docs/plan/repowiki-data-supply-design.md` | 修正通道 A 表述，补术语链接 |
| `docs/app/server/mcp-api.md` | `wiki_read` 更名为 `read_only`，更新默认画像 |
| `crates/core/cce-config/src/modules/mcp.rs` | `McpProfile::WikiRead` 更名，默认改 `read_only` |
| `crates/app/cce-mcp/src/{handler,profile,budget}.rs` | 同步枚举与 wire label，更新注释 |
| `crates/app/cce-server-shared/src/services/wiki.rs` | 修正顶部注释，不改逻辑 |
| 相关单测 | 断言中的画像名与wire label 同步 |
| `docs/frontend` 架构文档 | 补「前端不提供 wiki」声明 |

实施顺序：先落文档（术语表、通道 A 表述、前端声明），再改命名与默认值，最后跑验证。命名改动集中且机械，可一次完成。

## 九、验收标准

- `cce wiki context` / `cce wiki diff` 是文档中唯一的 wiki 导出入口；HTTP 端点仅在 API 参考中以内部传输身份出现。
- 配置中不存在 `wiki_read`，只有 `read_only`；默认画像为 `read_only`。
- `cce-mcp` 仍不提供 wiki 导出工具，只读画像保留且预算强制开启。
- `services/wiki.rs` 顶部注释不再声称 CLI 直接复用该服务。
- 术语表覆盖「WCP 导出」「只读画像」「受限工具面」三项。
- `cargo fmt --check`、`cargo check --workspace`、相关 crate 单测与 clippy 通过。

## 十、风险与待确认

- 更名会让已按 `wiki_read` 配置的部署失效。按无兼容包袱原则接受；若存在外部下游，需在发布说明中显式提示。
- 默认画像改为 `read_only` 后，原本依赖默认全开的 MCP 调用方会失去写工具。由于 MCP 默认关闭且需显式启用，影响面可控。
- 本文收敛的是公开契约与文档表述，不删除 HTTP 端点。若后续确需物理删除，须先解决 CLI 分层与远程 server 依赖，属于另立课题。
