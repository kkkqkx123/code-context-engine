# 项目级配置参考

## 概述

项目级配置文件允许为每个项目自定义索引、扫描和处理选项，而无需修改全局配置。敏感设置（API 密钥、URL）保留在全局配置中，项目配置仅覆盖与项目特性相关的设置。

**配置文件位置**: `<project>/.cce/config.toml`

**本地覆盖文件**: `<project>/.cce/config.local.toml`（不提交到版本控制）

## 配置优先级

```
环境变量 > .env 文件 > config.local.toml > config.toml > 全局默认值
```

## 可覆盖的配置项

项目配置可以覆盖以下模块的设置：

- ✅ `scanner` - 文件扫描配置
- ✅ `grouper` - 实体分组配置
- ✅ `orchestrator` - 编排器配置
- ✅ `relation` - 关系索引配置
- ✅ `ast_to_nl` - AST 转自然语言配置
- ✅ `summary` - 摘要生成配置
- ✅ `embedder.model` - 嵌入模型选择（仅限模型名称和预处理）

**不可覆盖的配置**（必须在全局配置中设置）:

- ❌ `server` - 服务器配置
- ❌ `database` - 数据库配置
- ❌ `embedder.providers` - 嵌入模型提供者（base_url, api_keys）
- ❌ `llm` - LLM 配置
- ❌ `logger` - 日志配置
- ❌ `export` - 导出配置

## 基础配置

```toml
# 项目名称（可选，用于显示）
name = "my-project"

# 项目根路径（可选，默认为配置文件所在目录）
root_path = "."
```

## 嵌入模型配置

项目可以选择使用哪个嵌入模型，但不能更改模型的连接信息（这些在全局配置中定义）。

```toml
[embedder]
model = "bge-m3"                    # 模型名称（引用全局配置中的模型定义）

# 可选：覆盖预处理器
[embedder.preprocessor]
prefix = "Represent this code for retrieval: "  # 文本前缀
template = "{{text}}"                             # 文本模板
```

### 设计原则

- **模型维度不可覆盖**: 不同模型产生不兼容的向量，在项目级别更改维度会导致索引损坏
- **提供者由模型定义决定**: 模型在全局配置中已关联到特定提供者，项目只需引用模型名称
- **API 密钥保留在全局**: 敏感信息不应在项目配置中重复

## 文件扫描配置

```toml
[scanner]
follow_symlinks = false             # 是否跟随符号链接
respect_gitignore = true            # 是否尊重 .gitignore 规则
exclude_patterns = [                # 排除模式
    "node_modules",
    "dist",
    ".git",
    "target",
    "*.log"
]
include_patterns = []               # 包含模式（空表示包含所有非排除文件）
gitignore_patterns = []             # 额外的 gitignore 模式
binary_check_size = 8192            # 二进制文件检查大小（字节）
max_hash_file_size = 10485760       # 最大哈希文件大小（字节，默认 10MB）
default_max_content_size = 1048576  # 默认最大内容大小（字节，默认 1MB）
max_file_size = 512000              # 最大文件大小（字节，默认 500KB）
```

### 扫描策略建议

#### Web 前端项目

```toml
[scanner]
respect_gitignore = true
exclude_patterns = [
    "node_modules",
    "dist",
    "build",
    ".next",
    "coverage",
    "*.min.js",
    "*.map"
]
```

#### Rust 项目

```toml
[scanner]
respect_gitignore = true
exclude_patterns = [
    "target",
    ".git",
    "*.lock"
]
```

#### Python 项目

```toml
[scanner]
respect_gitignore = true
exclude_patterns = [
    "__pycache__",
    "*.pyc",
    ".venv",
    "venv",
    ".pytest_cache",
    ".mypy_cache"
]
```

## 实体分组配置

Grouper 负责将 AST 解析出的实体进行智能分组和嵌套处理。

