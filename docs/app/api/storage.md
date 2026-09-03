# 存储管理 API

## DELETE /api/index

清空所有索引数据。

### 请求

**方法**: `DELETE`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "clear_vectors": true,
  "clear_bm25": true,
  "clear_relations": true,
  "clear_cache": true
}
```

**请求字段**:

| 字段              | 类型    | 必填 | 默认值 | 描述               |
| ----------------- | ------- | ---- | ------ | ------------------ |
| `clear_vectors`   | boolean | 否   | `true` | 是否清空向量存储   |
| `clear_bm25`      | boolean | 否   | `true` | 是否清空 BM25 索引 |
| `clear_relations` | boolean | 否   | `true` | 是否清空关系存储   |
| `clear_cache`     | boolean | 否   | `true` | 是否清空缓存       |

### 响应

```json
{
  "success": true,
  "vectors_cleared": 1500,
  "bm25_cleared": 1500,
  "relations_cleared": 300,
  "cache_cleared": 100,
  "elapsed_ms": 500,
  "message": "Index cleared successfully"
}
```

**响应字段**:

| 字段                | 类型    | 描述               |
| ------------------- | ------- | ------------------ |
| `success`           | boolean | 操作是否成功       |
| `vectors_cleared`   | number  | 清除的向量数       |
| `bm25_cleared`      | number  | 清除的 BM25 条目数 |
| `relations_cleared` | number  | 清除的关系数       |
| `cache_cleared`     | number  | 清除的缓存条目数   |
| `elapsed_ms`        | number  | 耗时（毫秒）       |
| `message`           | string  | 结果描述消息       |

### 示例

```bash
# 清空所有索引
curl -X DELETE "http://localhost:3000/api/index"

# 仅清空向量存储
curl -X DELETE "http://localhost:3000/api/index" \
  -H "Content-Type: application/json" \
  -d '{
    "clear_vectors": true,
    "clear_bm25": false,
    "clear_relations": false,
    "clear_cache": false
  }'
```

---

## DELETE /api/index/file/:file_path

删除指定文件的所有索引数据。

### 请求

**方法**: `DELETE`

**路径参数**:

| 参数        | 类型   | 描述                 |
| ----------- | ------ | -------------------- |
| `file_path` | string | 文件路径（URL 编码） |

### 响应

```json
{
  "success": true,
  "message": "File deleted successfully: src/old_file.rs",
  "file_path": "src/old_file.rs",
  "vectors_deleted": 25,
  "bm25_documents_deleted": 25,
  "relations_deleted": 25,
  "elapsed_ms": 50
}
```

**响应字段**:

| 字段                     | 类型    | 描述               |
| ------------------------ | ------- | ------------------ |
| `success`                | boolean | 操作是否成功       |
| `message`                | string  | 结果描述消息       |
| `file_path`              | string  | 被删除的文件路径   |
| `vectors_deleted`        | number  | 删除的向量数       |
| `bm25_documents_deleted` | number  | 删除的 BM25 文档数 |
| `relations_deleted`      | number  | 删除的关系数       |
| `elapsed_ms`             | number  | 耗时（毫秒）       |

### 示例

```bash
curl -X DELETE "http://localhost:3000/api/index/file/src%2Fold_file.rs"
```

---

## DELETE /api/index/entity/:id

删除指定实体。

### 请求

**方法**: `DELETE`

**路径参数**:

| 参数 | 类型   | 描述    |
| ---- | ------ | ------- |
| `id` | number | 实体 ID |

### 响应

```json
{
  "success": true,
  "message": "Entity deleted successfully: 123",
  "entity_id": 123,
  "vectors_deleted": 5,
  "bm25_documents_deleted": 5,
  "relations_deleted": 3,
  "elapsed_ms": 30
}
```

**响应字段**:

| 字段                     | 类型    | 描述               |
| ------------------------ | ------- | ------------------ |
| `success`                | boolean | 操作是否成功       |
| `message`                | string  | 结果描述消息       |
| `entity_id`              | number  | 被删除的实体 ID    |
| `vectors_deleted`        | number  | 删除的向量数       |
| `bm25_documents_deleted` | number  | 删除的 BM25 文档数 |
| `relations_deleted`      | number  | 删除的关系数       |
| `elapsed_ms`             | number  | 耗时（毫秒）       |

### 示例

```bash
curl -X DELETE "http://localhost:3000/api/index/entity/123"
```

---

## DELETE /api/index/batch

批量删除文件和实体。

### 请求

**方法**: `DELETE`

**Content-Type**: `application/json`

**请求体**:

```json
{
  "file_paths": ["src/old_file1.rs", "src/old_file2.rs"],
  "entity_ids": [123, 124, 125]
}
```

**请求字段**:

| 字段         | 类型     | 必填 | 默认值 | 描述                 |
| ------------ | -------- | ---- | ------ | -------------------- |
| `file_paths` | string[] | 否   | `[]`   | 要删除的文件路径列表 |
| `entity_ids` | number[] | 否   | `[]`   | 要删除的实体 ID 列表 |

### 响应

```json
{
  "success": true,
  "files_deleted": 2,
  "entities_deleted": 3,
  "errors": [],
  "elapsed_ms": 100
}
```

**响应字段**:

| 字段               | 类型     | 描述         |
| ------------------ | -------- | ------------ |
| `success`          | boolean  | 操作是否成功 |
| `files_deleted`    | number   | 删除的文件数 |
| `entities_deleted` | number   | 删除的实体数 |
| `errors`           | string[] | 错误列表     |
| `elapsed_ms`       | number   | 耗时（毫秒） |

### 示例

```bash
curl -X DELETE "http://localhost:3000/api/index/batch" \
  -H "Content-Type: application/json" \
  -d '{
    "file_paths": ["src/old_file.rs"],
    "entity_ids": [123, 124]
  }'
