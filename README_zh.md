# Code Context Engine

Code Context Engine（CCE）是一个使用 Rust 编写的服务端与命令行工具，用于代码库的索引与检索。
它把代码解析为语义实体，并将其转换为自然语言描述，再通过大模型生成向量存入向量数据库，
从而支持按意图查询代码。

## 核心特性

- **语义代码搜索** —— 使用自然语言提问（例如"用户认证逻辑在哪里？"），返回匹配的函数、类与代码片段，并附带文件路径与行号范围。
- **混合检索** —— 融合向量相似度、BM25 全文检索与基于关系图的扩展，并可选地通过大模型对结果进行重排。
- **关系图** —— 内置符号表、调用链、被调用者/调用者、类继承与依赖查询，底层由倒排索引支撑。
- **多语言 AST 解析** —— 基于 tree-sitter 的解析器覆盖 20 余种编程语言。
- **文档索引** —— 支持 Markdown、JSON、TOML、YAML、XML 以及多种纯文本格式，并针对不同格式采用专门的解析流程。
- **文件级摘要** —— 摘要会生成独立向量并参与检索，用于提升语义相关文件中代码片段的排名。
- **增量更新与文件监控** —— 仅重新索引变更内容，或通过实时监控保持索引最新。
- **多种交互入口** —— 提供 REST API、命令行客户端、Model Context Protocol（MCP）服务端以及 Web 控制台。
- **插件系统** —— 通过 Lua 脚本或原生动态库扩展或覆盖解析、文本生成、向量生成、分组、切块、重排等能力。

## 工作原理

索引流程：

```
扫描目录
  -> 解析文件（tree-sitter AST）
  -> 对相关实体分组
  -> 转换为自然语言（BM25 文本 + 语义摘要）
  -> 生成文件摘要
  -> 通过大模型服务批量生成向量
  -> 向量写入 Qdrant，元数据与关系写入 SQLite
```

查询时，引擎会同时检索向量库、BM25 索引与关系图，对候选结果进行合并、增益与可选重排，
最后返回排序后的结果。

存储后端：

| 后端 | 用途 |
|------|------|
| Qdrant | 实体、代码块与文件摘要向量 |
| Tantivy（内嵌 BM25） | 全文关键词检索与高亮片段 |
| SQLite | 项目元数据、实体存储、关系索引、缓存与历史记录 |

## 支持的语言

Rust、Python、JavaScript、TypeScript、TSX、Java、Go、C、C++、C#、Ruby、PHP、Kotlin、Scala、
Dart、Lua、Bash、HTML、CSS、Vue 与 Svelte。

文档与配置格式包括 Markdown、JSON、TOML、YAML、XML、INI、CSV、Makefile、RST、
Dockerfile 以及普通日志/文本文件。

## 环境要求

- Rust 1.86 或更高版本（用于编译可执行文件）
- 一个 Qdrant 实例（本地或远程）
- 一个向量模型服务。兼容 OpenAI 协议的提供方均可，例如 OpenAI、Azure OpenAI、Ollama 与 SiliconFlow。对话模型与重排模型为可选项。

## 快速开始

### 1. 编译

```bash
cargo build --release
```

编译完成后会在 `target/release/` 下生成三个可执行文件：

| 可执行文件 | 作用 |
|------------|------|
| `cce` | HTTP 服务端 |
| `cce-cli` | 命令行客户端 |
| `cce-mcp` | MCP 服务端 |

### 2. 配置

复制示例配置与环境变量文件：

```bash
cp config.example.toml config.toml
cp .env.example .env
```

在 `.env` 中填写服务提供方的 API Key，并确认 `config.toml` 中的 Qdrant 地址与要使用的向量模型。

```dotenv
CCE_EMB_API_KEY_SILICONFLOW=your-embedding-key
CCE_LLM_API_KEY_SILICONFLOW=your-chat-key
CCE_DB_QDRANT_URL=http://localhost:6333
```

数据库连接与日志相关设置可通过 `CCE_DB_*` 与 `CCE_LOG_*` 环境变量覆盖。模型提供方、模型与默认项
均在 `config.toml` 中配置。

### 3. 启动服务端

