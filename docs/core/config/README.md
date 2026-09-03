# 配置文件说明

## 快速开始

Code Context Engine 使用三层配置系统，支持全局配置、项目级配置和环境变量覆盖。

**推荐阅读顺序**:
1. [全局配置参考](./global-config-reference.md) - 了解核心服务配置
2. [项目级配置参考](./project-config-reference.md) - 学习如何为项目自定义配置
3. [环境变量配置参考](./env-config-reference.md) - 掌握敏感信息管理

## 配置文件位置

### 配置层次结构

```
环境变量 (最高优先级)
    ↓ 覆盖
.env 文件
    ↓ 覆盖
项目本地配置 (<project>/.cce/config.local.toml)
    ↓ 覆盖
项目配置 (<project>/.cce/config.toml)
    ↓ 继承
全局配置 (config.toml)
    ↓ 回退
代码默认值 (最低优先级)
```

### 文件位置说明

| 配置文件 | 位置 | 用途 | 版本控制 |
|---------|------|------|----------|
| **全局配置** | 项目根目录 `config.toml` | 核心服务配置（服务器、数据库、API） | ✅ 提交 |
| **项目配置** | `<project>/.cce/config.toml` | 项目特定配置（扫描、索引、分组） | ✅ 提交 |
| **本地覆盖** | `<project>/.cce/config.local.toml` | 个人偏好设置（调试、临时覆盖） | ❌ 忽略 |
| **环境变量** | `.env` 文件或 shell | 敏感信息（API 密钥、密码） | ❌ 忽略 |

### 查找规则

**全局配置**:
- 从当前工作目录向上查找 `config.toml`
- 如果未找到，使用代码内置默认值

**项目配置**:
- 从指定目录向上查找 `.cce/config.toml`
- 也支持直接在目录中放置 `config.toml`
- 找到的第一个配置文件所在目录即为项目根目录

**本地覆盖**:
- 在项目配置文件的同一目录下查找 `config.local.toml`
- 仅覆盖项目配置中指定的字段

## 详细参考文档

### 📘 [全局配置参考](./global-config-reference.md)

涵盖所有核心服务配置的完整说明：

- **服务器配置**: host, port
- **数据库配置**: Qdrant, SQLite, BM25
- **嵌入模型配置**: 多提供者架构、模型注册表
- **LLM 配置**: 多提供者、健康检查、选择策略
- **日志配置**: 级别、输出、格式
- **预设配置**: 集合预设、批处理预设、grouper 预设

**适用场景**: 
- 首次安装和配置系统
- 管理 API 密钥和数据库连接
- 调整服务器和性能参数

### 📗 [项目级配置参考](./project-config-reference.md)

项目特定配置的详细说明：

- **嵌入模型选择**: 为不同项目选择不同模型
- **文件扫描**: 排除模式、gitignore 集成
- **实体分组**: 嵌套深度、模式检测
- **编排器**: 批处理、热更新、缓存
- **关系索引**: 调用图、依赖图
- **AST 转自然语言**: 分块策略、转换选项
- **摘要生成**: 本地/LLM/混合策略

**适用场景**:
- 为新项目创建配置
- 优化特定项目的索引性能
- 调整扫描和分组策略

### 📙 [环境变量配置参考](./env-config-reference.md)

环境变量和敏感信息管理的完整指南：

- **.env 文件**: 加载规则、位置查找
- **环境变量列表**: 所有支持的变量
- **占位符语法**: `${VAR_NAME}` 使用方法
- **Docker Secrets**: 从文件加载密钥
- **验证机制**: 必需变量检查
- **迁移指南**: 从单提供者到多提供者

**适用场景**:
- 管理 API 密钥和敏感信息
- Docker/Kubernetes 部署
- CI/CD 环境配置
- 多环境配置管理

## 配置示例

### 最小配置

使用 `config.minimal.toml` 作为起点：

```bash
cp config.minimal.toml config.toml
```

编辑 `config.toml`，至少配置以下内容：

```toml
[server]
host = "0.0.0.0"
port = 9000

[database.qdrant]
url = "http://localhost:6333"
vector_size = 1024
enabled = true

[database.sqlite]
path = "metadata.db"

[embedder]
default_model = "text-embedding-3-small"

[embedder.providers.openai]
base_url = "https://api.openai.com/v1"
api_keys = ["${EMB_API_KEY_OPENAI}"]

[embedder.models.text-embedding-3-small]
vector_dimension = 1536
provider_id = "openai"
```

在 `.env` 文件中设置 API 密钥：

```bash
EMB_API_KEY_OPENAI=sk-your-api-key
```

### 完整配置

