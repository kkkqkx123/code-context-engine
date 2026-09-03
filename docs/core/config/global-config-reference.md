# 全局配置参考

## 概述

全局配置文件 (`config.toml`) 定义了 Code Context Engine 的核心服务配置，包括服务器、数据库、嵌入模型、LLM 等基础设施设置。这些配置通常在整个系统中共享，不应频繁更改。

**配置文件位置**: 项目根目录的 `config.toml`

## 配置结构

```toml
[server]              # 服务器配置
[database]            # 数据库配置（Qdrant, SQLite, BM25）
[embedder]            # 嵌入模型配置
[llm]                 # LLM 配置
[logger]              # 日志配置
[scanner]             # 文件扫描配置
[grouper]             # 实体分组配置
[orchestrator]        # 编排器配置
[relation]            # 关系索引配置
[ast_to_nl]           # AST 转自然语言配置
[summary]             # 摘要生成配置
[export]              # 导出配置
```

## 服务器配置

```toml
[server]
host = "0.0.0.0"     # 服务器监听地址
port = 9000          # 服务器端口 (1-65535)
```

### 环境变量覆盖

- `CCE_SERVER_HOST`: 覆盖 host
- `CCE_SERVER_PORT`: 覆盖 port

## 数据库配置

### Qdrant 向量数据库

```toml
[database.qdrant]
url = "http://localhost:6333"         # Qdrant 服务器 URL
api_key = ""                          # API 密钥（可选，支持 ${ENV_VAR} 占位符）
vector_size = 1024                    # 向量维度（必须与嵌入模型匹配）
distance_metric = "cosine"            # 距离度量: cosine, euclid, dot
timeout_ms = 30000                    # 请求超时时间（毫秒）
max_retries = 3                       # 最大重试次数
retry_delay_ms = 1000                 # 重试延迟（毫秒）
enabled = true                        # 是否启用 Qdrant
preset = "medium"                     # 集合预设: tiny, small, medium, large
```

#### 集合预设说明

| 预设 | 适用场景 | 向量数量范围 | HNSW |
|------|---------|------------|------|
| tiny | 测试/小型项目 | ≤ 2,000 | 禁用 |
| small | 小型项目 | 2,000 - 10,000 | 启用 |
| medium | 中型项目 | 10,000 - 100,000 | 启用 |
| large | 大型项目 | > 100,000 | 启用（优化） |

#### 环境变量覆盖

- `CCE_DB_QDRANT_URL`: 覆盖 url
- `CCE_DB_QDRANT_API_KEY`: 覆盖 api_key

### SQLite 元数据数据库

```toml
[database.sqlite]
path = "metadata.db"                  # 数据库文件路径
enable_wal = true                     # 启用 WAL 模式（提高并发性能）
enable_fk = true                      # 启用外键约束
synchronous = "NORMAL"                # 同步模式: OFF, NORMAL, FULL, EXTRA
cache_size = -64000                   # 缓存大小（KB），负数表示 KB，正数表示页数
busy_timeout_ms = 5000               # 锁等待超时时间（毫秒）
temp_store = "MEMORY"                 # 临时存储位置: DEFAULT, FILE, MEMORY, NONE
mmap_size = 268435456                 # 内存映射 I/O 大小（字节），0 表示禁用
```

#### 同步模式说明

| 模式 | 安全性 | 性能 | 推荐场景 |
|------|-------|------|---------|
| OFF | 低 | 最高 | 开发/测试环境 |
| NORMAL | 中 | 高 | **WAL 模式下的推荐设置** |
| FULL | 高 | 中 | 需要强一致性的生产环境 |
| EXTRA | 最高 | 最低 | 极端数据安全要求 |

#### 环境变量覆盖

- `CCE_DB_SQLITE_PATH`: 覆盖 path
- `CCE_DB_SQLITE_SYNC`: 覆盖 synchronous
- `CCE_DB_SQLITE_CACHE_SIZE`: 覆盖 cache_size
- `CCE_DB_SQLITE_BUSY_TIMEOUT`: 覆盖 busy_timeout_ms
- `CCE_DB_SQLITE_TEMP_STORE`: 覆盖 temp_store
- `CCE_DB_SQLITE_MMAP_SIZE`: 覆盖 mmap_size

