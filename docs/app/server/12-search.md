# Search 域 HTTP 端点
> 操作数：3
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/entities/search` | `handle_entity_search` | body (EntitySearchRequest) |
| POST | `/api/search` | `handle_search` | body (SearchRequest) |
| POST | `/api/search/aggregated` | `handle_aggregated_search` | body (AggregatedSearchRequest) |