```bash
./target/release/cce
```

服务端默认监听 `0.0.0.0:9000`。通过 `CCE_CONFIG` 可以指定其他配置文件：

```bash
CCE_CONFIG=config.prod.toml ./target/release/cce
```

## 基本用法

### 命令行客户端

CLI 通过 HTTP 与服务端通信，默认服务端地址为 `http://localhost:3000`，可用 `-s` 参数或
`CCE_SERVER_URL` 环境变量覆盖。

```bash
# 指定服务端地址并检查状态
cce-cli -s http://localhost:9000 status

# 为代码库创建项目
cce-cli project create --path /path/to/project --name my-project

# 执行索引（把 1 替换为上面返回的项目 ID）
cce-cli project index 1

# 使用自然语言搜索
cce-cli search query --query "用户认证逻辑在哪里？" --limit 10

# 限定目录或实体类型
cce-cli search query --query "错误处理" --directory src/api --entities function,method

# 切换检索模式：vector、bm25 或 hybrid
cce-cli search query --query "重试逻辑" --query-type hybrid
```

查询实体与关系：

```bash
# 函数详情
cce-cli entity function 123

# 该函数的调用者与被调用函数
cce-cli entity callers 123
cce-cli entity calls 123

# 遍历调用链（up 表示向上查找调用者，down 表示向下查找被调用函数）
cce-cli entity call-chain 123 --direction down

# 类继承与实现关系
cce-cli entity inheritance 789
```

保持索引最新：

```bash
cce-cli watch start --path /path/to/project
cce-cli watch status
cce-cli watch stop
```

使用 `-f json` 或 `-f plain` 可获得便于脚本处理或管道传递的输出。

### HTTP API

所有接口位于 `/api` 路径下并返回 JSON。索引类接口需要提供 `project_id`。

```bash
# 创建项目
curl -X POST http://localhost:9000/api/project \
  -H "Content-Type: application/json" \
  -d '{"name": "My Project", "root_path": "/path/to/project", "extensions": ["rs", "py"]}'

# 索引项目
curl -X POST http://localhost:9000/api/project/1/index

# 搜索
curl -X POST http://localhost:9000/api/search \
  -H "Content-Type: application/json" \
  -d '{"project_id": 1, "query": "user authentication", "limit": 10}'
```

统一响应结构：

```json
{ "success": true, "data": { } }
```

### Web 控制台

`frontend/` 目录包含一个基于 SvelteKit 的控制台，可用于浏览项目、执行搜索、查看实体与管理索引。

```bash
cd frontend
npm install
npm run dev
```

开发服务器监听 `http://localhost:3001`，并将 API 请求代理到 `http://localhost:9000` 的后端。

### MCP 服务端

`cce-mcp` 将引擎能力以工具形式暴露给 MCP 客户端，例如 IDE 助手与智能体框架。
它支持 `stdio` 与 streamable HTTP 两种传输方式。

```toml
[mcp]
enabled = true
transport = "stdio"
```

可用工具包括 `search`、`keyword_search`、`entity_callees`、`entity_callers`、`call_chain`、
`list_projects`、`get_project`、`index_project`、`incremental_index` 与 `health`。
通过 `enabled_tools` 可以只暴露其中一部分，例如禁用写操作相关的工具。

## 插件

插件可以在不修改核心程序的前提下扩展或覆盖索引流程。支持两种插件类型：

- **Lua 脚本** —— 在带有内存与时间限制的沙箱虚拟机中执行。
- **原生动态库** —— 通过稳定的 C ABI 加载。

插件能力涵盖 AST 到自然语言的文本生成、文档解析、实体抽取、分组、切块、重排、查询改写、
结果过滤与文件过滤等。插件在 `.cce/plugins.json` 中注册，详见 `docs/user-guide/plugin/`。

## 文档

详细文档位于 `docs/` 目录：

- `docs/app/api/` —— HTTP API 参考
- `docs/architecture/` —— 系统设计与数据流
- `docs/user-guide/plugin/` —— 插件开发与能力参考
- `crates/app/cce-cli/README.md` —— 完整的 CLI 命令参考

## 许可证

GPL-3.0，详见 `LICENSE`。