参考 `config.example.toml` 查看所有可用配置项及其默认值：

```bash
cp config.example.toml config.toml
```

然后根据需要注释掉不需要的配置项。

## 环境变量使用

### 常用环境变量

#### 服务器配置
```bash
export CCE_SERVER_HOST="0.0.0.0"
export CCE_SERVER_PORT="9000"
```

#### 数据库配置
```bash
# Qdrant
export CCE_DB_QDRANT_URL="http://localhost:6333"
export CCE_DB_QDRANT_API_KEY="your-qdrant-key"

# SQLite
export CCE_DB_SQLITE_PATH="metadata.db"
export CCE_DB_SQLITE_SYNC="NORMAL"
export CCE_DB_SQLITE_CACHE_SIZE="-64000"
```

#### 嵌入模型配置（多提供者）
```bash
# OpenAI
export CCE_EMB_API_KEY_OPENAI="sk-xxx"

# Google Gemini
export CCE_EMB_API_KEY_GEMINI="AIzaSy..."

# Azure OpenAI
export CCE_EMB_API_KEY_AZURE="azure-key"
```

#### LLM 配置（多提供者）
```bash
# OpenAI GPT
export CCE_LLM_API_KEY_OPENAI="sk-xxx"

# Anthropic Claude
export CCE_LLM_API_KEY_ANTHROPIC="sk-ant-..."

# Ollama Local（通常不需要密钥）
# export CCE_LLM_API_KEY_OLLAMA=""
```

#### 日志配置
```bash
export CCE_LOG_LEVEL="info"        # trace, debug, info, warn, error
export CCE_LOG_OUTPUT="stdout"     # stdout, stderr, file
export CCE_LOG_FORMAT="pretty"     # pretty, compact, json
export CCE_LOG_FILE="/var/log/cce/app.log"  # 当 CCE_LOG_OUTPUT=file 时
```

### 在配置文件中使用环境变量占位符

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

系统会在启动时自动解析这些占位符。

### 从文件加载 API 密钥（适合 Docker Secrets）

```toml
[embedder.providers.openai]
api_key_file = "/run/secrets/embedder_api_key"

[[llm.providers]]
id = "openai-gpt4"
api_key_file = "/run/secrets/llm_api_key"
```

这适用于 Docker Secrets、Kubernetes Secrets 或其他密钥管理系统。

## 配置验证与依赖解析

### 自动验证

系统会在启动时自动验证配置，检查：

1. **依赖关系**: 如果启用了某个功能，其依赖的功能也会自动启用
2. **值范围**: 配置值必须在有效范围内
3. **必填字段**: 必须提供必要的配置项
4. **环境变量**: 验证所有必需的 API 密钥环境变量已设置

### 常见配置警告

| 警告 | 原因 | 解决方案 |
|------|------|----------|
| `store_vectors=true` 但 `qdrant.enabled=false` | 向量存储未启用 | 设置 `database.qdrant.enabled = true` |
| `store_bm25=true` 但 `bm25.enabled=false` | BM25 索引未启用 | 设置 `database.bm25.enabled = true` |
| `llm.enabled=true` 但无提供者配置 | LLM 配置不完整 | 至少配置一个 LLM 提供者 |
| `include_summary=true` 但 `store_summaries=false` | 摘要未存储 | 自动启用 `orchestrator.indexer.store_summaries` |
| `enable_relation_enhancement=true` 但 `build_relations=false` | 关系未构建 | 自动启用 `orchestrator.indexer.build_relations` |
| `logger.output=file` 但 `file` 为空 | 日志文件路径未指定 | 设置 `logger.file` |

### 自动依赖解析

系统会自动解析配置依赖并启用所需功能：

```toml
# 用户配置
[export]
include_summary = true
enable_relation_enhancement = true

# 系统自动启用以下依赖
[orchestrator.indexer]
store_summaries = true      # 自动启用
build_relations = true      # 自动启用

[relation.index]
enabled = true              # 自动启用
```

日志会显示自动启用的功能：
```
INFO Auto-enabled orchestrator.indexer.store_summaries (required by export.include_summary)
INFO Auto-enabled orchestrator.indexer.build_relations (required by export.enable_relation_enhancement)
INFO Auto-enabled relation.index.enabled (required by orchestrator.indexer.build_relations)
```

## 配置类型详解

详细的配置类型说明请参考各参考文档，以下是快速索引：

### 日志级别

```toml
[logger]
level = "info"  # trace, debug, info, warn, error
output = "stdout"  # stdout, stderr, file
format = "pretty"  # pretty, compact, json
file = "/var/log/cce/app.log"  # 当 output="file" 时必需
```

