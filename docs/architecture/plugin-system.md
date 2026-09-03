# Plugin System Architecture

## Overview

CCE plugin system enables extending the indexing pipeline via **Lua scripts** and **Native dynamic libraries**. Both plugin types implement the same `CodePlugin` trait and share a unified capability-based routing mechanism.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Application Layer                        │
│  cce-plugin-sdk    │    cce-plugin-runtime                      │
│  ┌──────────────┐  │  ┌──────────────────────────────────────┐  │
│  │ FfiPlugin    │  │  │ LuaPlugin          NativePlugin      │  │
│  │ declare_     │  │  │ (mlua 5.5)         (libloading)      │  │
│  │ plugin!      │  │  │ VM pool + sandbox  C ABI v1          │  │
│  └──────────────┘  │  └──────────────────────────────────────┘  │
├────────────────────┼────────────────────────────────────────────┤
│                    │  FilePluginSource (registry.rs)            │
│                    │  plugins.json → PluginBundle[]             │
├────────────────────┴────────────────────────────────────────────┤
│                        Core Layer                               │
│  cce-plugin                                                           │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │ CodePlugin trait    PluginRegistry    PluginCapability       │  │
│  │ PluginBundle        PluginSource      PluginError            │  │
│  └──────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

## Crate Layout

| Crate | Path | Role |
|-------|------|------|
| `cce-plugin` | `crates/core/cce-plugin` | Core types: `CodePlugin`, `PluginRegistry`, `PluginCapability`, `PluginError` |
| `cce-plugin-runtime` | `crates/app/cce_plugin_runtime` | Concrete loaders: `LuaPlugin`, `NativePlugin`, `FilePluginSource` |
| `cce-plugin-sdk` | `crates/app/plugin-sdk` | SDK for building native plugins: `FfiPlugin` trait, `declare_plugin!` macro |

## Plugin Lifecycle

### Loading

```
1. FilePluginSource reads .cce/plugins.json
2. For each PluginEntry:
   a. Compute content_digest (SHA-256 of file bytes)
   b. Dispatch by PluginType:
      - Lua:   read script → validate syntax → probe plugin table → build LuaPlugin
      - Native: dlopen → validate ABI version → extract symbols → probe capabilities → build NativePlugin
3. Wrap in PluginBundle (with file_patterns, languages, capabilities, priority)
4. Register in PluginRegistry (HashMap<id, RegistryEntry>)
```

### Execution

```
1. Pipeline code queries PluginRegistry by capability + file_path + language
2. Registry filters by file_patterns (glob) and languages (case-insensitive)
3. Sort by effective priority (per-capability override > plugin-level priority), descending
4. Call plugins in priority order:
   - Override tier: first non-None result wins, remaining plugins skipped
   - Chain tier: all run in order, previous output feeds next input
   - Additive tier: all run, results merged
```

### Unloading

- `LuaPlugin`: Lua states returned to pool, VMs dropped on pool overflow
- `NativePlugin`: `Drop` calls `cce_plugin_destroy` (if lifecycle supported), library handle dropped

## Capability System

### Capability Facets (16 total)

| Facet | Consumption | Description |
|-------|-------------|-------------|
| `TextGen` | Override (per-group) | AST→NL text generation (BM25/Embedding) |
| `FormatParse` | Override | Parse custom document formats into entities |
| `EntityExtract` | Additive | Supplementary entity extraction from code |
| `AstLanguage` | Additive | Custom tree-sitter grammar + query scheme (Native only) |
| `LanguageRemap` | Additive | Map custom language to host built-in grammar (Lua + Native) |
| `LangHeuristics` | Override (per-method) | stdlib/test-file/entity-kind heuristics |
| `SymbolExtract` | Override | Import/export extraction for custom languages |
| `Group` | Chain | Post-grouping hook |
| `GroupOverride` | Override | Fully replace built-in grouping |
| `Chunk` | Override | Replace built-in chunking |
| `Rerank` | Override | Query result reranking |
| `RelationExtract` | Additive | Supplementary symbol/relation extraction |
| `QueryRewrite` | Chain | Query rewriting/expansion before recall |
| `Fusion` | Override | Hybrid fusion weight override |
| `ResultFilter` | Chain | Post-rerank result filtering/annotation |
| `FileFilter` | Override | Scanner file inclusion/exclusion |

### Priority System

- **Plugin-level priority**: Global priority for all capabilities (default: `0`)
- **Per-capability priority**: Override per capability via `capability_priorities` map
- **Host override**: `plugins.json` entry can override both plugin-level and per-capability priorities
- **Negative priority**: Places plugin below built-in implementation (fallback tier)

### Effective Priority Resolution

```
1. Start with plugin.metadata.priority
2. Apply bundle.priority override (if set)
3. Apply bundle.capability_priorities override (if set)
4. Apply plugin.metadata.capability_priorities for remaining capabilities
```

## Plugin Types

### Lua Plugins

**Security Model**: Sandboxed execution via Lua 5.5 standard library restrictions.

```
Allowed: string, table, math, utf8, base (pairs, ipairs, type, error, pcall, etc.)
Excluded: io, os, debug, ffi, package
```

**Execution Model**:
- VM pool with upper bound (16 states per plugin)
- Debug hook fires every 10,000 instructions to check:
  - Cancellation token (hard timeout, default 5s)
  - Memory limit (default 64MB per call)