```toml
[grouper]
# 基本配置
max_nesting_depth = 5               # 最大嵌套深度
min_entity_length = 10              # 最小实体长度（字符）
max_entity_length = 10000           # 最大实体长度（字符）

# 模式检测
enable_pattern_detection = true     # 启用模式检测
pattern_confidence_threshold = 0.7  # 模式置信度阈值

# 性能优化
parallel_processing = true          # 启用并行处理
max_workers = 4                     # 最大工作线程数

# 测试文件优化
test_file_patterns = [              # 测试文件匹配模式
    "*_test.*",
    "*.test.*",
    "*.spec.*",
    "test_*"
]
test_optimization_enabled = true    # 启用测试文件优化
```

### 预设配置

代码中提供了多种预设配置，可以直接使用：

```rust
// 小代码库 (< 1000 文件)
NestProcessorConfig::small_codebase()

// 大代码库 (> 10000 文件)
NestProcessorConfig::large_codebase()

// 模式检测优化
NestProcessorConfig::pattern_optimized()

// 测试文件优化
NestProcessorConfig::test_optimized()
```

### TOML 等效配置

#### 小代码库预设

```toml
[grouper]
max_nesting_depth = 3
min_entity_length = 5
max_entity_length = 5000
enable_pattern_detection = false
parallel_processing = false
```

#### 大代码库预设

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

## 编排器配置

Orchestrator 管理索引构建、批处理和热更新流程。

```toml
[orchestrator]
# 批处理配置
[orchestrator.batch]
batch_size = 100                    # 批处理大小
max_concurrent_batches = 4          # 最大并发批次数
timeout_secs = 300                  # 批处理超时时间（秒）
retry_on_failure = true             # 失败时重试
max_retries = 3                     # 最大重试次数

# 热更新配置
[orchestrator.hot_update]
enabled = true                      # 启用热更新
debounce_ms = 1000                  # 去抖时间（毫秒）
watch_delay_ms = 500                # 监控延迟（毫秒）
max_queue_size = 1000               # 最大队列大小
batch_process_interval_ms = 2000    # 批量处理间隔（毫秒）

# 文件监控配置
[orchestrator.hot_update.watch]
recursive = true                    # 递归监控
follow_symlinks = false             # 跟随符号链接
poll_interval_ms = 1000             # 轮询间隔（毫秒，仅在非原生监控时使用）

# 索引器配置
[orchestrator.indexer]
store_vectors = true                # 存储向量到 Qdrant
store_bm25 = true                   # 存储 BM25 索引
store_summaries = false             # 存储摘要（需要 LLM）
build_relations = true              # 构建关系索引
parse_doc_comments = true           # 解析文档注释
max_file_parse_time_secs = 30       # 单文件最大解析时间（秒）

# 缓存配置
[orchestrator.cache]
enabled = true                      # 启用缓存
persistent = true                   # 启用持久化缓存
cache_dir = ".cce/cache"            # 缓存目录
max_cache_size_mb = 1024            # 最大缓存大小（MB）
cache_ttl_hours = 24                # 缓存生存时间（小时）

[orchestrator.cache.persistent]
compression = "zstd"                # 压缩算法: none, zstd, lz4
compression_level = 3               # 压缩级别（1-22，仅适用于 zstd）
```

### 批处理配置预设

#### 小项目预设 (< 100 文件)

```toml
[orchestrator.batch]
batch_size = 50
max_concurrent_batches = 2
timeout_secs = 120
```

#### 大项目预设 (> 10,000 文件)

```toml
[orchestrator.batch]
batch_size = 200
max_concurrent_batches = 8
timeout_secs = 600
```

#### 低内存环境预设

```toml
[orchestrator.batch]
batch_size = 20
max_concurrent_batches = 1
timeout_secs = 180
```

### 热更新调优

#### 快速响应模式（开发环境）

```toml
[orchestrator.hot_update]
debounce_ms = 500
watch_delay_ms = 200
batch_process_interval_ms = 1000
```

#### 稳定模式（生产环境）

```toml
[orchestrator.hot_update]
debounce_ms = 2000
watch_delay_ms = 1000
batch_process_interval_ms = 5000
```

## 关系索引配置

关系索引用于提取函数调用链、依赖关系等代码结构信息。

