# 前端日志管理功能设计方案

## 一、需求概述

当前前端已有 `LogViewer.svelte` 组件，但仅用于展示文件监听事件（create/modify/delete），并非应用日志。本方案旨在为前端补充**应用日志管理**功能，使用户能够通过 Web UI 实时查看、筛选、搜索系统运行日志，并管理日志文件。

### 与现有 LogViewer 的区别

| 维度 | 现有 LogViewer | 新增日志管理 |
|------|---------------|-------------|
| 数据源 | 文件监听事件流 | 应用运行日志 |
| 事件类型 | create/modify/delete | debug/info/warn/error |
| 目的 | 展示文件变更动态 | 运维监控、故障排查 |

## 二、整体架构

```
┌─────────────────────────────────────────────────────┐
│                    Frontend (Svelte)                  │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────┐ │
│  │ RealtimeLog  │  │ LogSearch    │  │ LogFile   │ │
│  │ Viewer       │  │ Panel        │  │ Manager   │ │
│  └──────┬───────┘  └──────┬───────┘  └─────┬─────┘ │
│         │                  │                 │       │
│         ▼                  ▼                 ▼       │
│  ┌──────────────────────────────────────────────┐   │
│  │              /api/logs/*  (REST)              │   │
│  └──────────────────┬───────────────────────────┘   │
└─────────────────────┼───────────────────────────────┘
                      │ HTTP / WebSocket
┌─────────────────────┼───────────────────────────────┐
│                 cce-server                           │
│  ┌──────────────────▼───────────────────────────┐   │
│  │           Log Management Module               │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────────┐  │   │
│  │  │ Log      │ │ Log      │ │ Log File     │  │   │
│  │  │ Capture  │ │ Query    │ │ Operations   │  │   │
│  │  └──────────┘ └──────────┘ └──────────────┘  │   │
│  │  ┌────────────────────────────────────────┐   │   │
│  │  │   In-Memory Ring Buffer + Broadcast    │   │   │
│  │  └────────────────────────────────────────┘   │   │
│  └───────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

## 三、后端 API 设计

### 实时日志流

**WebSocket 端点**：`/ws/logs`

服务端通过 `tokio::sync::broadcast` 将 tracing 日志事件广播给所有 WebSocket 客户端。客户端连接后实时接收格式化后的日志条目。

消息格式为 JSON，包含以下字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `timestamp` | string | ISO 8601 时间戳 |
| `level` | string | TRACE / DEBUG / INFO / WARN / ERROR |
| `target` | string | 日志来源模块（如 `cce_server::api`） |
| `message` | string | 日志消息正文 |
| `fields` | object | 可选的结构化字段（key-value） |

### 日志查询

**GET `/api/logs`**

查询参数：

| 参数 | 类型 | 说明 |
|------|------|------|
| `start_time` | ISO 8601 | 起始时间（含） |
| `end_time` | ISO 8601 | 结束时间（含） |
| `level` | string | 最低级别过滤 |
| `module` | string | 模块名模糊匹配 |
| `query` | string | 消息内容关键词搜索 |
| `page` | integer | 页码（从 1 开始） |
| `page_size` | integer | 每页条数（默认 100） |

### 日志文件管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/logs/files` | 列出日志目录下所有文件（文件名、大小、修改时间） |
| GET | `/api/logs/files/{name}/download` | 下载指定日志文件 |
| DELETE | `/api/logs/files/{name}` | 删除已轮转的历史文件（当前活跃文件不可删除） |
| POST | `/api/logs/rotate` | 手动触发日志轮转 |

### 日志配置

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/logs/config` | 获取当前日志配置 |
| PUT | `/api/logs/config` | 热更新日志级别和输出格式 |

---

## 四、后端实现要点

### 日志捕获层

在 `tracing_subscriber` 的 `Registry` 上添加自定义 `Layer`，该 Layer 将每个日志事件做两件事：

1. 写入固定容量的环形缓冲区（用于历史查询）
2. 通过 `tokio::sync::broadcast` channel 推送给 WebSocket 订阅者

### 环形缓冲区

使用 `crossbeam::ArrayQueue<LogEvent>` 实现固定容量环形缓冲（默认上限 10000 条）。当队列满时，新日志覆盖最旧日志。该缓冲区仅支撑实时查询场景，历史日志依赖磁盘文件。

### 文件扫描

日志文件管理 API 通过 `std::fs` 扫描日志目录（路径从 `[logger].file` 配置的父目录获取），返回文件名、大小、修改时间等元信息。

### 热更新

利用 `tracing_subscriber::reload::Layer` 包装 `EnvFilter`，管理 API 修改 filter 指令后调用 `reload::Handle::replace()` 即可即时生效，无需重启。

---

## 五、前端组件设计

### RealtimeLogViewer

全屏日志实时流面板，类似 `htop` 风格。

- **顶部筛选栏**：级别多选（TRACE/DEBUG/INFO/WARN/ERROR）、模块下拉选择、关键词搜索框
- **主体区域**：可滚动日志列表，按级别着色（ERROR 红、WARN 黄、INFO 绿、DEBUG 灰）
- **底部状态栏**：WebSocket 连接状态、当前展示条目数、日志速率（条/秒）
- **交互控制**：暂停/恢复自动滚动、清空视图、导出当前视图为 JSON

### LogSearchPanel

独立的历史日志查询页面。

- **左侧面板**：时间范围选择器（快捷选项：最近 5 分钟 / 1 小时 / 24 小时 / 自定义）、级别多选、模块输入框、关键词搜索框、查询按钮
- **右侧结果表格**：分页表格展示查询结果，支持导出 CSV

### LogFileManager

文件列表管理页面。

- **表格列**：文件名、大小、修改时间、操作（下载 / 删除）
- **顶部操作**：立即轮转按钮、刷新按钮
- **安全限制**：当前活跃日志文件（正在写入）不可删除

### LogConfigPanel

日志配置管理页面。

- **只读展示**：当前生效的级别、输出目标、格式、文件路径
- **可修改项**：级别切换下拉框（trace / debug / info / warn / error）、格式选择（pretty / compact / json）
- **即时生效**：修改后通过 PUT API 热更新，无需重启

---

## 六、前端路由规划

| 路由 | 组件 | 说明 |
|------|------|------|
| `/logs/realtime` | RealtimeLogViewer | 实时日志流 |
| `/logs/search` | LogSearchPanel | 历史日志搜索 |
| `/logs/files` | LogFileManager | 日志文件管理 |
| `/logs/config` | LogConfigPanel | 日志配置管理 |

---

## 七、配置扩展

在 `[logger]` 段新增前端日志管理相关配置：

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `enable_http_api` | bool | true | 是否启用日志 HTTP API |
| `ws_capacity` | integer | 256 | WebSocket 广播通道容量 |
| `memory_buffer_size` | integer | 10000 | 内存环形缓冲区容量 |
| `max_file_size_mb` | integer | 无 | 单文件大小上限（可选） |
| `max_files` | integer | 无 | 最大保留文件数（可选） |
| `max_total_size_mb` | integer | 无 | 日志目录总大小上限（可选） |

---

## 八、实施步骤

1. 抽取 `cce-server/src/logger.rs` 为独立 `cce-logging` crate，统一所有入口的日志初始化
2. 在 `cce-logging` 中实现日志捕获 Layer、环形缓冲区、broadcast 通道
3. 在 `cce-server` 中实现日志查询、文件管理、配置热更新等 REST API
4. 实现 WebSocket 端点 `/ws/logs` 用于实时日志推送
5. 构建四个前端组件并接入路由
6. 前端导航菜单增加"日志管理"分组
