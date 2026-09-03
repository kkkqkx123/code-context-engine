# CCE Native Plugin SDK 使用指南

## 概述

`cce-plugin-sdk` 是 [Code Context Engine (CCE)](https://github.com/atomgit/code-context-engine) 的原生插件开发工具包。它提供了以下能力：

- **`FfiPlugin` trait** — 插件开发者只需实现这个 trait
- **`declare_plugin!` 宏** — 自动生成所有 `extern "C"` 导出函数
- **FFI 协议封装** — JSON-over-C-ABI 序列化/反序列化全程自动处理
- **panic 防护** — 插件内 panic 会被捕获并转为 FFI 错误结果，不会跨 ABI 展开

使用本 SDK 开发的插件编译为 `.so`（Linux）/ `.dylib`（macOS）/ `.dll`（Windows）动态库，可供 CCE 宿主加载。

> **ABI 定义**：导出的 C ABI 由 `cce-plugin-sdk/include/cce_plugin.h` 权威定义。
> 使用其他语言（C/C++/Zig 等）编写的插件可以直接对照头文件实现，无需本 SDK。
>
> 宿主侧的插件加载器位于 `crates/app/cce-plugin-runtime/src/native.rs`（Lua 与原生插件运行时均在 `cce-plugin-runtime` crate，而非 `cce_infrastructure`）。修改 ABI 时必须同步更新三处：`include/cce_plugin.h`、`cce-plugin-sdk/src/lib.rs`（宏与助手）、`crates/app/cce-plugin-runtime/src/native.rs`（宿主加载器）。

---

## 快速开始

### 1. 创建插件项目

```bash
cargo new --lib my-cce-plugin
cd my-cce-plugin
```

### 2. 修改 `Cargo.toml`

```toml
[package]
name = "my-cce-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]    # ← 关键：编译为动态库

[dependencies]
cce-plugin-sdk = "0.1"
```

### 3. 实现插件逻辑

```rust
// src/lib.rs
use cce_plugin_sdk::{declare_plugin, FfiPlugin, PluginError, PluginMetadata};

struct MyPlugin;

impl FfiPlugin for MyPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "my-org/flask-templater".into(),
            name: "Flask Templater".into(),
            version: "0.1.0".into(),
            priority: 10,
            description: Some("Generates descriptions for Flask route handlers".into()),
        }
    }

    fn supports_bm25(&self) -> bool { true }

    fn generate_bm25(&self, _ctx: *mut std::ffi::c_void, group_json: &str)
        -> Result<Option<String>, PluginError> {
        let v: serde_json::Value = serde_json::from_str(group_json)
            .map_err(|e| PluginError::InvalidOutput(format!("Invalid group JSON: {}", e)))?;
        let name = v["name"].as_str().unwrap_or("unnamed");
        Ok(Some(format!("Flask route handler function {}.", name)))
    }
}

declare_plugin!(MyPlugin);
```

### 4. 编译

```bash
cargo build --release
```

产物位于 `target/release/`：
- Linux: `libmy_cce_plugin.so`
- macOS: `libmy_cce_plugin.dylib`
- Windows: `my_cce_plugin.dll`

### 5. 注册到 CCE

在 `plugins.json` 中添加：

```json
{
    "plugins": [
        {
            "id": "my-org/flask-templater",
            "path": "path/to/libmy_cce_plugin.so",
            "plugin_type": "native",
            "enabled": true
        }
    ]
}
```

---

## `FfiPlugin` trait 参考

### 必需方法

| 方法 | 返回 | 说明 |
|------|------|------|
| `metadata()` | `PluginMetadata` | 返回插件元数据（id、名称、版本、优先级） |

### 可选方法

| 方法 | 返回 | 默认值 | 需同时覆盖 |
|------|------|--------|-----------|
| `supports_bm25()` | `bool` | `false` | — |
| `supports_embedding()` | `bool` | `false` | — |
| `supports_lifecycle()` | `bool` | `false` | — |
| `create_context()` | `Option<*mut c_void>` | `None` | `supports_lifecycle→true` |
| `destroy_context(ctx)` | — | — | `create_context` 返回非空 |
| `generate_bm25(ctx, group_json)` | `Result<Option<String>, PluginError>` | `Ok(None)` | `supports_bm25→true` |
| `generate_embedding(ctx, group_json)` | `Result<Option<String>, PluginError>` | `Ok(None)` | `supports_embedding→true` |
| `generate_bm25_batch(ctx, groups_json)` | `Result<Vec<Option<String>>, PluginError>` | 逐个调用单条方法 | `supports_bm25→true` |
| `generate_embedding_batch(ctx, groups_json)` | `Result<Vec<Option<String>>, PluginError>` | 逐个调用单条方法 | `supports_embedding→true` |
| `supports_parse()` | `bool` | `false` | — |
| `parse_document(ctx, content, file_path)` | `Result<Option<PluginDocument>, PluginError>` | `Ok(None)` | `supports_parse→true` |
| `supports_extract()` | `bool` | `false` | — |
| `extract_entities(ctx, content, file_path, language)` | `Result<Option<Vec<PluginEntity>>, PluginError>` | `Ok(None)` | `supports_extract→true` |
| `supports_group()` | `bool` | `false` | — |
| `post_group(ctx, groups_json, context_json)` | `Result<Option<Vec<EntityGroup>>, PluginError>` | `Ok(None)` | `supports_group→true` |
| `supports_chunk()` | `bool` | `false` | — |
| `chunk(ctx, conversions_json, file_path)` | `Result<Option<Vec<ChunkedResult>>, PluginError>` | `Ok(None)` | `supports_chunk→true` |
| `supports_rerank()` | `bool` | `false` | — |
| `rerank(ctx, query, candidates_json)` | `Result<Option<RerankResult>, PluginError>` | `Ok(None)` | `supports_rerank→true` |
| `supports_ast_language()` | `bool` | `false` | — |
| `tree_sitter_language()` | `Option<*mut c_void>` | `None` | `supports_ast_language→true` |
| `query_scheme(ctx, query_type)` | `Option<String>` | `None` | `supports_ast_language→true` |
| `language_name()` | `Option<String>` | `None` | `supports_ast_language→true` |
| `language_extensions()` | `Vec<String>` | 空 | `supports_ast_language→true` |
| `supports_language_remap()` | `bool` | `false` | — |
| `remap_grammar_language()` | `Option<String>` | `None` | `supports_language_remap→true` |
| `supports_stdlib_heuristic()` | `bool` | `false` | — |
| `classify_stdlib(ctx, module_path)` | `Option<String>` | `None` | `supports_stdlib_heuristic→true` |
| `supports_test_file_heuristic()` | `bool` | `false` | — |
| `is_test_file(ctx, file_path, content)` | `Option<bool>` | `None` | `supports_test_file_heuristic→true` |
| `supports_entity_kind_heuristic()` | `bool` | `false` | — |
| `entity_kind(ctx, capture_name)` | `Option<String>` | `None` | `supports_entity_kind_heuristic→true` |
| `supports_group_override()` | `bool` | `false` | — |
| `group(ctx, context_json)` | `Result<Option<Vec<EntityGroup>>, PluginError>` | `Ok(None)` | `supports_group_override→true` |
| `supports_relation_extract()` | `bool` | `false` | — |
| `extract_symbols(ctx, content, file_path, language)` | `Result<Option<Vec<PluginSymbol>>, PluginError>` | `Ok(None)` | `supports_relation_extract→true` |
| `extract_relations(ctx, content, file_path, language)` | `Result<Option<Vec<PluginRelation>>, PluginError>` | `Ok(None)` | `supports_relation_extract→true` |
| `supports_symbol_extract()` | `bool` | `false` | — |
| `extract_imports(ctx, content, file_path, language)` | `Result<Option<Vec<PluginImport>>, PluginError>` | `Ok(None)` | `supports_symbol_extract→true` |
| `extract_exports(ctx, content, file_path, language)` | `Result<Option<Vec<PluginExport>>, PluginError>` | `Ok(None)` | `supports_symbol_extract→true` |
| `supports_query_rewrite()` | `bool` | `false` | — |
| `rewrite_query(ctx, query)` | `Result<Option<QueryRewriteResult>, PluginError>` | `Ok(None)` | `supports_query_rewrite→true` |
| `supports_fusion()` | `bool` | `false` | — |
| `fusion_weights(ctx, query, vector_count, bm25_count)` | `Result<Option<FusionWeights>, PluginError>` | `Ok(None)` | `supports_fusion→true` |
| `supports_result_filter()` | `bool` | `false` | — |
| `filter_results(ctx, query, results_json)` | `Result<Option<Vec<ResultFilterEntry>>, PluginError>` | `Ok(None)` | `supports_result_filter→true` |
| `supports_file_filter()` | `bool` | `false` | — |
| `filter_file(ctx, file_path, is_directory, size)` | `Result<Option<FileFilterDecision>, PluginError>` | `Ok(None)` | `supports_file_filter→true` |

> **版本提示**：ABI 版本历史已重置为 1（开发阶段不回退兼容）。NL 生成方法均携带 `ctx` 上下文指针（`create_context` 的返回值，无生命周期时为 null）。所有 SDK 插件默认实现 batch 方法（逐个回退到单条），为吞吐量建议重写为单次批量处理。流水线扩展能力（`TextGen` / `FormatParse` / `EntityExtract` / `Group` / `GroupOverride` / `Chunk` / `Rerank` / `AstLanguage` / `LanguageRemap` / `LangHeuristics` / `RelationExtract` / `SymbolExtract` / `QueryRewrite` / `Fusion` / `ResultFilter` / `FileFilter`）均为同一 ABI 的可选能力。

### 流水线扩展能力

插件可声明多个能力面（capability facet）。宿主按能力面 + `file_patterns` + `languages` + `priority` 路由调用，并复用统一的超时 / 熔断 / panic 隔离机制。详见 `docs/plan/plugin_extension_design.md`。

能力面分三档消费语义（priority 降序，同优先级按声明顺序）：

- **覆盖档**（首个非空结果胜出）：`FormatParse`、`GroupOverride`、`Chunk`、`Rerank`、`SymbolExtract`、`Fusion`、`FileFilter`、`LangHeuristics`、`TextGen`（逐组填充）。
- **链式档**（全部按顺序执行，前一输出为后一输入）：`Group`、`QueryRewrite`、`ResultFilter`。
- **加法档**（结果合并）：`EntityExtract`、`RelationExtract`、`AstLanguage`、`LanguageRemap`。

优先级建议区间：0–99 通用兜底 / 100–999 语言领域特定 / 1000–9999 显式覆盖级。宿主可在 `.cce/plugins.json` 的条目中显式覆盖优先级（`priority` 字段），无需改动插件文件。

- **FormatParse**：解析自定义文档格式（如 `.proto`）。返回 `PluginDocument { title, language, entities }`。
- **EntityExtract**：在代码实体流中补充框架特定实体（如 Flask 路由）。返回 `PluginEntity` 数组；宿主将其注入分组 → 文本生成 → 分块流水线。
- **Group**：内置分组完成后回调，可合并 / 拆分 / 重命名分组。入参为 `EntityGroup` 数组 + `GroupPluginContext { file_path, language, source }`。
- **GroupOverride**：完全替换内置分组。入参为扩展 `GroupPluginContext`（含序列化的实体与原始关系），返回 `EntityGroup` 数组；返回非空数组时跳过内置分组，下游 `post_group` 钩子与 combined-source 生成仍照常执行。
- **Chunk**：完全替换内置分块，返回标准 `ChunkedResult` 形状。入参为 `GroupConversions` 数组 + `file_path`。
- **Rerank**：查询结果重排（全局能力，无文件 / 语言过滤）。入参为查询 + `RerankCandidate` 数组，返回 `RerankResult`（重排顺序 + 分数 + 可选理由）。
- **AstLanguage**：自定义语言解析（仅 Native），提供 tree-sitter grammar 指针 + 查询方案字符串 + 语言名与扩展名。宿主在注册时校验 grammar 的 ABI 版本（`plugins.grammar_abi_policy`，默认 `deny` 拒绝区间外 grammar，可配 `warn`）；缺失 / 空指针 grammar 一律跳过并告警。
- **LanguageRemap**：自定义语言直接复用宿主内置 grammar（Lua 与 Native 均可，无 FFI 指针）。声明 `language_name` / `language_extensions` / `remap_grammar_language`（指向宿主内置语言名），8 类查询方案可选——未提供时回落被引用语言的方案。适用于既有语言超集 / 子集形态的 DSL（模板方言、配置扩展等）。
- **LangHeuristics**：自定义语言启发式，三个钩子相互独立、均可缺省：
  - `classify_stdlib(module_path)` → 标准库分类名（`Collection` / `Io` / …）或 nil，接入实体 stdlib 标记（仅对 `Language::Custom` 生效）；
  - `is_test_file(file_path, content)` → `true` / `false` / nil，内置路径规则无信号时生效，标记整个文件为测试文件；
  - `entity_kind(capture_name)` → 实体 kind 名（`function` / `class` / …）或 nil，内置捕获名映射未命中时生效。
- **RelationExtract**：向关系索引补充框架特定符号与显式关系（如 Spring `@Bean` 注入）。实现 `extract_symbols(...)` / `extract_relations(...)`，返回 `PluginSymbol` / `PluginRelation` 数组；宿主将符号注册进项目符号表、关系经解析器解析为 `SymbolKey → entity_id`（无法解析的目标丢弃并告警）。默认关闭，需 `relation.plugin_symbols_enabled = true`。
- **SymbolExtract**：自定义语言的 import/export 符号提取，配合 `AstLanguage` 使用。实现 `extract_imports(content, file_path, language)` / `extract_exports(...)`，返回 `PluginImport` / `PluginExport` 数组；宿主将其转换为标准化 import/export 进入关系索引（默认关闭，需 `relation.plugin_symbol_extract_enabled = true`）。
- **QueryRewrite**：召回前改写 / 扩展查询（链式档）。返回 `QueryRewriteResult { rewritten_query, expansion_terms }`；原始查询保留为兜底项。
- **Fusion**：覆写混合融合权重（覆盖档，首个非 `None` 生效）。返回 `FusionWeights { vector_weight, bm25_weight, min_score }`，权重由宿主校验至 `[0, 1]`。
- **ResultFilter**：重排后按 id 移除 / 增益 / 标注候选（链式档）。返回 `ResultFilterEntry { id, remove, boost, note }` 数组。
- **FileFilter**：扫描期文件纳入 / 排除决策（覆盖档，首个非 `Neutral` 生效）。返回 `FileFilterDecision`（`include` / `exclude` / `neutral`）；`Neutral` 交由内置 `PatternMatcher`。

跨 FFI 的实体 / 文档 / 分块 / 重排 JSON schema 见 `include/cce_plugin.h` 注释。

### 扩展边界（明确不支持）

- **查询类型固定 8 类**：`entity` / `call` / `control_flow` / `behavior` / `dependency` / `comment` / `embedded` / `structural`。查询类型贯通实体提取、关系构建、摘要生成全链路，不开放自定义；"新查询维度"诉求由 `EntityExtract` / `RelationExtract` 以附加实体 / 关系形式表达。
- **转换器级（AST→NL 算法）不可覆盖**：仅 `TextGen` 可整体替换生成文本；转换流程内部逻辑不可定制。
- **存储层不可插件化**：Qdrant / Tantivy / SQLite 不提供插件接口。

### 返回值约定

- `Ok(Some(value))` — 成功，有返回值
- `Ok(None)` — 成功，无返回值（插件选择跳过该输入，宿主回退到内置转换器）
- `Err(PluginError)` — 执行出错，宿主记录错误并继续

### 上下文与线程安全

`create_context()` 返回的上下文指针会作为每次生成调用的第一个参数传入。
宿主**不会**对并发调用加锁（避免挂起插件死锁整个管道），因此插件上下文**必须线程安全**——这是显式的 ABI 契约。

### NL 生成的 group_json 格式

`generate_bm25` 和 `generate_embedding` 接收 JSON 字符串形式的 `EntityGroup`；batch 方法接收 JSON **数组**。由于宿主内部的 `EntityGroup` 使用了 `CompactString`、`SmallVec` 等优化类型，SDK 不重新导出这些类型，而是直接传递 JSON。

```rust
fn generate_bm25(&self, _ctx: *mut std::ffi::c_void, group_json: &str)
    -> Result<Option<String>, PluginError> {
    let group: serde_json::Value = serde_json::from_str(group_json)
        .map_err(|e| PluginError::InvalidOutput(format!("Invalid group JSON: {}", e)))?;
    let name = group["name"].as_str().unwrap_or("unnamed");
    let kind = group["kind"].as_str().unwrap_or("unknown");
    Ok(Some(format!("{}: {}", kind, name)))
}
```

---

## `declare_plugin!` 宏参考

### 用法

```rust
declare_plugin!(MyPlugin);                // 类型实现 Default
declare_plugin!(MyPlugin, MyPlugin::new()); // 自定义初始化
```

### 生成的符号

| C 符号 | 作用 | 始终导出 |
|--------|------|---------|
| `cce_plugin_abi_version` | 返回 ABI 版本号（当前 1） | ✅ |
| `cce_plugin_metadata` | 返回 JSON 格式的 `PluginMetadata`（原始对象，非信封结构） | ✅ |
| `cce_plugin_has_bm25_generation` | 是否支持 BM25 生成 | ✅ |
| `cce_plugin_has_embedding_generation` | 是否支持 Embedding 生成 | ✅ |
| `cce_plugin_has_lifecycle` | 是否支持生命周期管理 | ✅ |
| `cce_plugin_create` | 创建插件上下文（可选） | ✅ |
| `cce_plugin_destroy` | 销毁插件上下文 | ✅ |
| `cce_plugin_free_string` | 释放由插件分配的 C 字符串 | ✅ |
| `cce_plugin_generate_bm25` | BM25 文本生成入口 | ✅ |
| `cce_plugin_generate_embedding` | Embedding 文本生成入口 | ✅ |
| `cce_plugin_generate_bm25_batch` | BM25 批量文本生成入口 | ✅ |
| `cce_plugin_generate_embedding_batch` | Embedding 批量文本生成入口 | ✅ |
| `cce_plugin_has_parse` / `cce_plugin_parse_document` | `FormatParse` 能力 | ✅ |
| `cce_plugin_has_extract` / `cce_plugin_extract_entities` | `EntityExtract` 能力 | ✅ |
| `cce_plugin_has_group` / `cce_plugin_post_group` | `Group` 能力 | ✅ |
| `cce_plugin_has_chunk` / `cce_plugin_chunk` | `Chunk` 能力 | ✅ |
| `cce_plugin_has_rerank` / `cce_plugin_rerank` | `Rerank` 能力 | ✅ |
| `cce_plugin_has_ast_language` | `AstLanguage` 能力 | ✅ |
| `cce_plugin_tree_sitter_language` | 返回自定义语言 grammar 指针 | ✅ |
| `cce_plugin_query_scheme` | 返回查询方案字符串 | ✅ |
| `cce_plugin_language_name` | 返回自定义语言名 | ✅ |
| `cce_plugin_language_extensions` | 返回扩展名 JSON 数组 | ✅ |
| `cce_plugin_has_language_remap` | `LanguageRemap` 能力（复用宿主内置 grammar） | ✅ |
| `cce_plugin_remap_grammar_language` | 返回被引用的宿主内置语言名 | ✅ |
| `cce_plugin_has_stdlib_heuristic` / `cce_plugin_classify_stdlib` | `LangHeuristics` stdlib 分类钩子 | ✅ |
| `cce_plugin_has_test_file_heuristic` / `cce_plugin_is_test_file` | `LangHeuristics` 测试文件钩子 | ✅ |
| `cce_plugin_has_entity_kind_heuristic` / `cce_plugin_entity_kind` | `LangHeuristics` 实体 kind 钩子 | ✅ |
| `cce_plugin_has_group_override` / `cce_plugin_group` | `GroupOverride` 能力（完全替换内置分组） | ✅ |
| `cce_plugin_has_relation_extract` / `cce_plugin_extract_symbols` / `cce_plugin_extract_relations` | `RelationExtract` 能力（符号 + 显式关系注入） | ✅ |
| `cce_plugin_has_symbol_extract` / `cce_plugin_extract_imports` / `cce_plugin_extract_exports` | `SymbolExtract` 能力（import/export 提取） | ✅ |
| `cce_plugin_has_query_rewrite` / `cce_plugin_rewrite_query` | `QueryRewrite` 能力 | ✅ |
| `cce_plugin_has_fusion` / `cce_plugin_fusion_weights` | `Fusion` 能力 | ✅ |
| `cce_plugin_has_result_filter` / `cce_plugin_filter_results` | `ResultFilter` 能力 | ✅ |
| `cce_plugin_has_file_filter` / `cce_plugin_filter_file` | `FileFilter` 能力 | ✅ |

所有函数始终生成，未实现的能力返回 `{"result":"none"}`。宿主通过 `supports_*` 方法判断是否调用，因此不会产生额外开销。

### 静态单例与 panic 防护

宏会生成一个 `std::sync::LazyLock<MyPlugin>` 静态单例，并确保所有导出函数不会把 panic 展开到 C ABI 边界（`catch_unwind` 防护）。插件内 panic 会以 `{"result":"error",...,"error_type":"execution_failed"}` 形式回报给宿主。

---

## ABI 协议

### C 函数签名

```c
// 必需
uint32_t cce_plugin_abi_version(void);
char*    cce_plugin_metadata(void);
bool     cce_plugin_has_bm25_generation(void);
bool     cce_plugin_has_embedding_generation(void);
bool     cce_plugin_has_lifecycle(void);
void*    cce_plugin_create(void);
void     cce_plugin_destroy(void* ctx);
void     cce_plugin_free_string(char* ptr);

// 可选（由宿主按需加载）
char* cce_plugin_generate_bm25(void* ctx, const char* group_json);
char* cce_plugin_generate_embedding(void* ctx, const char* group_json);
char* cce_plugin_generate_bm25_batch(void* ctx, const char* groups_json);
char* cce_plugin_generate_embedding_batch(void* ctx, const char* groups_json);

// 语言扩展（AstLanguage / LanguageRemap / LangHeuristics）
const void* cce_plugin_tree_sitter_language(void);
bool        cce_plugin_has_ast_language(void);
char*       cce_plugin_query_scheme(void* ctx, uint32_t query_type);
char*       cce_plugin_language_name(void);
char*       cce_plugin_language_extensions(void);
bool        cce_plugin_has_language_remap(void);
char*       cce_plugin_remap_grammar_language(void);
bool        cce_plugin_has_stdlib_heuristic(void);
char*       cce_plugin_classify_stdlib(void* ctx, const char* module_path);
bool        cce_plugin_has_test_file_heuristic(void);
char*       cce_plugin_is_test_file(void* ctx, const char* file_path, const char* content);
bool        cce_plugin_has_entity_kind_heuristic(void);
char*       cce_plugin_entity_kind(void* ctx, const char* capture_name);
```

### FFI 结果格式

所有返回 `char*` 的生成函数都返回 JSON 字符串，格式统一：

```json
// 成功，有返回值（batch：value 为长度与输入一致的数组，元素可为 null）
{"result":"ok","value":<任意JSON值>}

// 成功，无返回值（宿主回退到内置转换器）
{"result":"none"}

// 错误——error_type 还原宿主的 PluginError 变体
//   script | timeout | invalid_output | logic | resource |
//   circuit_broken | not_found | execution_failed（缺省视为 script）
{"result":"error","message":"人类可读的错误描述","error_type":"script"}
```

### 内存管理

- 插件分配所有返回的 `char*` 字符串（通过 `CString::into_raw()` / `malloc`）
- 宿主调用 `cce_plugin_free_string(ptr)` 释放
- SDK 自动处理：`declare_plugin!` 生成的 `cce_plugin_free_string` 正确释放所有返回字符串

---

## 完整示例

### 最小插件（仅元数据）

```rust
use cce_plugin_sdk::{declare_plugin, FfiPlugin, PluginMetadata};

struct MinimalPlugin;

impl FfiPlugin for MinimalPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "example/minimal".into(),
            name: "Minimal Plugin".into(),
            version: "1.0.0".into(),
            priority: 0,
            description: None,
        }
    }
}

declare_plugin!(MinimalPlugin);
```

### NL 生成插件（含批量）

```rust
use cce_plugin_sdk::{declare_plugin, FfiPlugin, PluginMetadata, PluginError};

struct NlPlugin;

impl FfiPlugin for NlPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "example/nl-gen".into(),
            name: "NL Generator".into(),
            version: "0.1.0".into(),
            priority: 5,
            description: Some("Generates descriptions for code entities".into()),
        }
    }

    fn supports_bm25(&self) -> bool { true }
    fn supports_embedding(&self) -> bool { true }

    fn generate_bm25(&self, _ctx: *mut std::ffi::c_void, group_json: &str)
        -> Result<Option<String>, PluginError> {
        let v: serde_json::Value = serde_json::from_str(group_json)
            .map_err(|e| PluginError::InvalidOutput(format!("JSON parse error: {}", e)))?;
        let name = v["name"].as_str().unwrap_or("?");
        let kind = v["kind"].as_str().unwrap_or("?");
        Ok(Some(format!("A {} named {}", kind, name)))
    }

    fn generate_bm25_batch(&self, _ctx: *mut std::ffi::c_void, groups_json: &str)
        -> Result<Vec<Option<String>>, PluginError> {
        let groups: Vec<serde_json::Value> = serde_json::from_str(groups_json)
            .map_err(|e| PluginError::InvalidOutput(format!("JSON parse error: {}", e)))?;
        Ok(groups.iter().map(|g| {
            Some(format!("A {} named {}", g["kind"].as_str().unwrap_or("?"), g["name"].as_str().unwrap_or("?")))
        }).collect())
    }
}

declare_plugin!(NlPlugin);
```

### 带生命周期的插件

```rust
use cce_plugin_sdk::{declare_plugin, FfiPlugin, PluginMetadata, PluginError};
use std::collections::HashMap;

struct StatefulPlugin { cache: HashMap<String, String> }

impl Default for StatefulPlugin {
    fn default() -> Self { Self { cache: HashMap::new() } }
}

impl FfiPlugin for StatefulPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "example/stateful".into(),
            name: "Stateful Plugin".into(),
            version: "1.0.0".into(),
            priority: 10,
            description: Some("Plugin with internal state".into()),
        }
    }

    fn supports_lifecycle(&self) -> bool { true }

    fn create_context(&self) -> Option<*mut std::ffi::c_void> {
        let ctx = Box::new(HashMap::<String, String>::new());
        Some(Box::into_raw(ctx) as *mut _)
    }

    unsafe fn destroy_context(&self, ctx: *mut std::ffi::c_void) {
        if !ctx.is_null() {
            drop(Box::from_raw(ctx as *mut HashMap<String, String>));
        }
    }
}

declare_plugin!(StatefulPlugin);
```

> 上下文会被并发访问：请使用 `Mutex`/`RwLock` 保护内部可变状态。

---

## 非 Rust 插件

不依赖本 SDK，直接实现 `cce_plugin.h` 中声明的函数即可（参考
`crates/app/cce-plugin-runtime/tests/fixtures/c_plugin.c` 与端到端测试
`crates/app/cce-plugin-runtime/tests/native_plugin_e2e.rs`）。注意：
- 所有返回的 `char*` 必须由插件分配、宿主通过 `cce_plugin_free_string` 释放
- 生成函数入参为 UTF-8 字符串，返回 `FfiResult` 信封 JSON
- 插件上下文必须线程安全

---

## 版本兼容性

| SDK 版本 | ABI 版本 | 最低 Rust 版本 | 备注 |
|----------|---------|---------------|------|
| 0.x（开发阶段） | 1 | 1.80（宿主 workspace 要求 1.86+） | 开发阶段版本历史已重置为 1；破坏性升级时递增，宿主拒绝低于最小支持版本的插件 |

宿主加载时会检查 `cce_plugin_abi_version()` 返回值，低于宿主最小支持版本（1）的插件会被拒绝；高于当前版本（1）的插件会被接受并记录警告。遵循「不回退兼容」原则（见 `AGENTS.md` 与 `docs/plan/plugin_extension_design.md` §2），ABI 升级不做向后兼容。