```toml
[relation]
# 调用链查询最大深度
max_call_depth = 10
# 是否提取 import/export 信息
analyze_imports = true
# 是否构建跨文件依赖边并在热更新中传播
track_cross_file_deps = true
[relation.index]
enabled = true                      # 启用关系索引
max_relations_per_file = 10000      # 单个文件最多保留的关系数（跨文件关系保底 1/4 配额，至少 1 条）
resolve_call_chains = true          # 启用调用链与路径查询
filter_stdlib_calls = true          # 不保留标准库关系
```

### 关系索引最佳实践

> **语义边界**：当 `track_cross_file_deps = false` 或依赖传播深度受限（`max_depth`）时，
> 热更新不会重解析全部依赖文件，调用链/调用者查询可能缺失跨文件边（幽灵调用者不会被错误返回，
> 但真实跨文件边可能不在结果中）。该配置通过查询能力信息暴露
> （`IndexCapabilities` 的 `relation_propagation` 标记），消费方可据此提示查询结果范围。

#### 仅构建调用图（轻量级）

```toml
[relation.index]
enabled = true
max_relations_per_file = 5000
resolve_call_chains = true
filter_stdlib_calls = true
```

#### 完整关系索引（重量级）

```toml
[relation.index]
enabled = true
build_call_graph = true
build_dependency_graph = true
max_depth = 15

[relation.indexer]
parallel_build = true
max_workers = 8
```

## AST 转自然语言配置

控制如何将 AST 实体转换为自然语言描述。

```toml
[ast_to_nl]
# 分块配置
[ast_to_nl.chunking]
enabled = true                      # 启用分块
max_chunk_size = 512                # 最大分块大小（token）
chunk_overlap = 50                  # 分块重叠（token）
split_strategy = "semantic"         # 分片策略: semantic, fixed, hybrid

# 转换配置
[ast_to_nl.conversion]
include_signatures = true           # 包含函数签名
include_docstrings = true           # 包含文档字符串
include_type_info = true            # 包含类型信息
simplify_generics = true            # 简化泛型表示
max_parameters = 10                 # 最大参数数量（超过则截断）

# 语言特定配置
[ast_to_nl.languages.rust]
include_trait_impls = true          # 包含 trait 实现
include_derive_macros = true        # 包含 derive 宏

[ast_to_nl.languages.typescript]
include_interfaces = true           # 包含接口定义
include_type_aliases = true         # 包含类型别名
include_jsdoc = true                # 包含 JSDoc 注释

[ast_to_nl.languages.python]
include_decorators = true           # 包含装饰器
include_type_hints = true           # 包含类型提示
include_docstrings = true           # 包含 docstring
```

### 分块策略说明

| 策略 | 说明 | 适用场景 |
|------|------|---------|
| semantic | 基于语义边界分块 | 通用代码，保持上下文完整性 |
| fixed | 固定大小分块 | 需要精确控制分块大小 |
| hybrid | 混合策略 | 结合语义和固定大小 |

## 摘要生成配置

控制如何生成代码实体的摘要。当前只保留策略和输出限制，不再拆分为 `local` / `llm` / `hybrid` 子配置。

```toml
[summary]
# 生成策略: auto, rule_based, model_enhanced, minimal
strategy = "auto"
# 最大摘要长度（tokens）
max_summary_length = 500
# 摘要中最多抽取的实体数
max_entities = 10
# 摘要中最多保留的导入数
max_imports = 10
# 模型摘要请求的最大并发数
max_concurrent = 5
```

### 策略说明

| 策略 | 说明 | 适用场景 |
|------|------|---------|
| auto | 自动选择合适的摘要生成方式 | 默认推荐 |
| rule_based | 仅使用规则生成摘要 | 无 LLM 或需要稳定输出 |
| model_enhanced | 使用 LLM 增强摘要 | 需要更高质量摘要 |
| minimal | 只生成最基础摘要 | 资源受限或调试场景 |

## 配置示例

### 示例 1: Web 前端项目

