# Project 域 HTTP 端点
> 操作数：15
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/project` | `handle_list_projects` |  |
| POST | `/api/project` | `handle_create_project` | body (CreateProjectRequest) |
| DELETE | `/api/project/{project_id}` | `handle_delete_project` | project_id (p) |
| GET | `/api/project/{project_id}` | `handle_get_project` | project_id (p) |
| PUT | `/api/project/{project_id}` | `handle_update_project` | project_id (p), body (UpdateProjectRequest) |
| GET | `/api/project/{project_id}/config` | `handle_get_project_config` | project_id (p) |
| PUT | `/api/project/{project_id}/config` | `handle_update_project_config` | project_id (p), body (ProjectConfigUpdateRequest) |
| GET | `/api/project/{project_id}/dead-letters` | `handle_dead_letter_list` | project_id (p) |
| POST | `/api/project/{project_id}/dead-letters/acknowledge` | `handle_dead_letter_acknowledge` | project_id (p), body (DeadLetterAcknowledgeRequest) |
| POST | `/api/project/{project_id}/dead-letters/retry` | `handle_dead_letter_files_retry` | project_id (p), body (DeadLetterRetryRequest) |
| POST | `/api/project/{project_id}/index` | `handle_project_index` | project_id (p) |
| PUT | `/api/project/{project_id}/llm/config` | `handle_update_project_llm_config` | project_id (p), body (ProjectLlmUpdateRequest) |
| GET | `/api/project/{project_id}/llm/overview` | `handle_get_project_llm_overview` | project_id (p) |
| POST | `/api/project/{project_id}/llm/test` | `handle_test_project_llm` | project_id (p), body (LlmTestRequest) |
| POST | `/api/project/{project_id}/reload` | `handle_reload_project_config` | project_id (p) |
