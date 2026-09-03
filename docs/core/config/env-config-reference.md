# 环境变量配置参考

## 概述

Code Context Engine 支持通过环境变量和 `.env` 文件配置敏感信息和运行时参数。环境变量具有最高优先级，可以覆盖配置文件中的设置。

## 配置优先级

```
1. 直接环境变量（最高优先级）
2. .env 文件
3. config.local.toml
4. config.toml
5. 全局默认值（最低优先级）
```

## .env 文件加载

系统会自动从以下位置加载 `.env` 文件（按顺序）：

1. **项目根目录**: 包含 `config.toml` 或 `Cargo.toml` 的目录
2. **可执行文件目录**: 程序所在目录
3. **当前工作目录**: 运行命令时的目录

第一个找到的 `.env` 文件会被加载。

### .env 文件示例

```bash
# Code Context Engine Environment Configuration

# =============================================================================
# Server Configuration
# =============================================================================
CCE_SERVER_HOST=0.0.0.0
CCE_SERVER_PORT=9000

# =============================================================================
# Database Configuration
# =============================================================================

# Qdrant Vector Database
CCE_DB_QDRANT_URL=http://localhost:6333
CCE_DB_QDRANT_API_KEY=qdrant-secret-key

# SQLite Metadata Database
CCE_DB_SQLITE_PATH=metadata.db
CCE_DB_SQLITE_SYNC=NORMAL
CCE_DB_SQLITE_CACHE_SIZE=-64000
CCE_DB_SQLITE_BUSY_TIMEOUT=5000
CCE_DB_SQLITE_TEMP_STORE=MEMORY
CCE_DB_SQLITE_MMAP_SIZE=268435456

# =============================================================================
# Embedder Configuration (Multi-Provider)
# =============================================================================

# OpenAI Embedding API Key
CCE_EMB_API_KEY_OPENAI=sk-xxx

# Ollama (no API key needed for local service)
# CCE_EMB_API_KEY_OLLAMA=

# Google Gemini API Key
CCE_EMB_API_KEY_GEMINI=AIzaSy...

# Azure OpenAI API Key
CCE_EMB_API_KEY_AZURE=azure-key

# =============================================================================
# LLM Configuration (Multi-Provider)
# =============================================================================

# OpenAI GPT API Key
CCE_LLM_API_KEY_OPENAI=sk-xxx

# Anthropic Claude API Key
CCE_LLM_API_KEY_ANTHROPIC=sk-ant-...

# Ollama Local (no API key needed)
# CCE_LLM_API_KEY_OLLAMA=

# =============================================================================
# Logger Configuration
# =============================================================================
CCE_LOG_LEVEL=info
CCE_LOG_OUTPUT=stdout
CCE_LOG_FORMAT=pretty
CCE_LOG_FILE=

# =============================================================================
# Docker Secrets Support
# =============================================================================
# If using Docker secrets, set file paths instead of direct values
# CCE_EMB_API_KEY_FILE=/run/secrets/embedder_api_key
# CCE_LLM_API_KEY_FILE=/run/secrets/llm_api_key
```

## 环境变量列表

### 服务器配置

| 变量名 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `CCE_SERVER_HOST` | string | `0.0.0.0` | 服务器监听地址 |
| `CCE_SERVER_PORT` | integer | `9000` | 服务器端口 (1-65535) |

### 数据库配置

#### Qdrant

| 变量名 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `CCE_DB_QDRANT_URL` | string | `http://localhost:6333` | Qdrant 服务器 URL |
| `CCE_DB_QDRANT_API_KEY` | string | - | Qdrant API 密钥（可选） |

#### SQLite

| 变量名 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `CCE_DB_SQLITE_PATH` | string | `metadata.db` | SQLite 数据库文件路径 |
| `CCE_DB_SQLITE_SYNC` | enum | `NORMAL` | 同步模式: OFF, NORMAL, FULL, EXTRA |
| `CCE_DB_SQLITE_CACHE_SIZE` | integer | `-64000` | 缓存大小（KB），负数表示 KB |
| `CCE_DB_SQLITE_BUSY_TIMEOUT` | integer | `5000` | 锁等待超时时间（毫秒） |
| `CCE_DB_SQLITE_TEMP_STORE` | enum | `MEMORY` | 临时存储: DEFAULT, FILE, MEMORY, NONE |
| `CCE_DB_SQLITE_MMAP_SIZE` | integer | `268435456` | 内存映射 I/O 大小（字节） |