### BM25 全文检索

```toml
[database.bm25]
enabled = true                        # 是否启用 BM25 索引
endpoint = "http://localhost:50051"   # gRPC 端点
index_name = "code_index"             # 默认索引名称
timeout_ms = 5000                     # 连接超时（毫秒）
max_retries = 3                       # 最大重试次数
retry_delay_ms = 100                  # 重试延迟（毫秒）

# BM25 算法参数（针对代码搜索优化）
[database.bm25.algorithm]
k1 = 1.8                              # 词频饱和参数（范围: 1.0-2.0）
b = 0.4                               # 文档长度归一化参数（范围: 0.0-1.0）

# 字段权重
[database.bm25.field_weights]
title = 3.0                           # 标题字段权重（实体名称 - 最高优先级）
content = 1.0                         # 内容字段权重（描述 - 基准）
keywords = 2.0                        # 关键词字段权重（提取的术语 - 高优先级）

# 搜索行为配置
[database.bm25.search]
default_limit = 10                    # 默认结果数量限制
max_limit = 100                       # 最大结果数量限制
enable_highlight = true               # 启用结果高亮
highlight_fragment_size = 200         # 高亮片段大小（字符）

# 索引管理器配置
[database.bm25.index_manager]
writer_memory_budget = 50000000       # 写入器内存预算（字节，默认 50MB）
reader_cache_enabled = true           # 启用读取器缓存
reload_policy = "on_commit_with_delay" # 重新加载策略: on_commit, on_commit_with_delay, manual

[database.bm25.index_manager.algorithm]
k1 = 1.2                              # 索引管理器的 k1 参数
b = 0.75                              # 索引管理器的 b 参数
```

#### BM25 参数调优指南

**k1 参数**（词频饱和度）:
- **较低值 (1.0-1.4)**: 适合通用文本搜索，快速饱和
- **较高值 (1.6-2.0)**: 适合代码搜索，强调精确标识符匹配
- **推荐**: 代码搜索使用 1.8

**b 参数**（文档长度归一化）:
- **较低值 (0.3-0.5)**: 减少对短文档的惩罚，适合代码实体
- **较高值 (0.6-0.9)**: 更强的长度归一化，适合长文档
- **推荐**: 代码搜索使用 0.4

## 嵌入模型配置

### 多提供者架构

```toml
[embedder]
default_model = "text-embedding-3-small"  # 默认使用的模型名称

# 提供者定义（连接信息）
[embedder.providers.openai]
base_url = "https://api.openai.com/v1"
api_keys = ["${EMB_API_KEY_OPENAI}"]      # 支持环境变量占位符
provider_type = "remote"                  # local 或 remote
timeout_secs = 30                         # 请求超时（秒）
max_retries = 3                           # 最大重试次数
retry_delay_ms = 1000                     # 重试延迟（毫秒）
proxy_url = ""                            # 代理 URL（可选）

[embedder.providers.ollama]
base_url = "http://localhost:11434/v1"
api_keys = []                             # 本地服务可使用空数组
provider_type = "local"
timeout_secs = 60
max_retries = 5
retry_delay_ms = 2000

# 模型定义（元数据）
[embedder.models.text-embedding-3-small]
vector_dimension = 1536                   # 向量维度（必须与实际模型匹配）
provider_id = "openai"                    # 引用提供者 ID
api_model_name = "text-embedding-3-small" # API 模型名称（可选）
max_batch_tokens = 8192                   # 每批次最大 token 数
max_item_tokens = 8191                    # 单个文本项最大 token 数

# 预处理器配置
[embedder.models.text-embedding-3-small.preprocessor]
prefix = ""                               # 文本前缀
template = ""                             # 文本模板

# BGE-M3 模型示例
[embedder.models.bge-m3]
vector_dimension = 1024
provider_id = "ollama"
api_model_name = "bge-m3"
```

