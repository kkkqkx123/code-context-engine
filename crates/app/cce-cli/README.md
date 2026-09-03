# CCE CLI - Code Context Engine 命令行客户端

独立的命令行客户端，通过 HTTP API 与 Code Context Engine 服务端通信。

## 功能特性

- **索引管理**: 执行全量索引、增量索引、单文件解析
- **代码搜索**: 向量搜索、BM25搜索、混合搜索
- **项目管理**: 创建、查看、更新、删除项目
- **实体查询**: 函数详情、调用链、类继承关系
- **文件监控**: 启动/停止目录监控
- **存储管理**: 查看存储状态、清理索引
- **工具命令**: 代码压缩、诊断、符号查找

## 安装

```bash
cd cce-cli
cargo build --release
```

编译后的二进制文件位于 `target/release/cce-cli`。

## 使用方法

### 基本语法

```bash
cce-cli [OPTIONS] <COMMAND>
```

### 全局选项

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `-s, --server` | 服务端 URL | `http://localhost:3000` |
| `-f, --format` | 输出格式 (table/json/plain) | `table` |
| `-v, --verbose` | 详细输出 | `false` |

也可以通过环境变量 `CCE_SERVER_URL` 设置服务端地址。

## 命令详解

### 索引命令

```bash
# 执行全量索引
cce-cli index run --path /path/to/project

# 指定文件扩展名
cce-cli index run --path /path/to/project --extensions rs,py,js

# 排除目录
cce-cli index run --path /path/to/project --exclude node_modules,target

# 强制重新索引
cce-cli index run --path /path/to/project --force

# 增量索引
cce-cli index incremental --add file1.rs,file2.rs --remove old.rs

# 解析单个文件
cce-cli index parse --file src/main.rs
```

### 搜索命令

```bash
# 基本搜索
cce-cli search query --query "function name"

# 指定搜索类型
cce-cli search query --query "error handling" --query-type vector
cce-cli search query --query "error handling" --query-type bm25
cce-cli search query --query "error handling" --query-type hybrid

# 限制结果数量
cce-cli search query --query "handler" --limit 20

# 过滤条件
cce-cli search query --query "handler" --extensions rs --directory src/api
cce-cli search query --query "handler" --entities function,method
cce-cli search query --query "handler" --languages rust,python

# 最小分数阈值
cce-cli search query --query "handler" --min-score 0.5
```

### 项目命令

```bash
# 创建项目
cce-cli project create --path /path/to/project --name my-project

# 列出所有项目
cce-cli project list

# 查看项目详情
cce-cli project get 1

# 更新项目
cce-cli project update 1 --name new-name

# 删除项目
cce-cli project delete 1

# 索引项目
cce-cli project index 1
```

### 实体命令

```bash
# 查看函数详情
cce-cli entity function 123

# 查看函数调用（被调用的函数）
cce-cli entity calls 123

# 查看函数调用者
cce-cli entity callers 123

# 查看调用链
cce-cli entity call-chain 123 --direction up    # 向上查找调用者
cce-cli entity call-chain 123 --direction down  # 向下查找被调用函数

# 查找调用路径
cce-cli entity call-path --from 123 --to 456

# 查看类继承关系
cce-cli entity inheritance 789

# 查看类实现关系
cce-cli entity implementations 789
```

### 监控命令

```bash
# 启动目录监控
cce-cli watch start --path /path/to/project

# 指定监控的文件扩展名
cce-cli watch start --path /path/to/project --extensions rs,py

# 设置防抖间隔
cce-cli watch start --path /path/to/project --debounce 1000

# 停止监控
cce-cli watch stop

# 查看监控状态
cce-cli watch status
```

### 存储命令

```bash
# 查看存储状态
cce-cli storage status

# 查看索引统计
cce-cli storage stats

# 清理索引
cce-cli storage clear

# 选择性清理
cce-cli storage clear --vectors true --bm25 false --relations false

# 删除文件
cce-cli storage delete-file src/old.rs

# 删除实体
cce-cli storage delete-entity 123

# 批量删除
cce-cli storage batch-delete --files file1.rs,file2.rs --entities 123,456
```

### 工具命令

```bash
# 压缩代码
cce-cli tools compress --code "fn main() { println!(\"hello\"); }"

# 诊断代码
cce-cli tools diagnose --code "fn test() { let x; x + 1 }"

# 获取符号列表
cce-cli tools symbols --file src/main.rs

# 查找引用
cce-cli tools references --symbol handle_request

# 跳转到定义
cce-cli tools definition --symbol handle_request
```