```toml
name = "frontend-app"

[embedder]
model = "text-embedding-3-small"

[scanner]
respect_gitignore = true
exclude_patterns = [
    "node_modules",
    "dist",
    "build",
    ".next",
    "coverage"
]

[grouper]
max_nesting_depth = 4
enable_pattern_detection = true

[orchestrator.batch]
batch_size = 100
max_concurrent_batches = 4

[orchestrator.hot_update]
enabled = true
debounce_ms = 500

[relation.index]
enabled = true
build_call_graph = true
build_dependency_graph = false
```

### 示例 2: Rust 后端服务

```toml
name = "backend-service"

[embedder]
model = "bge-m3"

[scanner]
respect_gitignore = true
exclude_patterns = ["target", ".git"]

[grouper]
max_nesting_depth = 6
parallel_processing = true
max_workers = 8

[orchestrator.batch]
batch_size = 150
max_concurrent_batches = 6

[orchestrator.indexer]
store_vectors = true
store_bm25 = true
build_relations = true

[relation.index]
enabled = true
build_call_graph = true
build_dependency_graph = true
max_depth = 10

[summary]
strategy = "model_enhanced"
max_summary_length = 800
max_entities = 10
max_imports = 10

[llm]
chat_model = "gpt-4o"
```

### 示例 3: 大型 monorepo

```toml
name = "monorepo"

[scanner]
respect_gitignore = true
exclude_patterns = [
    "node_modules",
    "dist",
    "target",
    ".git",
    "*.log",
    "*.tmp"
]

[grouper]
max_nesting_depth = 5
enable_pattern_detection = true
pattern_confidence_threshold = 0.8
parallel_processing = true
max_workers = 12

[orchestrator.batch]
batch_size = 200
max_concurrent_batches = 8
timeout_secs = 600

[orchestrator.hot_update]
enabled = true
debounce_ms = 2000
batch_process_interval_ms = 5000

[orchestrator.cache]
enabled = true
persistent = true
max_cache_size_mb = 2048
cache_ttl_hours = 48

[orchestrator.indexer]
store_vectors = true
store_bm25 = true
store_summaries = false
build_relations = false  # 大型项目禁用关系索引以提高性能

[relation.index]
enabled = false  # 按需启用
```

## 本地覆盖配置

使用 `config.local.toml` 覆盖个人偏好设置，该文件不应提交到版本控制。

`.gitignore`:
```
.cce/config.local.toml
```

`config.local.toml` 示例:
```toml
# 个人调试设置
[orchestrator.hot_update]
debounce_ms = 200  # 更快的热更新响应

[logger]
level = "debug"    # 更详细的日志
```

## 最佳实践

### 1. 根据项目规模调整批处理

```toml
# 小型项目 (< 1000 文件)
[orchestrator.batch]
batch_size = 50
max_concurrent_batches = 2

# 中型项目 (1000 - 10000 文件)
[orchestrator.batch]
batch_size = 100
max_concurrent_batches = 4

# 大型项目 (> 10000 文件)
[orchestrator.batch]
batch_size = 200
max_concurrent_batches = 8
```

### 2. 优化热更新性能

```toml
# 开发环境：快速响应
[orchestrator.hot_update]
debounce_ms = 500
batch_process_interval_ms = 1000

# 生产环境：稳定性优先
[orchestrator.hot_update]
debounce_ms = 2000
batch_process_interval_ms = 5000
```

### 3. 合理配置缓存

```toml
[orchestrator.cache]
enabled = true
persistent = true
max_cache_size_mb = 1024  # 根据可用内存调整
cache_ttl_hours = 24      # 每天刷新一次
```

### 4. 选择性启用关系索引

```toml
# 小型项目：启用完整关系索引
[relation.index]
enabled = true
build_call_graph = true
build_dependency_graph = true

# 大型项目：仅启用调用图或完全禁用
[relation.index]
enabled = true
build_call_graph = true
build_dependency_graph = false  # 禁用依赖图以提高性能
```

## 相关文档

- [全局配置参考](./global-config-reference.md)
- [环境变量配置参考](./env-config-reference.md)
