# 直接 print 保留决策留档

> 本文档是直接 print 迁移工作（见 `docs/print-migration-analysis.md` 与 `docs/plan/direct-print-logging-migration.md`）中"保留类"的精简决策留档，供后续维护者参考，避免误迁或误删。落地时间为 2026-11-22。

## 决策一句话

项目中绝大多数直接 print（共 400 处）不是日志技术债，而是**产品输出契约**或**引导期必要兜底**，永久保留、禁止迁移到 tracing。仅 gateway 的 13 处运行期诊断 eprintln 属于技术债，已迁移。

## 保留类别与理由

### 一、CLI 产品输出（379 处）

- 范围：`cce-cli/src/commands/*`、`cce-cli/src/output.rs`、`cce-cli/src/main.rs` 的全部 println/eprintln。
- 性质：CLI 的 stdout 是功能接口（返回值），不是日志。成功载荷走 stdout、失败走 stderr 是明确契约；`--format json` 模式依赖 stdout 输出可被机器解析的 JSON。
- 不可迁的原因：tracing 日志会与结果数据混流，直接破坏 JSON 等机器消费契约。
- 约定：CLI 输出统一经 `output.rs` 的封装函数（`print_success`/`print_error`/`print_warning`/`print_table`/`print_output`），新命令不得绕过封装直接打印。

### 二、gateway 进度协议（14 处 println）

- 范围：`cce-gateway/src/sync.rs`、`cce-gateway/src/watch.rs` 中受 `json_progress` / `verbose` 控制的 println。
- 性质：daemon/脚本集成的数据通道。`json_progress` 模式下输出 `{"kind":...}` 单行 JSON（scan/manifest/sync_complete/watching/pushing/staged 等），非 JSON 模式输出人读文本。
- 不可迁的原因：stdout 必须专供协议数据，任何日志混入都会破坏下游 JSON Lines 解析。

### 三、引导期错误提示（3 处）

- 范围与理由：
  - `cce-server/src/main.rs`：logger 初始化失败时的兜底，此时 tracing subscriber 不可用。
  - `cce-config/src/settings.rs` 的 `init_with_fallback`（2 处）：配置加载失败/未找到时的提示，该方法在三个二进制入口中都是最早被调用，早于任何 logger 初始化。
- 不可迁的原因：这些调用发生时 tracing subscriber 尚不存在或已判定不可用，迁到 tracing 会被丢弃，用户看不到关键启动错误。属正确的 bootstrap 例外。

## 边界规则（防误迁）

1. 新增日志一律用 `tracing` 宏；仅当"输出即产品数据"（CLI 结果、gateway 进度协议）或"logger 尚未初始化"（引导期）时才允许直接 print/eprintln。
2. stdout 为产品输出契约流，stderr 为诊断流；gateway 已在入口强制日志输出为 stderr 以维持此边界。
3. 判断某个 print 是否该保留，看它的消费者：下游程序或终端用户按格式解析它的，保留；仅开发者排障用的，应走 tracing。