### 状态检查

```bash
# 检查服务端状态
cce-cli status
```

## 输出格式

### 表格格式（默认）

以表格形式展示结果，适合交互式使用。

```bash
cce-cli project list
```

### JSON 格式

以 JSON 格式输出，适合脚本处理。

```bash
cce-cli -f json project list
```

### 纯文本格式

以纯文本格式输出，适合管道处理。

```bash
cce-cli -f plain search query --query "handler"
```

## 配置

配置文件位于 `~/.config/cce-cli/config.toml`：

```toml
server_url = "http://localhost:3000"
output_format = "table"
timeout = 300
```

## 环境变量

| 变量 | 说明 |
|------|------|
| `CCE_SERVER_URL` | 服务端 URL |

## 示例工作流

### 1. 创建并索引项目

```bash
# 创建项目
cce-cli project create --path /home/user/my-project --name my-project

# 索引项目
cce-cli project index 1

# 检查状态
cce-cli status
```

### 2. 搜索代码

```bash
# 搜索函数
cce-cli search query --query "handle request" --limit 10

# 搜索特定类型
cce-cli search query --query "error" --entities function,method

# 搜索特定目录
cce-cli search query --query "config" --directory src/config
```

### 3. 分析调用关系

```bash
# 查看函数详情
cce-cli entity function 123

# 查看调用链
cce-cli entity call-chain 123 --direction down

# 查找调用路径
cce-cli entity call-path --from 123 --to 456
```

### 4. 实时监控

```bash
# 启动监控
cce-cli watch start --path /home/user/my-project

# 查看状态
cce-cli watch status

# 停止监控
cce-cli watch stop
```

## 架构设计

```
cce-cli/
├── Cargo.toml          # 依赖配置
├── README.md           # 文档
└── src/
    ├── main.rs         # 入口点
    ├── cli.rs          # CLI 参数定义
    ├── client.rs       # HTTP 客户端
    ├── config.rs       # 配置管理
    ├── types.rs        # API 类型定义
    ├── output.rs       # 输出格式化
    └── commands/       # 命令处理器
        ├── mod.rs
        ├── index.rs    # 索引命令
        ├── search.rs   # 搜索命令
        ├── project.rs  # 项目命令
        ├── entity.rs   # 实体命令
        ├── watch.rs    # 监控命令
        ├── storage.rs  # 存储命令
        ├── tools.rs    # 工具命令
        └── status.rs   # 状态命令
```

## 技术栈

| 组件 | 技术 |
|------|------|
| CLI 框架 | clap |
| HTTP 客户端 | reqwest |
| 序列化 | serde, serde_json |
| 终端输出 | colored, comfy-table, indicatif |
| 错误处理 | anyhow, thiserror |
| 异步运行时 | tokio |

## 与服务端 API 的对应关系

| CLI 命令 | HTTP API |
|----------|----------|
| `index run` | `POST /api/index` |
| `index incremental` | `POST /api/index/incremental` |
| `index parse` | `POST /api/parse` |
| `search query` | `POST /api/search` |
| `project create` | `POST /api/project` |
| `project list` | `GET /api/project` |
| `project get` | `GET /api/project/:id` |
| `project update` | `PUT /api/project/:id` |
| `project delete` | `DELETE /api/project/:id` |
| `project index` | `POST /api/project/:id/index` |
| `entity function` | `GET /api/function/:id` |
| `entity calls` | `GET /api/function/:id/calls` |
| `entity callers` | `GET /api/function/:id/callers` |
| `entity call-chain` | `GET /api/call-chain/:id` |
| `entity call-path` | `GET /api/call-path` |
| `entity inheritance` | `GET /api/class/:id/inheritance` |
| `entity implementations` | `GET /api/class/:id/implementations` |
| `watch start` | `POST /api/watch/start` |
| `watch stop` | `POST /api/watch/stop` |
| `watch status` | `GET /api/watch/status` |
| `storage status` | `GET /api/storage/status` |
| `storage stats` | `GET /api/index/stats` |
| `storage clear` | `DELETE /api/index` |
| `tools compress` | `POST /api/tools/compress` |
| `tools diagnose` | `POST /api/tools/diagnose` |
| `tools symbols` | `POST /api/tools/symbols` |
| `tools references` | `POST /api/tools/references` |
| `tools definition` | `POST /api/tools/definition` |
