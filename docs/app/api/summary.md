# 文件摘要 API

## POST /api/summary

生成文件摘要（不存储）。

### 请求

**方法**: `POST`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "file_paths": ["src/main.rs", "src/parser.rs"],
  "directory_paths": ["src/utils/"],
  "extensions": ["rs", "py"],
  "exclude_dirs": ["target", "tests"],
  "respect_gitignore": true,
  "ignore_patterns": ["*.test.*"],
  "recursive": true,
  "max_files": 100
}
```

**请求字段**:

| 字段 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `file_paths` | string[] | 否 | `[]` | 要生成摘要的文件路径列表 |
| `directory_paths` | string[] | 否 | `[]` | 要扫描的目录路径列表 |
| `extensions` | string[] | 否 | `[]` | 要包含的文件扩展名 |
| `exclude_dirs` | string[] | 否 | `[]` | 要排除的目录 |
| `respect_gitignore` | boolean | 否 | `true` | 是否遵守 .gitignore |
| `ignore_patterns` | string[] | 否 | `[]` | 额外的忽略模式 |
| `recursive` | boolean | 否 | `true` | 是否递归扫描目录 |
| `max_files` | number | 否 | `100` | 最大处理文件数（安全限制） |

### 响应

```json
{
  "success": true,
  "total_files": 5,
  "success_count": 4,
  "failed_count": 1,
  "summaries": [
    {
      "file_path": "src/main.rs",
      "language": "rust",
      "summary": "Main entry point for the application. Contains the main function that initializes the engine and starts the HTTP server.",
      "main_entities": ["main", "serve"],
      "imports": ["std::net", "axum", "tokio"],
      "exports": ["serve"],
      "entity_count": 5,
      "line_count": 150,
      "tags": ["entry-point", "server", "initialization"],
      "importance_level": "high",
      "success": true,
      "error": null
    },
    {
      "file_path": "src/parser.rs",
      "language": "rust",
      "summary": "Parser module for extracting entities from source code using tree-sitter.",
      "main_entities": ["Parser", "parse_file", "extract_entities"],
      "imports": ["tree_sitter", "crate::ast"],
      "exports": ["Parser", "parse_file"],
      "entity_count": 15,
      "line_count": 300,
      "tags": ["parsing", "ast", "tree-sitter"],
      "importance_level": "medium",
      "success": true,
      "error": null
    }
  ],
  "elapsed_ms": 500,
  "warnings": []
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `total_files` | number | 处理的文件总数 |
| `success_count` | number | 成功处理的文件数 |
| `failed_count` | number | 失败的文件数 |
| `summaries` | FileSummaryItem[] | 摘要结果列表 |
| `elapsed_ms` | number | 耗时（毫秒） |
| `warnings` | string[] | 警告消息列表 |

**FileSummaryItem 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `file_path` | string | 文件路径 |
| `language` | string | 编程语言 |
| `summary` | string | 摘要文本 |
| `main_entities` | string[] | 主要实体列表 |
| `imports` | string[] | 导入的模块/依赖 |
| `exports` | string[] | 导出的符号 |
| `entity_count` | number | 实体总数 |
| `line_count` | number | 代码行数 |
| `tags` | string[] | 分类标签 |
| `importance_level` | string | 重要程度（high/medium/low） |
| `success` | boolean | 是否解析成功 |
| `error` | string? | 错误消息（如果失败） |

### 示例

```bash
# 为单个文件生成摘要
curl -X POST "http://localhost:3000/api/summary" \
  -H "Content-Type: application/json" \
  -d '{
    "file_paths": ["src/main.rs"]
  }'

# 为目录生成摘要
curl -X POST "http://localhost:3000/api/summary" \
  -H "Content-Type: application/json" \
  -d '{
    "directory_paths": ["src/"],
    "extensions": ["rs"],
    "exclude_dirs": ["target", "tests"],
    "max_files": 50
  }'

# 为多个文件生成摘要
curl -X POST "http://localhost:3000/api/summary" \
  -H "Content-Type: application/json" \
  -d '{
    "file_paths": ["src/main.rs", "src/parser.rs", "src/indexer.rs"]
  }'
```

### 使用场景

1. **代码审查**: 快速了解文件的主要功能和结构
2. **文档生成**: 为代码库生成概览文档
3. **代码导航**: 帮助开发者快速定位相关代码
4. **重构分析**: 识别文件的重要性和依赖关系

### 注意事项

- 这是一个临时操作，不会存储任何数据
- `max_files` 参数用于防止意外处理过多文件
- 失败的文件会在响应中标记，但不会影响其他文件的处理
- 警告消息可能包含跳过的文件信息
