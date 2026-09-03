# 工具 API

## POST /api/tools/compress

压缩代码文件（AST转自然语言）。

该端点将代码文件解析为AST，然后将实体转换为自然语言描述，用于简化大型单体文件的理解。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "file_path": "src/main.rs",
  "include_entities": true,
  "include_groups": false
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `file_path` | string | 是 | - | 要压缩的文件路径 |
| `include_entities` | boolean | 否 | `false` | 是否在响应中包含实体信息 |
| `include_groups` | boolean | 否 | `false` | 是否在响应中包含分组信息 |

### 响应

```json
{
  "success": true,
  "data": {
    "file_path": "src/main.rs",
    "compressed_text": "The file contains a main function that initializes the application...",
    "entities": [...],
    "groups": [...]
  },
  "error": null
}
```

### 示例

```bash
curl -X POST "http://localhost:3000/api/tools/compress" \
  -H "Content-Type: application/json" \
  -d '{
    "file_path": "src/main.rs",
    "include_entities": true
  }'
```

---

## POST /api/tools/compress/batch

批量压缩代码文件（AST转自然语言）。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "file_paths": ["src/main.rs", "src/lib.rs"],
  "include_entities": false,
  "include_groups": false,
  "max_concurrency": 4
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `file_paths` | string[] | 是 | - | 要压缩的文件路径列表 |
| `include_entities` | boolean | 否 | `false` | 是否包含实体信息 |
| `include_groups` | boolean | 否 | `false` | 是否包含分组信息 |
| `max_concurrency` | number | 否 | `4` | 最大并发任务数 |

### 响应

```json
{
  "successes": [
    ["src/main.rs", {"file_path": "src/main.rs", "compressed_text": "..."}],
    ["src/lib.rs", {"file_path": "src/lib.rs", "compressed_text": "..."}]
  ],
  "failures": []
}
```

### 示例

```bash
curl -X POST "http://localhost:3000/api/tools/compress/batch" \
  -H "Content-Type: application/json" \
  -d '{
    "file_paths": ["src/main.rs", "src/lib.rs"]
  }'
```

---

## POST /api/tools/diagnose

诊断代码语法错误（基于AST解析）。

该端点使用tree-sitter解析代码，检测语法错误，如未闭合的括号、未闭合的字符串、缺少分号等。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "code": "fn main() { let x = 1; }",
  "language": "rust",
  "file_name": "main.rs",
  "include_ast": false
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `code` | string | 是 | - | 要诊断的代码内容 |
| `language` | string | 否 | 自动检测 | 编程语言（从file_name或自动检测） |
| `file_name` | string | 否 | - | 文件名（用于语言检测和错误消息） |
| `include_ast` | boolean | 否 | `false` | 是否在响应中包含AST结构 |

**支持的语言**:

Rust, Python, JavaScript, TypeScript, C, C++, C#, Go, Java, Kotlin, Ruby, PHP, JSON, YAML, TOML, XML, HTML, CSS, SCSS, LESS, Vue, Svelte, JSX, TSX

### 响应

```json
{
  "success": true,
  "result": {
    "has_errors": false,
    "errors": [],
    "ast": null
  },
  "error": null
}
```

或者当有错误时：

```json
{
  "success": true,
  "result": {
    "has_errors": true,
    "errors": [
      {
        "message": "Expected '}' to close block",
        "line": 5,
        "column": 0
      }
    ],
    "ast": null
  },
  "error": null
}
```

### 示例

```bash
curl -X POST "http://localhost:3000/api/tools/diagnose" \
  -H "Content-Type: application/json" \
  -d '{
    "code": "fn main() { let x = 1; }",
    "language": "rust"
  }'
```

---

## POST /api/tools/keyword-search

BM25 关键词搜索。从 BM25 索引中检索匹配的代码块，从 SQLite 中获取完整内容并生成高亮片段（`<mark>` 标签）。

该端点独立于向量搜索管道，专注于纯关键词搜索场景。如果 SQLite 未配置，仍然返回 BM25 结果但不含高亮片段。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "query": "async fn handle",
  "top_n": 10,
  "project_id": 1
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `query` | string | 是 | - | 搜索关键词 |
| `top_n` | number | 否 | `10` | 最大返回结果数 |
| `project_id` | number | 否 | `0` | 项目 ID |