### 嵌入模型配置

#### 多提供者 API 密钥

使用占位符语法在 `config.toml` 中引用环境变量：

```toml
[embedder.providers.openai]
api_keys = ["${CCE_EMB_API_KEY_OPENAI}"]

[embedder.providers.ollama]
api_keys = []  # 本地服务不需要 API 密钥
```

环境变量命名规则: `CCE_EMB_API_KEY_{PROVIDER_ID}`

| 变量名示例 | 说明 |
|-----------|------|
| `CCE_EMB_API_KEY_OPENAI` | OpenAI 嵌入 API 密钥 |
| `CCE_EMB_API_KEY_OLLAMA` | Ollama API 密钥（通常为空） |
| `CCE_EMB_API_KEY_GEMINI` | Google Gemini API 密钥 |
| `CCE_EMB_API_KEY_AZURE` | Azure OpenAI API 密钥 |

### LLM 配置

#### 多提供者 API 密钥

使用占位符语法在 `config.toml` 中引用环境变量：

```toml
[[llm.providers]]
id = "openai-gpt4"
api_keys = ["${CCE_LLM_API_KEY_OPENAI}"]

[[llm.providers]]
id = "anthropic-claude"
api_keys = ["${CCE_LLM_API_KEY_ANTHROPIC}"]
```

环境变量命名规则: `CCE_LLM_API_KEY_{PROVIDER_ID}`

| 变量名示例 | 说明 |
|-----------|------|
| `CCE_LLM_API_KEY_OPENAI` | OpenAI GPT API 密钥 |
| `CCE_LLM_API_KEY_ANTHROPIC` | Anthropic Claude API 密钥 |
| `CCE_LLM_API_KEY_OLLAMA` | Ollama API 密钥（通常为空） |
| `CCE_LLM_API_KEY_AZURE` | Azure OpenAI API 密钥 |

### 日志配置

| 变量名 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `CCE_LOG_LEVEL` | enum | `info` | 日志级别: trace, debug, info, warn, error |
| `CCE_LOG_OUTPUT` | enum | `stdout` | 输出目标: stdout, stderr, file |
| `CCE_LOG_FORMAT` | enum | `pretty` | 输出格式: pretty, compact, json |
| `CCE_LOG_FILE` | string | - | 日志文件路径（当 CCE_LOG_OUTPUT=file 时必需） |

## 环境变量占位符语法

在 `config.toml` 中使用 `${VAR_NAME}` 语法引用环境变量：

```toml
[database.qdrant]
url = "${CCE_DB_QDRANT_URL}"
api_key = "${CCE_DB_QDRANT_API_KEY}"

[embedder.providers.openai]
base_url = "https://api.openai.com/v1"
api_keys = ["${CCE_EMB_API_KEY_OPENAI}"]

[[llm.providers]]
id = "openai-gpt4"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_keys = ["${CCE_LLM_API_KEY_OPENAI}"]
```

### 占位符解析规则

1. **如果环境变量存在**: 替换为环境变量的值
2. **如果环境变量不存在**: 保持占位符原样（可能导致错误）
3. **多个占位符**: 每个占位符独立解析

```toml
# 示例：部分解析
# 假设 CCE_EMB_API_KEY_OPENAI 已设置，但 CCE_DB_QDRANT_API_KEY 未设置
api_keys = ["${CCE_EMB_API_KEY_OPENAI}"]  # → ["sk-xxx"]
api_key = "${CCE_DB_QDRANT_API_KEY}"      # → "${CCE_DB_QDRANT_API_KEY}" (保持原样)
```

## 从文件加载 API 密钥