详见：[全局配置参考 - 日志配置](./global-config-reference.md#日志配置)

### SQLite 同步模式

```toml
[database.sqlite]
synchronous = "NORMAL"  # OFF, NORMAL, FULL, EXTRA
temp_store = "MEMORY"   # DEFAULT, FILE, MEMORY, NONE
cache_size = -64000     # KB（负数）或页数（正数）
mmap_size = 268435456   # 字节，0 表示禁用
```

**推荐设置**（WAL 模式下）:
- `synchronous = "NORMAL"` - 平衡安全性和性能
- `temp_store = "MEMORY"` - 更好的性能
- `cache_size = -64000` - 64MB 缓存
- `mmap_size = 268435456` - 256MB 内存映射

详见：[全局配置参考 - SQLite 配置](./global-config-reference.md#sqlite-元数据数据库)

### 距离度量

```toml
[database.qdrant]
distance_metric = "cosine"  # cosine, euclid, dot
```

**选择指南**:
- **Cosine**: 推荐用于大多数场景，对向量长度不敏感
- **Euclid**: 欧几里得距离，适合需要绝对距离的场景
- **Dot**: 点积，适合归一化向量

详见：[全局配置参考 - Qdrant 配置](./global-config-reference.md#qdrant-向量数据库)

### 集合预设

```toml
[database.qdrant]
preset = "medium"  # tiny, small, medium, large
```

| 预设 | 向量数量 | HNSW | 适用场景 |
|------|---------|------|----------|
| tiny | ≤ 2,000 | 禁用 | 测试/小型项目 |
| small | 2,000 - 10,000 | 启用 | 小型项目 |
| medium | 10,000 - 100,000 | 启用 | 中型项目（默认） |
| large | > 100,000 | 启用（优化） | 大型项目 |

详见：[全局配置参考 - 集合预设](./global-config-reference.md#集合预设说明)

### BM25 算法参数

```toml
[database.bm25.algorithm]
k1 = 1.8  # 词频饱和参数（1.0-2.0）
b = 0.4   # 文档长度归一化（0.0-1.0）
```

**代码搜索优化**:
- `k1 = 1.8` - 较高的值强调精确标识符匹配
- `b = 0.4` - 较低的值减少对短代码实体的惩罚

详见：[全局配置参考 - BM25 配置](./global-config-reference.md#bm25-全文检索)

## 项目级配置覆盖

### 可覆盖的配置

项目配置文件 `.cce/config.toml` 可以覆盖以下配置：

- ✅ `scanner` - 文件扫描配置
- ✅ `grouper` - 实体分组配置
- ✅ `orchestrator` - 编排器配置
- ✅ `relation` - 关系索引配置
- ✅ `ast_to_nl` - AST 转自然语言配置
- ✅ `summary` - 摘要生成配置
- ✅ `embedder.model` - 嵌入模型选择（仅限模型名称和预处理）

### 不可覆盖的配置

以下配置**不能**在项目级覆盖（必须在全局配置中设置）：

- ❌ `server` - 服务器配置
- ❌ `database` - 数据库配置
- ❌ `embedder.providers` - 嵌入模型提供者（base_url, api_keys）
- ❌ `llm` - LLM 配置
- ❌ `logger` - 日志配置
- ❌ `export` - 导出配置

### 设计原则

1. **敏感信息保留在全局**: API 密钥、URL 等敏感设置不应在项目配置中重复
2. **模型维度不可更改**: 不同模型产生不兼容的向量，项目只能选择模型，不能更改维度
3. **项目配置聚焦索引**: 项目配置主要关注扫描、索引、分组等与项目特性相关的设置

### 示例

`.cce/config.toml`:
```toml
name = "my-web-project"

[embedder]
model = "bge-m3"  # 使用不同的模型

[scanner]
respect_gitignore = true
exclude_patterns = ["node_modules", "dist", ".next"]

[grouper]
max_nesting_depth = 4
enable_pattern_detection = true

[orchestrator.batch]
batch_size = 100
max_concurrent_batches = 4

[relation.index]
enabled = true
build_call_graph = true
build_dependency_graph = false
```

详见：[项目级配置参考](./project-config-reference.md)

## 配置预设

代码中提供了多种配置预设，可以直接使用或在 TOML 中手动配置等效值。

### 批处理配置预设

#### Rust 代码中使用

```rust
// 小项目 (< 100 文件)
let batch_config = BatchConfig::small_project();

// 大项目 (> 10,000 文件)
let batch_config = BatchConfig::large_project();

// 低内存环境
let batch_config = BatchConfig::low_memory();
```

#### TOML 等效配置

**小项目预设**:
```toml
[orchestrator.batch]
batch_size = 50
max_concurrent_batches = 2
timeout_secs = 120
```

**大项目预设**:
```toml
[orchestrator.batch]
batch_size = 200
max_concurrent_batches = 8
timeout_secs = 600
```

**低内存预设**:
```toml
[orchestrator.batch]
batch_size = 20
max_concurrent_batches = 1
timeout_secs = 180
```

详见：[项目级配置参考 - 批处理配置](./project-config-reference.md#批处理配置预设)

### Grouper 配置预设

#### Rust 代码中使用

```rust
// 小代码库 (< 1,000 文件)
let grouper_config = NestProcessorConfig::small_codebase();

// 大代码库 (> 10,000 文件)
let grouper_config = NestProcessorConfig::large_codebase();

// 模式检测优化
let grouper_config = NestProcessorConfig::pattern_optimized();

// 测试文件优化
let grouper_config = NestProcessorConfig::test_optimized();
```

#### TOML 等效配置

**小代码库预设**:
```toml
[grouper]
max_nesting_depth = 3
min_entity_length = 5
max_entity_length = 5000
enable_pattern_detection = false
parallel_processing = false
```

**大代码库预设**:
```toml
[grouper]
max_nesting_depth = 7
min_entity_length = 15
max_entity_length = 20000
enable_pattern_detection = true
pattern_confidence_threshold = 0.8
parallel_processing = true
max_workers = 8
```

详见：[项目级配置参考 - 实体分组配置](./project-config-reference.md#实体分组配置)

## 相关资源

### 配置文档

- [全局配置参考](./global-config-reference.md) - 核心服务配置完整说明
- [项目级配置参考](./project-config-reference.md) - 项目特定配置指南
- [环境变量配置参考](./env-config-reference.md) - 环境变量和敏感信息管理
- [配置示例文件](../../config.example.toml) - 完整配置示例
- [最小配置文件](../../config.minimal.toml) - 最小必要配置

### 架构文档

- [配置加载机制](../architecture/config-loading.md) - 配置加载流程详解
- [配置合并策略](../architecture/config-merging.md) - 多层配置合并规则
- [配置验证与依赖解析](../architecture/config-validation.md) - 验证和自动依赖解析
- [环境变量重构](../config/env-refactor.md) - 从单提供者到多提供者的迁移

### 最佳实践

- [Docker 部署配置](../deployment/docker.md) - Docker 环境下的配置管理
- [生产环境配置](../deployment/production.md) - 生产环境推荐配置
- [性能调优指南](../performance/tuning.md) - 根据硬件和项目规模优化配置

## 常见问题

### Q1: 如何为不同项目使用不同的嵌入模型？

**A**: 在每个项目的 `.cce/config.toml` 中指定不同的模型：

```toml
# 项目 A
[embedder]
model = "text-embedding-3-small"

# 项目 B
[embedder]
model = "bge-m3"
```

确保在全局配置的 `[embedder.models]` 中定义了这些模型。

### Q2: 如何在开发环境和生产环境使用不同配置？

**A**: 使用不同的 `.env` 文件：

```bash
# 开发环境
.env.development:
CCE_SERVER_HOST=localhost
CCE_LOG_LEVEL=debug

# 生产环境
.env.production:
CCE_SERVER_HOST=0.0.0.0
CCE_LOG_LEVEL=warn
```

启动时指定：
```bash
# 开发环境
source .env.development && cargo run

# 生产环境
source .env.production && ./target/release/code-context-engine
```

### Q3: 配置修改后需要重启服务吗？

**A**: 
- **全局配置**（server, database, embedder, llm）: 需要重启服务
- **项目配置**（scanner, grouper, orchestrator）: 重新索引项目时生效
- **热更新配置**: 修改后立即生效（针对文件监控）

### Q4: 如何验证配置是否正确？

**A**: 启动服务时会显示配置验证结果：

```bash
cargo run

# 查看输出中的验证信息
INFO Configuration loaded successfully
WARN Auto-enabled database.qdrant.enabled (required by indexer.store_vectors)
INFO All configuration dependencies resolved
```

如果有错误，服务会启动失败并显示详细错误信息。

### Q5: 如何安全地管理 API 密钥？

**A**: 
1. **不要**将 API 密钥硬编码在配置文件中
2. **使用**环境变量或 `.env` 文件
3. **添加** `.env` 到 `.gitignore`
4. **提供** `.env.example` 模板（不含真实密钥）
5. **考虑**使用 Docker Secrets 或 Kubernetes Secrets

详见：[环境变量配置参考 - 最佳实践](./env-config-reference.md#最佳实践)