### 环境变量占位符

在配置中使用 `${VAR_NAME}` 语法引用环境变量：

```toml
[embedder.providers.openai]
api_keys = ["${EMB_API_KEY_OPENAI}"]
```

系统会在启动时自动解析这些占位符。如果环境变量未设置，占位符将保持原样。

### 从文件加载 API 密钥

```toml
[embedder.providers.openai]
api_key_file = "/run/secrets/embedder_api_key"
```

这适用于 Docker Secrets 或其他密钥管理系统。

## LLM 配置

### 多提供者架构

```toml
[llm]
enabled = false                           # 是否启用 LLM 功能

# 多提供者设置
[llm.multi]
strategy = "round_robin"                  # 选择策略: round_robin, random, priority
max_retries = 3                           # 最大重试次数

# 健康检查配置
[llm.health]
enabled = true                            # 启用健康检查
interval_secs = 60                        # 检查间隔（秒）
timeout_secs = 10                         # 检查超时（秒）

# 提供者定义（以 id 作为键）
[llm.providers.openai-gpt4]
name = "OpenAI GPT-4"                     # 显示名称
provider_type = "remote"                  # local 或 remote
base_url = "https://api.openai.com/v1"
api_keys = ["${LLM_API_KEY_OPENAI}"]      # API 密钥
timeout_secs = 30                         # 请求超时（秒）
max_retries = 3                           # 最大重试次数
retry_delay_ms = 1000                     # 重试延迟（毫秒）
retry_jitter = 0.2                        # 重试延迟随机抖动比例（默认 0.2，追加 0~20%）
rate_limit_max_retries = 5                # 429 限流错误的独立重试预算（默认 5 次）
rate_limit_max_delay_ms = 60000           # 429 重试等待时间上限（毫秒，默认 60000）
rate_limit = 60                           # 每分钟最大请求数（0 = 不限速，同上游共享）

[llm.providers.openai-gpt4.circuit_breaker] # 熔断器配置（默认全启用）
enabled = true                            # 是否启用熔断（默认 true）
failure_threshold = 5                     # 连续失败多少次开启熔断（默认 5）
recovery_timeout_secs = 60                # 熔断后恢复探测前的等待秒数（默认 60）

[llm.providers.ollama-local]
name = "Ollama Local"
provider_type = "local"
base_url = "http://localhost:11434/v1"
api_keys = []                             # 本地服务可留空
timeout_secs = 60
max_retries = 5
retry_delay_ms = 2000
rate_limit = 0                            # 本地服务通常不限速
```

### 熔断器（circuit_breaker）

熔断器与限速器共享同一上游粒度（按 `base_url`）：同一上游的所有模型客户端（embedding / chat / rerank）共用同一个熔断器。计入熔断的失败包括 HTTP 5xx/网络错误、超时和无效响应；429 限流、鉴权、模型不存在等错误不计入。

- `failure_threshold`：连续失败计数达到该值后熔断开启（默认 5）。
- `recovery_timeout_secs`：熔断开启后，等待该秒数才允许一次半开探测请求（默认 60）。
- 多个提供者引用同一 `base_url` 时，首个注册的熔断配置生效。

### 选择策略

| 策略 | 说明 | 适用场景 |
|------|------|---------|
| round_robin | 轮询选择 | 负载均衡 |
| random | 随机选择 | 分散负载 |
| priority | 优先级选择 | 主备切换 |

## 日志配置

```toml
[logger]
level = "info"                            # 日志级别: trace, debug, info, warn, error
output = "stdout"                         # 输出目标: stdout, stderr, file
format = "pretty"                         # 输出格式: pretty, compact, json
file = ""                                 # 日志文件路径（当 output="file" 时必需）
```

### 环境变量覆盖

- `CCE_LOG_LEVEL`: 覆盖 level
- `CCE_LOG_OUTPUT`: 覆盖 output
- `CCE_LOG_FORMAT`: 覆盖 format
- `CCE_LOG_FILE`: 覆盖 file