对于 Docker Secrets 或其他密钥管理系统，可以使用文件路径代替直接的值：

```toml
[embedder.providers.openai]
api_key_file = "/run/secrets/embedder_api_key"

[[llm.providers]]
id = "openai-gpt4"
api_key_file = "/run/secrets/llm_api_key"
```

系统会在启动时读取文件内容（去除首尾空白）作为 API 密钥。

### Docker Compose 示例

```yaml
version: '3.8'

services:
  code-context-engine:
    image: cce:latest
    secrets:
      - embedder_api_key
      - llm_api_key
    environment:
      - CCE_DB_QDRANT_URL=http://qdrant:6333
    volumes:
      - ./config.toml:/app/config.toml

secrets:
  embedder_api_key:
    file: ./secrets/embedder_api_key.txt
  llm_api_key:
    file: ./secrets/llm_api_key.txt
```

## 环境变量验证

系统在启动时会验证所有必需的 API 密钥环境变量是否已设置。

### 验证规则

1. **检查占位符**: 扫描配置中的所有 `${VAR_NAME}` 占位符
2. **验证存在性**: 确保对应的环境变量已设置
3. **报告缺失**: 如果有缺失的环境变量，启动失败并显示错误信息

### 错误示例

```
Error: Required environment variables not set: 
  - embedder.provider.openai.CCE_EMB_API_KEY_OPENAI
  - llm.provider.openai-gpt4.CCE_LLM_API_KEY_OPENAI
```

### 解决方案

在 `.env` 文件或 shell 环境中设置缺失的变量：

```bash
export CCE_EMB_API_KEY_OPENAI=sk-xxx
export CCE_LLM_API_KEY_OPENAI=sk-yyy
```

## 使用场景

### 场景 1: 开发环境

`.env.development`:
```bash
CCE_SERVER_HOST=localhost
CCE_SERVER_PORT=9000

CCE_DB_QDRANT_URL=http://localhost:6333
CCE_DB_SQLITE_PATH=dev_metadata.db

CCE_EMB_API_KEY_OPENAI=sk-dev-key
CCE_LLM_API_KEY_OPENAI=sk-dev-key

CCE_LOG_LEVEL=debug
CCE_LOG_OUTPUT=stdout
CCE_LOG_FORMAT=pretty
```

### 场景 2: 生产环境

`.env.production`:
```bash
CCE_SERVER_HOST=0.0.0.0
CCE_SERVER_PORT=9000

CCE_DB_QDRANT_URL=https://qdrant.example.com
CCE_DB_QDRANT_API_KEY=${QDRANT_PROD_KEY}
CCE_DB_SQLITE_PATH=/data/metadata.db
CCE_DB_SQLITE_SYNC=FULL

CCE_EMB_API_KEY_OPENAI=${OPENAI_PROD_KEY}
CCE_LLM_API_KEY_OPENAI=${OPENAI_PROD_KEY}

CCE_LOG_LEVEL=warn
CCE_LOG_OUTPUT=file
CCE_LOG_FORMAT=json
CCE_LOG_FILE=/var/log/cce/app.log
```

### 场景 3: CI/CD 环境

```bash
# GitHub Actions workflow
env:
  CCE_SERVER_HOST: localhost
  CCE_SERVER_PORT: 9000
  CCE_DB_QDRANT_URL: http://localhost:6333
  CCE_EMB_API_KEY_OPENAI: ${{ secrets.CCE_EMB_API_KEY }}
  CCE_LLM_API_KEY_OPENAI: ${{ secrets.CCE_LLM_API_KEY }}
  CCE_LOG_LEVEL: info
```

### 场景 4: Docker 容器

```dockerfile
FROM rust:latest

WORKDIR /app
COPY . .

# Build
RUN cargo build --release

# Runtime configuration via environment variables
ENV CCE_SERVER_HOST=0.0.0.0
ENV CCE_SERVER_PORT=9000
ENV CCE_DB_QDRANT_URL=http://qdrant:6333
ENV CCE_LOG_LEVEL=info

EXPOSE 9000

CMD ["./target/release/code-context-engine"]
```

