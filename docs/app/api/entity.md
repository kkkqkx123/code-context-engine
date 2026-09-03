# 实体查询 API

## GET /api/project/{project_id}/function/{id}

获取函数详细信息。

### 请求

**方法**: `GET`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |
| `id` | string | 函数 ID（稳定符号 ID） |

### 响应

```json
{
  "success": true,
  "function": {
    "id": 123,
    "name": "parse_file",
    "signature": "fn parse_file(path: &Path) -> Result<ParsedFile>",
    "parameters": [
      {
        "name": "path",
        "type_name": "&Path"
      }
    ],
    "return_type": "Result<ParsedFile>",
    "file_path": "src/parser.rs",
    "start_line": 10,
    "end_line": 25,
    "doc_comment": "Parse a file and extract entities"
  }
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `function` | FunctionInfo | 函数信息 |

**FunctionInfo 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `id` | number | 函数 ID |
| `name` | string | 函数名称 |
| `signature` | string | 函数签名 |
| `parameters` | ParameterInfo[] | 参数列表 |
| `return_type` | string? | 返回类型 |
| `file_path` | string | 文件路径 |
| `start_line` | number | 起始行号 |
| `end_line` | number | 结束行号 |
| `doc_comment` | string? | 文档注释 |

### 示例

```bash
curl "http://localhost:3000/api/project/1/function/123"
```

---

## GET /api/project/{project_id}/function/{id}/calls

获取函数调用的所有函数（被调用者）。

### 请求

**方法**: `GET`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |
| `id` | string | 函数 ID（稳定符号 ID） |

### 响应

```json
{
  "success": true,
  "function_id": 123,
  "function_name": "parse_file",
  "callees": [
    {
      "function_id": 124,
      "function_name": "read_file",
      "file_path": "src/io.rs",
      "depth": 1,
      "relation_type": "callee",
      "call_line": 12
    }
  ],
  "total_callees": 5
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `function_id` | number | 函数 ID |
| `function_name` | string | 函数名称 |
| `callees` | CallChainNode[] | 被调用函数列表 |
| `total_callees` | number | 被调用函数总数 |

### 示例

```bash
curl "http://localhost:3000/api/project/1/function/123/calls"
```

---

## GET /api/project/{project_id}/function/{id}/callers

获取调用该函数的所有函数（调用者）。

### 请求

**方法**: `GET`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |
| `id` | string | 函数 ID（稳定符号 ID） |

### 响应

```json
{
  "success": true,
  "function_id": 123,
  "function_name": "parse_file",
  "callers": [
    {
      "function_id": 125,
      "function_name": "index_directory",
      "file_path": "src/indexer.rs",
      "depth": 1,
      "relation_type": "caller",
      "call_line": 45
    }
  ],
  "total_callers": 3
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `function_id` | number | 函数 ID |
| `function_name` | string | 函数名称 |
| `callers` | CallChainNode[] | 调用者列表 |
| `total_callers` | number | 调用者总数 |

### 示例

```bash
curl "http://localhost:3000/api/project/1/function/123/callers"
```

---

## GET /api/project/{project_id}/call-chain/{id}

获取函数的完整调用链。

### 请求

**方法**: `GET`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |
| `id` | string | 函数 ID（稳定符号 ID） |

**查询参数**:

| 参数 | 类型 | 默认值 | 描述 |
|-----|------|--------|------|
| `direction` | string | `"down"` | 调用链方向（`"up"` 或 `"down"`） |
| `max_depth` | number | `5` | 最大深度 |

### 响应

```json
{
  "success": true,
  "function_id": 123,
  "function_name": "parse_file",
  "direction": "down",
  "call_chain": [
    {
      "function_id": 124,
      "function_name": "read_file",
      "file_path": "src/io.rs",
      "depth": 1,
      "relation_type": "callee",
      "call_line": 12
    },
    {
      "function_id": 125,
      "function_name": "detect_encoding",
      "file_path": "src/encoding.rs",
      "depth": 2,
      "relation_type": "callee",
      "call_line": 15
    }
  ]
}
```

### 示例

```bash
# 向下追踪调用链
curl "http://localhost:3000/api/project/1/call-chain/123?direction=down&max_depth=5"

# 向上追踪调用链
curl "http://localhost:3000/api/project/1/call-chain/123?direction=up&max_depth=5"
```

---

## GET /api/project/{project_id}/call-path

查询两个函数之间的调用路径。

### 请求

**方法**: `GET`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |

**查询参数**:

| 参数 | 类型 | 必填 | 默认值 | 描述 |
|-----|------|------|--------|------|
| `start_id` | number | 是 | - | 起始函数 ID |
| `end_id` | number | 是 | - | 目标函数 ID |
| `max_depth` | number | 否 | `10` | 最大搜索深度 |

### 响应

```json
{
  "success": true,
  "start_function_id": 123,
  "end_function_id": 456,
  "path_found": true,
  "path": [
    {
      "function_id": 123,
      "function_name": "index_directory",
      "file_path": "src/indexer.rs",
      "depth": 0,
      "relation_type": "start",
      "call_line": null
    },
    {
      "function_id": 124,
      "function_name": "parse_file",
      "file_path": "src/parser.rs",
      "depth": 1,
      "relation_type": "callee",
      "call_line": 45
    },
    {
      "function_id": 456,
      "function_name": "read_file",
      "file_path": "src/io.rs",
      "depth": 2,
      "relation_type": "callee",
      "call_line": 12
    }
  ],
  "path_length": 2
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `start_function_id` | number | 起始函数 ID |
| `end_function_id` | number | 目标函数 ID |
| `path_found` | boolean | 是否找到路径 |
| `path` | CallChainNode[] | 调用路径 |
| `path_length` | number | 路径长度 |

### 示例

```bash
curl "http://localhost:3000/api/project/1/call-path?start_id=123&end_id=456&max_depth=10"
```

---

## GET /api/project/{project_id}/class/{id}/inheritance

获取类的继承关系。

### 请求

**方法**: `GET`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |
| `id` | string | 类 ID（稳定符号 ID） |

### 响应

```json
{
  "success": true,
  "class_id": 123,
  "class_name": "Parser",
  "base_classes": [
    {
      "class_id": 124,
      "class_name": "BaseParser",
      "file_path": "src/base.rs",
      "depth": 1
    }
  ],
  "derived_classes": [
    {
      "class_id": 125,
      "class_name": "RustParser",
      "file_path": "src/rust_parser.rs",
      "depth": 1
    }
  ]
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `class_id` | number | 类 ID |
| `class_name` | string | 类名称 |
| `base_classes` | ClassRelation[] | 基类列表 |
| `derived_classes` | ClassRelation[] | 派生类列表 |

**ClassRelation 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `class_id` | number | 类 ID |
| `class_name` | string | 类名称 |
| `file_path` | string | 文件路径 |
| `depth` | number | 继承深度 |

### 示例

```bash
curl "http://localhost:3000/api/project/1/class/123/inheritance"
```

---

## GET /api/project/{project_id}/class/{id}/implementations

获取类的实现关系（接口实现）。

### 请求

**方法**: `GET`

**路径参数**:

| 参数 | 类型 | 描述 |
|-----|------|------|
| `project_id` | number | 项目 ID |
| `id` | string | 类 ID（稳定符号 ID） |

### 响应

```json
{
  "success": true,
  "class_id": 123,
  "class_name": "Parser",
  "implemented_interfaces": [
    {
      "interface_id": 124,
      "interface_name": "IParser",
      "file_path": "src/interfaces.rs"
    }
  ],
  "implementing_classes": [
    {
      "class_id": 125,
      "class_name": "RustParser",
      "file_path": "src/rust_parser.rs",
      "depth": 1
    }
  ]
}
```

**响应字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `success` | boolean | 操作是否成功 |
| `class_id` | number | 类 ID |
| `class_name` | string | 类名称 |
| `implemented_interfaces` | InterfaceRelation[] | 实现的接口列表 |
| `implementing_classes` | ClassRelation[] | 实现该类的类列表 |

**InterfaceRelation 字段**:

| 字段 | 类型 | 描述 |
|-----|------|------|
| `interface_id` | number | 接口 ID |
| `interface_name` | string | 接口名称 |
| `file_path` | string | 文件路径 |

### 示例

```bash
curl "http://localhost:3000/api/project/1/class/123/implementations"
```