## 摘要生成配置

`[summary]` 用于控制文件摘要的生成策略和长度限制。当前只支持少量可调字段，项目级配置也可以覆盖这些值。

```toml
[summary]
strategy = "auto"          # auto, rule_based, model_enhanced, minimal
max_summary_length = 2000  # 最大摘要长度（tokens）
max_entities = 10          # 摘要中最多抽取的实体数
max_imports = 10           # 摘要中最多保留的导入数
max_concurrent = 5         # 模型摘要请求的最大并发数
```

- `strategy`：决定摘要使用规则生成、模型增强还是最小输出策略。
- `max_summary_length`：对最终摘要做长度截断。
- `max_entities`：控制摘要里保留的主实体数量。
- `max_concurrent`：限制同时执行的模型摘要请求数量，最小有效值为 1。
- `max_imports`：控制摘要里保留的导入数量。

## 其他模块配置

以下模块的配置也可以在全局配置文件中设置，但通常建议在项目级配置中自定义：

- **[scanner](#)**: 文件扫描配置
- **[grouper](#)**: 实体分组配置
- **[orchestrator](#)**: 编排器配置
- **[relation](#)**: 关系索引配置
- **[ast_to_nl](#)**: AST 转自然语言配置
- **[summary](#)**: 摘要生成配置
- **[export](#)**: 导出配置

详细配置请参考 [项目级配置参考](./project-config-reference.md)。

## 配置验证

系统在启动时会自动验证配置：

1. **依赖关系检查**: 如果启用了某个功能，其依赖的功能也会自动启用
2. **值范围验证**: 配置值必须在有效范围内
3. **必填字段检查**: 必须提供必要的配置项

### 常见配置警告

| 警告 | 原因 | 解决方案 |
|------|------|----------|
| `store_vectors=true` 但 `qdrant.enabled=false` | 向量存储未启用 | 设置 `database.qdrant.enabled = true` |
| `store_bm25=true` 但 `bm25.enabled=false` | BM25 索引未启用 | 设置 `database.bm25.enabled = true` |
| `llm.enabled=true` 但无提供者配置 | LLM 配置不完整 | 至少配置一个 LLM 提供者 |
| `logger.output=file` 但 `file` 为空 | 日志文件路径未指定 | 设置 `logger.file` |

## 最佳实践

### 1. 使用环境变量管理敏感信息

```toml
[embedder.providers.openai]
api_keys = ["${EMB_API_KEY_OPENAI}"]

[database.qdrant]
api_key = "${DB_QDRANT_API_KEY}"
```

在 `.env` 文件中设置：

```bash
EMB_API_KEY_OPENAI=sk-xxx
DB_QDRANT_API_KEY=qdrant-api-key
```

### 2. 根据项目规模选择合适的预设

```toml
# 小型项目 (< 10,000 文件)
[database.qdrant]
preset = "small"

# 中型项目 (10,000 - 100,000 文件)
[database.qdrant]
preset = "medium"

# 大型项目 (> 100,000 文件)
[database.qdrant]
preset = "large"
```

### 3. 优化 SQLite 性能

```toml
[database.sqlite]
enable_wal = true              # 启用 WAL 模式
synchronous = "NORMAL"         # WAL 模式下推荐 NORMAL
cache_size = -64000            # 64MB 缓存
temp_store = "MEMORY"          # 内存临时存储
mmap_size = 268435456          # 256MB 内存映射
```

### 4. 配置多个 LLM 提供者以实现高可用

```toml
[[llm.providers]]
id = "primary"
base_url = "https://api.openai.com/v1"
# ...

[[llm.providers]]
id = "backup"
base_url = "https://api.anthropic.com/v1"
# ...

[llm.multi]
strategy = "priority"          # 优先使用 primary，失败时切换到 backup
```

## 相关文档

- [项目级配置参考](./project-config-reference.md)
- [环境变量配置参考](./env-config-reference.md)
