# Index 域 HTTP 端点
> 操作数：3
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/index` | `handle_index` | body (IndexRequest) |
| POST | `/api/index/incremental` | `handle_incremental` | body (IncrementalIndexRequest) |
| POST | `/api/parse` | `handle_parse` | body (ParseRequest) |
