# 插件系统

CCE 支持通过插件扩展核心功能，包括 Lua 脚本插件和原生动态库插件。

## 插件类型

### Lua 插件

- 使用 `mlua` 加载 Lua 脚本
- 支持超时和内存限制保护
- 适合快速开发和轻量级扩展

### Native 插件

- 使用 `libloading` 加载动态库（.so/.dll/.dylib）
- 支持生命周期管理（create/destroy）
- 适合高性能和复杂逻辑扩展

## 插件注册

插件通过 `.cce/plugins.json` 配置文件注册：

```json
{
  "plugins": [
    {
      "id": "my-plugin",
      "path": "./plugins/my-plugin.lua",
      "type": "lua",
      "enabled": true,
      "file_patterns": ["*.py"],
      "languages": ["python"],
      "capabilities": ["text_gen"],
      "priority": 10
    }
  ]
}
```

### 配置字段

| 字段 | 说明 |
|------|------|
| `id` | 插件唯一标识 |
| `path` | 插件文件路径（相对于 `.cce/`） |
| `type` | 插件类型：`lua` 或 `native` |
| `enabled` | 是否启用 |
| `file_patterns` | 文件匹配模式（glob） |
| `languages` | 支持的编程语言 |
| `capabilities` | 声明的能力列表（空=运行时探测） |
| `priority` | 优先级（越高越先执行） |

## 扩展点

### 索引管道扩展点

| 扩展点 | 说明 | 类型 |
|--------|------|------|
| `TextGen` | 生成 BM25/Embedding 文本 | Override |
| `FormatParse` | 解析文档格式 | Override |
| `EntityExtract` | 提取补充实体 | Additive |
| `AstLanguage` | 自定义 tree-sitter 语言 | Additive (Native) |
| `LanguageRemap` | 语言重映射到宿主语法 | Additive |
| `LangHeuristics` | stdlib/test-file/entity-kind 分类 | Override |
| `SymbolExtract` | 提取导入/导出符号 | Override |
| `Group` | 分组后处理钩子 | Chain |
| `GroupOverride` | 完全覆盖分组逻辑 | Override |
| `Chunk` | 覆盖分块逻辑 | Override |
| `RelationExtract` | 提取符号/关系 | Additive |
| `FileFilter` | 文件包含/排除决策 | Override |

### 查询管道扩展点

| 扩展点 | 说明 | 类型 |
|--------|------|------|
| `QueryRewrite` | 查询重写/扩展 | Chain |
| `Fusion` | 混合融合权重覆盖 | Override |
| `Rerank` | 查询结果重排序 | Override |
| `ResultFilter` | 结果过滤/增强 | Chain |

## 执行模式

### Override 模式

第一个返回非 `None` 结果的插件生效，后续插件不再执行。

### Chain 模式

按优先级顺序执行所有插件，前一个输出作为后一个输入。

### Additive 模式

所有插件执行，结果合并。

## Lua 插件示例

```lua
plugin = {
    id = "my-textgen",
    name = "My TextGen Plugin",
    version = "0.1.0",
    priority = 10,
    capabilities = {"text_gen"},
    languages = {"python"},
}

function plugin.generate_bm25(group)
    -- group 包含: group_id, header, members, language 等
    return "自定义 BM25 文本"
end

function plugin.generate_embedding(group)
    return "自定义 Embedding 文本"
end
```

### Lua 插件可用函数

| 函数 | 对应扩展点 | 参数 |
|------|-----------|------|
| `generate_bm25(group)` | TextGen | EntityGroup |
| `generate_embedding(group)` | TextGen | EntityGroup |
| `parse_document(content, path)` | FormatParse | 文件内容、路径 |
| `extract_entities(content, path, lang)` | EntityExtract | 内容、路径、语言 |
| `group(context)` | GroupOverride | GroupPluginContext |
| `post_group(groups, context)` | Group | 分组列表、上下文 |
| `chunk(conversions, path)` | Chunk | 转换结果、路径 |
| `rerank(query, candidates)` | Rerank | 查询、候选结果 |
| `extract_symbols(content, path, lang)` | RelationExtract | 内容、路径、语言 |
| `extract_imports(content, path, lang)` | SymbolImport | 内容、路径、语言 |
| `extract_exports(content, path, lang)` | SymbolExport | 内容、路径、语言 |
| `rewrite_query(query)` | QueryRewrite | 查询字符串 |
| `fusion_weights(query, vec_count, bm25_count)` | Fusion | 查询、向量数、BM25数 |
| `filter_results(query, results)` | ResultFilter | 查询、结果列表 |
| `filter_file(path, is_dir, size)` | FileFilter | 路径、是否目录、大小 |
| `classify_stdlib(module_path)` | LangHeuristics | 模块路径 |
| `is_test_file(path, content)` | LangHeuristics | 路径、内容 |
| `entity_kind(capture_name)` | LangHeuristics | 查询捕获名 |

## Native 插件开发

### ABI 要求

Native 插件必须导出以下符号：

```c
// 必需符号
uint32_t cce_plugin_abi_version();
char* cce_plugin_metadata();  // 返回 JSON 元数据
bool cce_plugin_has_bm25_generation();
bool cce_plugin_has_embedding_generation();
bool cce_plugin_has_lifecycle();
void cce_plugin_free_string(char* ptr);

// 可选符号（按需导出）
char* cce_plugin_generate_bm25(void* ctx, const char* group_json);
char* cce_plugin_generate_embedding(void* ctx, const char* group_json);
// ... 其他扩展点函数
```

### 使用 declare_plugin! 宏

```rust
use cce_plugin_sdk::{declare_plugin, FfiPlugin, PluginMetadata};

struct MyPlugin;

impl FfiPlugin for MyPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "my-native-plugin".into(),
            name: "My Native Plugin".into(),
            version: "0.1.0".into(),
            priority: 10,
            description: Some("Custom plugin".into()),
            ..Default::default()
        }
    }
    
    fn supports_bm25(&self) -> bool { true }
    
    fn generate_bm25(&self, group: &EntityGroup) -> Result<Option<String>, PluginError> {
        // 实现逻辑
        Ok(Some("generated text".to_string()))
    }
}

declare_plugin!(MyPlugin);
```

### Cargo.toml 配置

```toml
[package]
name = "my-cce-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
cce-plugin-sdk = "0.1"
```

## 优先级机制

- **正优先级**: 在内置实现之前执行（override tier）
- **零优先级**: 默认，与内置实现竞争
- **负优先级**: 仅当内置实现返回空时执行（fallback tier）

### 能力级优先级

可为单个能力设置独立优先级：

```json
{
  "capability_priorities": {
    "text_gen": 100,
    "fusion": -1
  }
}
```

## 文件模式过滤

使用 glob 模式匹配文件：

```json
{
  "file_patterns": ["*.py", "src/**/*.rs", "test_*.js"]
}
```

支持的模式：
- `*` - 匹配任意字符
- `?` - 匹配单个字符
- `[abc]` - 匹配字符集
- `**` - 匹配任意目录层级

## 调试

### 日志

插件执行日志通过 `tracing` 输出，启用 DEBUG 级别查看详细信息：

```bash
RUST_LOG=debug cargo run
```

### 错误处理

插件错误不会中断主流程，会降级到内置实现。常见错误：
- `ScriptError`: Lua 脚本语法/运行时错误
- `Timeout`: 执行超时（默认 5 秒）
- `MemoryLimit`: 内存超限（默认 64MB）
- `InvalidOutput`: 返回数据格式错误