```

---

## GET /api/index/stats

获取索引统计信息。

### 请求

**方法**: `GET`

### 响应

```json
{
  "success": true,
  "statistics": {
    "total_entities": 5000,
    "total_relations": 1000,
    "total_vectors": 5000,
    "total_bm25_documents": 4500,
    "total_files": 200
  },
  "elapsed_ms": 5
}
```

**响应字段**:

| 字段         | 类型            | 描述         |
| ------------ | --------------- | ------------ |
| `success`    | boolean         | 操作是否成功 |
| `statistics` | IndexStatistics | 索引统计信息 |
| `elapsed_ms` | number          | 耗时（毫秒） |

**IndexStatistics 字段**:

| 字段                   | 类型   | 描述          |
| ---------------------- | ------ | ------------- |
| `total_entities`       | number | 实体总数      |
| `total_relations`      | number | 关系总数      |
| `total_vectors`        | number | 向量总数      |
| `total_bm25_documents` | number | BM25 文档总数 |
| `total_files`          | number | 文件总数      |

```bash
curl "http://localhost:3000/api/index/stats"
```

---

## GET /api/storage/status

获取存储系统状态。

### 请求

**方法**: `GET`

### 响应

```json
{
  "success": true,
  "status": {
    "vector_storage": {
      "connected": true,
      "item_count": 5000,
      "disk_usage_mb": 150.5,
      "version": "1.13.0",
      "last_error": null
    },
    "bm25_storage": {
      "connected": true,
      "item_count": 5000,
      "disk_usage_mb": 50.2,
      "version": null,
      "last_error": null
    },
    "relation_storage": {
      "connected": true,
      "item_count": 1000,
      "disk_usage_mb": 10.3,
      "version": null,
      "last_error": null
    },
    "cache_storage": {
      "connected": true,
      "item_count": 200,
      "disk_usage_mb": 5.1,
      "version": null,
      "last_error": null
    },
    "total_disk_usage_mb": 216.1,
    "process_status": {
      "managed": true,
      "status": "managed",
      "running": true
    }
  }
}
```

**响应字段**:

| 字段      | 类型          | 描述         |
| --------- | ------------- | ------------ |
| `success` | boolean       | 操作是否成功 |
| `status`  | StorageStatus | 存储状态     |

**StorageStatus 字段**:

| 字段                  | 类型                   | 描述                                  |
| --------------------- | ---------------------- | ------------------------------------- |
| `vector_storage`      | StorageComponentStatus | 向量存储状态                          |
| `bm25_storage`        | StorageComponentStatus | BM25 存储状态                         |
| `relation_storage`    | StorageComponentStatus | 关系存储状态                          |
| `cache_storage`       | StorageComponentStatus | 缓存存储状态                          |
| `total_disk_usage_mb` | number                 | 总磁盘使用量（MB）                    |
| `process_status`      | QdrantProcessInfo?     | Qdrant 子进程管理信息（仅启用时返回） |

**StorageComponentStatus 字段**:

| 字段            | 类型    | 描述                         |
| --------------- | ------- | ---------------------------- |
| `connected`     | boolean | 是否已连接                   |
| `item_count`    | number  | 存储的条目数                 |
| `disk_usage_mb` | number  | 磁盘使用量（MB）             |
| `version`       | string? | 服务版本号（仅向量存储支持） |
| `last_error`    | string? | 最后的错误消息               |

**QdrantProcessInfo 字段**:

| 字段      | 类型    | 描述                                  |
| --------- | ------- | ------------------------------------- |
| `managed` | boolean | 是否启用子进程管理                    |
| `status`  | string  | 进程管理状态 (`managed` / `external`) |
| `running` | boolean | 进程是否正在运行                      |

### 示例

```bash
curl "http://localhost:3000/api/storage/status"
```
