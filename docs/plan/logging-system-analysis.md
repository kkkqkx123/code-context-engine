# 日志系统分析与改进方案

## 一、现有架构概览

### 技术选型

| 组件 | 技术 | 用途 |
|------|------|------|
| 日志门面 | `tracing` (workspace 依赖) | 统一的日志宏接口 |
| 格式化/过滤 | `tracing-subscriber` | EnvFilter + fmt/json 输出 |
| 文件追加 | `tracing-appender` | 非阻塞写入、按天轮转 |
| 配置解析 | `cce-config` | `[logger]` 配置段 + 环境变量覆盖 |

### 初始化入口

```
cce-server/src/main.rs → logger::init(&config)
  ├── init_stdout()  → fmt::layer().pretty/compact/json()
  ├── init_stderr()  → fmt::layer().with_writer(stderr)
  └── init_file()    → tracing_appender::non_blocking + daily rotation
```

各入口的初始化方式：

| 入口 | 初始化方式 | 输出目标 |
|------|-----------|---------|
| `cce-server` | 根据 `[logger].output` 配置选择 | stdout / stderr / file |
| `cce-cli` | `fmt::init()` 硬编码 | stderr（忽略配置文件） |
| `cce-mcp` | 自定义 fmt 订阅器 | stderr（stdio 协议要求 stdout 干净） |
| `cce-gateway` | 无显式初始化 | 依赖默认行为 |
| `cce-storage-vector-qdrant` | 手动写文件 | `qdrant_stderr.log` |

### 配置结构

```toml
[logger]
level  = "info"        # trace | debug | info | warn | error
output = "stdout"      # stdout | stderr | file
format = "pretty"      # pretty | compact | json
file   = ""            # output=file 时的日志文件路径
```

环境变量覆盖：`CCE_LOG_LEVEL`、`CCE_LOG_OUTPUT`、`CCE_LOG_FORMAT`、`CCE_LOG_FILE`。

---

## 二、现有日志轮转机制分析

### 已实现：按天轮转

`cce-server/src/logger.rs` 中的核心实现：

```rust
let file_appender = tracing_appender::daily(
    file_path.parent().unwrap_or(Path::new(".")),
    file_path.file_name().unwrap_or_default(),
);
```

- **轮转策略**：每天创建新文件，命名格式为 `<prefix>.YYYY-MM-DD`
- **写入方式**：`NonBlocking` 异步写入，`WorkerGuard` 保存在 `LOG_GUARD` 静态变量中
- **配置校验**：`output = "file"` 时 `file` 字段必须非空

### 未实现的关键机制

| 机制 | 状态 | 影响 |
|------|------|------|
| 文件大小限制 | 未实现 | 单日高流量可产生超大文件，无法控制磁盘占用 |
| 数量/总大小保留策略 | 未实现 | 日志文件无限增长，最终耗尽磁盘空间 |
| 历史日志清理 | 未实现 | 旧日志永不删除 |
| 轮转后压缩 | 未实现 | 日志以纯文本存储，占用额外空间 |
| 热更新日志级别 | 未实现 | 修改级别必须重启服务 |
| 按模块/按级别过滤 | 未实现 | 只有一个全局级别，无法针对单个 crate 调试 |

### 风险场景

1. **磁盘爆满**：无保留策略，长期运行后日志目录持续增长。
2. **性能退化**：单文件过大时，尾部追加不受影响，但全文搜索/查看变慢。
3. **CLI 行为不一致**：`cce-cli` 完全忽略 `[logger]` 配置，用户设置不生效。
4. **Gateway 无日志初始化**：作为独立进程没有 tracing 初始化，日志行为不可控。

---

## 三、改进方案

### 改进优先级

1. **高优先级：大小轮转 + 保留策略** — 防止磁盘耗尽
2. **中优先级：CLI/Gateway 日志一致性** — 统一使用 `[logger]` 配置
3. **中优先级：热更新级别** — 无需重启即可调整日志详细度
4. **低优先级：按模块过滤** — 细粒度调试能力

### 大小轮转 + 保留策略

当前 `tracing_appender::daily` 仅支持时间维度轮转。改进方案：

- 使用 `tracing_appender::rolling::custom` 组合时间+大小双维度，或选用 `tracing-appender` 的 builder 自定义策略。
- 在 `LoggingConfig` 中新增可选字段：
  - `max_file_size_mb` — 单文件大小上限
  - `max_files` — 最大保留文件数
  - `max_total_size_mb` — 日志目录总大小上限
- 后台启动轻量级清理任务：每次轮转后检查保留策略，删除超出限制的最旧文件。

### CLI/Gateway 日志一致性

- `cce-cli` 应读取 `Settings::logger()` 并调用 `logger::init()`，而非硬编码 `fmt::init()`。
- `cce-gateway` 应在 `main()` 中加载配置并初始化日志系统。
- 抽取 `cce-server/src/logger.rs` 为独立的 `cce-logging` crate，供所有二进制入口复用。

### 热更新日志级别

- 通过管理 API 或信号（如 `SIGUSR1`）动态修改 `EnvFilter` 指令。
- `tracing-subscriber` 的 `reload::Layer` 支持运行时替换 filter。
- 前端可通过管理页面直接触发级别切换。

### 按模块过滤

- 扩展 `EnvFilter` 指令格式，支持 `cce-server=debug,cce-codegraph=warn` 这类按模块设置。
- 在前端配置界面提供模块级别的日志控制。

---

## 四、各入口改进对照表

| 入口 | 当前问题 | 改进措施 |
|------|---------|---------|
| `cce-server` | 无大小限制、无保留策略 | 增加轮转 + 清理机制 |
| `cce-cli` | 忽略配置文件 | 复用 `logger::init()` |
| `cce-gateway` | 无日志初始化 | 增加配置加载 + 初始化 |
| `cce-mcp` | 硬编码 stderr | 保持 stderr（协议要求），但级别应可配置 |
| `cce-storage-vector-qdrant` | 写死文件名 | 纳入统一管理 |
