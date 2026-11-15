# Ingest 域 HTTP 端点
> 操作数：4
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/project/{project_id}/ingest/batch`  (admission) | `handle_ingest_batch` | project_id (p), body (IngestBatchRequest) |
| POST | `/api/project/{project_id}/ingest/commit`  (admission) | `handle_ingest_commit` | project_id (p) |
| POST | `/api/project/{project_id}/ingest/event`  (admission) | `handle_ingest_event` | project_id (p), body (IngestEventRequest) |
| POST | `/api/project/{project_id}/ingest/manifest`  (admission) | `handle_ingest_manifest` | project_id (p), body (IngestManifestRequest) |