- Each call runs on a dedicated thread, VM returned to pool after

**Script Structure**:
```lua
plugin = {
    id = "org/my-plugin",
    name = "My Plugin",
    version = "1.0.0",
    priority = 10,
    -- Capability hooks (each optional):
    generate_bm25 = function(group) ... end,
    generate_embedding = function(group) ... end,
    -- ... other hooks
}
```

### Native Plugins

**ABI Contract**: Plugins are dynamic libraries (`.so`/`.dylib`/`.dll`) exporting C functions defined in `plugin-sdk/include/cce_plugin.h`.

**Required Exports**:
| Symbol | Signature |
|--------|-----------|
| `cce_plugin_abi_version` | `fn() -> u32` |
| `cce_plugin_metadata` | `fn() -> *mut c_char` (JSON) |
| `cce_plugin_has_bm25_generation` | `fn() -> bool` |
| `cce_plugin_has_embedding_generation` | `fn() -> bool` |
| `cce_plugin_has_lifecycle` | `fn() -> bool` |
| `cce_plugin_free_string` | `fn(*mut c_char)` |

**Optional Exports** (30+ symbols for each capability facet):
- `cce_plugin_generate_bm25` / `_batch`
- `cce_plugin_generate_embedding` / `_batch`
- `cce_plugin_parse_document`
- `cce_plugin_extract_entities`
- `cce_plugin_post_group` / `cce_plugin_group`
- `cce_plugin_chunk`
- `cce_plugin_rerank`
- `cce_plugin_extract_symbols` / `extract_relations`
- `cce_plugin_extract_imports` / `extract_exports`
- `cce_plugin_rewrite_query`
- `cce_plugin_fusion_weights`
- `cce_plugin_filter_results`
- `cce_plugin_filter_file`
- `cce_plugin_tree_sitter_language`
- `cce_plugin_query_scheme`
- `cce_plugin_language_name` / `language_extensions`
- `cce_plugin_remap_grammar_language`
- `cce_plugin_classify_stdlib` / `is_test_file` / `entity_kind`

**FFI Result Protocol**:
```json
{"result":"ok","value":<json>}
{"result":"none"}
{"result":"error","message":"...","error_type":"script|timeout|invalid_output|logic|resource|circuit_broken|not_found|execution_failed"}
```

**Memory Rules**:
- Plugin allocates all returned `char*` strings (via `malloc` / `CString::into_raw()`)
- Host frees via `cce_plugin_free_string`
- Plugin must not free or modify strings passed to it

**Thread Safety**: Context pointer and all FFI functions must be thread-safe. Host may call generate functions concurrently.

## Registration File Format

`.cce/plugins.json`:
```json
{
  "plugins": [
    {
      "id": "my-plugin",
      "path": "plugins/my-plugin.lua",
      "plugin_type": "lua",
      "enabled": true,
      "file_patterns": ["*.py", "*.rs"],
      "languages": ["python", "rust"],
      "capabilities": ["text_gen", "entity_extract"],
      "priority": 100,
      "capability_priorities": {
        "text_gen": 1000,
        "fusion": 10
      }
    }
  ]
}
```

### PluginEntry Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `id` | String | Yes | - | Unique plugin identifier |
| `path` | String | Yes | - | Path relative to registry file directory |
| `plugin_type` | Enum | No | `lua` | `lua` or `native` |
| `enabled` | Bool | No | `true` | Whether plugin is active |
| `file_patterns` | `[String]` | No | None (all) | Glob patterns for file filtering |
| `languages` | `[String]` | No | None (all) | Language name filters |
| `capabilities` | `[String]` | No | None (probe) | Declared capability facets |
| `priority` | Int32 | No | plugin metadata | Host-side priority override |
| `capability_priorities` | `{String: Int32}` | No | plugin metadata | Per-capability priority overrides |

## Error Handling

### Error Variants

| Variant | Meaning |
|---------|---------|
| `ScriptError` | Lua syntax/runtime error or plugin execution failure |
| `Timeout` | Plugin execution exceeded time budget |
| `InvalidOutput` | Plugin returned malformed data |
| `LogicError` | Internal logic error in plugin |
| `ResourceError` | External resource unavailable (file not found, library load failed) |
| `CircuitBroken` | Plugin disabled by circuit breaker |
| `NotFound` | Plugin not found |
| `ExecutionFailed` | Generic execution failure (e.g., panic recovery) |

### Error Propagation

- Plugin errors are logged and execution continues with built-in fallback
- `error_type` string in FFI result maps to concrete `PluginError` variant on host
- Unknown/missing `error_type` defaults to `ScriptError`

## Safety Considerations

### Lua Sandbox

- Standard library restrictions prevent file system access, process spawning, and external module loading
- Debug hook enforces instruction count and memory limits per call
- Lua states are pooled to avoid unbounded memory growth

### Native Plugin Safety

- `declare_plugin!` macro wraps all user code in `catch_unwind` to prevent panic unwinding across FFI boundary
- ABI version validation rejects incompatible plugins
- Context pointer is treated as opaque token, only dereferenced through plugin's own FFI functions
- Library handle kept alive to ensure function pointers remain valid

### Threat Model

- Plugins are loaded from trusted sources (project-level `plugins.json`)
- No remote plugin loading (plugins must be local files)
- Lua sandbox prevents file system access
- Native plugins have full system access (same as the host process)