### 响应

```json
{
  "success": true,
  "data": {
    "query": "async fn handle",
    "total": 2,
    "results": [
      {
        "chunk_id": "abc123",
        "score": 0.85,
        "file_path": "src/server.rs",
        "title": "handle_request",
        "highlighted_snippet": "pub <mark>async</mark> <mark>fn</mark> <mark>handle</mark>(req: Request) -> Response { ... }",
        "start_line": 10,
        "end_line": 25
      }
    ]
  },
  "error": null
}
```

### 示例

```bash
curl -X POST "http://localhost:3000/api/tools/keyword-search" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "async fn handle",
    "top_n": 10,
    "project_id": 1
  }'
```

---

## POST /api/tools/symbols

获取文件中的符号信息。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "paths": ["src/main.rs", "src/lib.rs"]
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `paths` | string[] | 是 | - | 文件路径列表 |

### 响应

```json
{
  "success": true,
  "result": {
    "symbols": {
      "src/main.rs": [
        {
          "name": "main",
          "kind": "function",
          "start_point": {"row": 0, "column": 0},
          "end_point": {"row": 5, "column": 1}
        }
      ],
      "src/lib.rs": [...]
    }
  },
  "error": null
}
```

### 示例

```bash
curl -X POST "http://localhost:3000/api/tools/symbols" \
  -H "Content-Type: application/json" \
  -d '{
    "paths": ["src/main.rs"]
  }'
```

---

## POST /api/tools/references

查找符号的所有引用。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "path": "src/main.rs",
  "line": 10,
  "column": 5,
  "symbol": "parse_file",
  "context_lines": 2,
  "include_snippet": true,
  "include_entity_info": false
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `path` | string | 是 | - | 包含符号的文件路径 |
| `line` | number | 是 | - | 行号（从1开始） |
| `column` | number | 否 | - | 列号（从1开始，可选） |
| `symbol` | string | 否 | - | 符号名称（可选，用于文档） |
| `context_lines` | number | 否 | - | 包含的上下文行数 |
| `include_snippet` | boolean | 否 | - | 是否为每个引用包含代码片段 |
| `include_entity_info` | boolean | 否 | - | 是否包含调用者实体信息 |

### 响应

```json
{
  "success": true,
  "result": {
    "references": [
      {
        "file_path": "src/main.rs",
        "line": 10,
        "column": 5,
        "snippet": "fn parse_file() { ... }",
        "entity_info": null
      },
      {
        "file_path": "src/indexer.rs",
        "line": 45,
        "column": 10,
        "snippet": "let result = parse_file();",
        "entity_info": null
      }
    ],
    "total": 2
  },
  "error": null
}
```

### 示例

```bash
curl -X POST "http://localhost:3000/api/tools/references" \
  -H "Content-Type: application/json" \
  -d '{
    "path": "src/main.rs",
    "line": 10,
    "column": 5
  }'
```

---

## POST /api/tools/definition

跳转到符号定义。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "path": "src/indexer.rs",
  "line": 45,
  "column": 10,
  "symbol": "parse_file",
  "include_body": false
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `path` | string | 是 | - | 文件路径 |
| `line` | number | 是 | - | 行号（从1开始） |
| `column` | number | 否 | - | 列号（从1开始，可选） |
| `symbol` | string | 否 | - | 符号名称（可选，用于文档） |
| `include_body` | boolean | 否 | `false` | 是否包含完整的定义体 |

### 响应

```json
{
  "success": true,
  "result": {
    "definitions": [
      {
        "file_path": "src/parser.rs",
        "line": 10,
        "column": 0,
        "name": "parse_file",
        "kind": "function",
        "body": null
      }
    ]
  },
  "error": null
}
```

### 示例

```bash
curl -X POST "http://localhost:3000/api/tools/definition" \
  -H "Content-Type: application/json" \
  -d '{
    "path": "src/indexer.rs",
    "line": 45,
    "column": 10
  }'
```