## 最佳实践

### 1. 使用 .gitignore 保护敏感信息

`.gitignore`:
```gitignore
# Environment files
.env
.env.local
.env.*.local

# Secret files
secrets/
*.key
*.pem
```

### 2. 提供 .env.example 模板

`.env.example`:
```bash
# Copy this file to .env and fill in your values

# Server
CCE_SERVER_HOST=0.0.0.0
CCE_SERVER_PORT=9000

# Database
CCE_DB_QDRANT_URL=http://localhost:6333
CCE_DB_QDRANT_API_KEY=your-qdrant-api-key

# Embedder
CCE_EMB_API_KEY_OPENAI=your-openai-api-key

# LLM
CCE_LLM_API_KEY_OPENAI=your-openai-api-key

# Logger
CCE_LOG_LEVEL=info
CCE_LOG_OUTPUT=stdout
CCE_LOG_FORMAT=pretty
```

### 3. 区分不同环境的配置

```
.env                # 默认配置（提交到版本控制，不含敏感信息）
.env.local          # 本地覆盖（不提交，个人设置）
.env.development    # 开发环境
.env.staging        # 预发布环境
.env.production     # 生产环境（严格保护）
```

### 4. 使用环境变量管理多提供者密钥

```bash
# .env
CCE_EMB_API_KEY_OPENAI=sk-openai-key
CCE_EMB_API_KEY_GEMINI=AIzaSy-gemini-key
CCE_EMB_API_KEY_AZURE=azure-key

CCE_LLM_API_KEY_OPENAI=sk-openai-key
CCE_LLM_API_KEY_ANTHROPIC=sk-ant-claude-key
```

```toml
# config.toml
[embedder.providers.openai]
api_keys = ["${CCE_EMB_API_KEY_OPENAI}"]

[embedder.providers.gemini]
api_keys = ["${CCE_EMB_API_KEY_GEMINI}"]

[[llm.providers]]
id = "openai"
api_keys = ["${CCE_LLM_API_KEY_OPENAI}"]

[[llm.providers]]
id = "anthropic"
api_keys = ["${CCE_LLM_API_KEY_ANTHROPIC}"]
```

### 5. 定期轮换 API 密钥

```bash
# 创建密钥轮换脚本
#!/bin/bash
# rotate_keys.sh

# Generate new keys (example)
export CCE_EMB_API_KEY_OPENAI=$(generate_new_key)
export CCE_LLM_API_KEY_OPENAI=$(generate_new_key)

# Restart service
systemctl restart code-context-engine
```

## 故障排查

### 问题 1: 环境变量未生效

**症状**: 配置仍使用默认值或配置文件中的值

**原因**: 
- `.env` 文件未在正确位置
- 环境变量名称拼写错误
- 环境变量在程序启动后设置

**解决方案**:
```bash
# 检查环境变量是否已设置
echo $EMB_API_KEY_OPENAI

# 检查 .env 文件位置
ls -la .env

# 手动加载 .env 文件
source .env

# 重新启动程序
cargo run
```

### 问题 2: 占位符未解析

**症状**: 日志显示 `"${VAR_NAME}"` 而非实际值

**原因**: 
- 环境变量未设置
- 占位符语法错误（缺少 `$` 或 `{}`）

**解决方案**:
```bash
# 设置环境变量
export VAR_NAME=value

# 检查占位符语法
# 正确: ${VAR_NAME}
# 错误: $VAR_NAME, {VAR_NAME}, %VAR_NAME%
```

### 问题 3: API 密钥验证失败

**症状**: 启动时报告 "Required environment variables not set"

**原因**: 
- 配置中使用了占位符但未设置对应环境变量

**解决方案**:
```bash
# 查看错误消息中的变量名
Error: Required environment variables not set: 
  - embedder.provider.openai.EMB_API_KEY_OPENAI

# 设置缺失的变量
export EMB_API_KEY_OPENAI=your-api-key

# 或在 .env 文件中添加
echo "EMB_API_KEY_OPENAI=your-api-key" >> .env
```
