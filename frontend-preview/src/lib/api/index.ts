/**
 * Index Management API
 * Handles project lifecycle and indexing operations.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { apiClient } from "./client";
import type { components } from "./schema";

export type Project = components["schemas"]["ProjectConfig"];
export type IndexRequest = components["schemas"]["IndexRequest"];
export type IndexResponse = components["schemas"]["IndexResponse"];
export type IncrementalIndexRequest = components["schemas"]["IncrementalIndexRequest"];
export type IncrementalIndexResponse = components["schemas"]["IncrementalIndexResponse"];
export type ParseResult = components["schemas"]["ParseResponse"];
export type ClearIndexRequest = components["schemas"]["ClearIndexRequest"];
export type ClearIndexResponse = components["schemas"]["ClearIndexResponse"];
export type IndexStatsResponse = components["schemas"]["IndexStatsResponse"];
export type DeleteFileResponse = components["schemas"]["DeleteFileResponse"];
export type DeleteEntityResponse = components["schemas"]["DeleteEntityResponse"];
export type BatchDeleteRequest = components["schemas"]["BatchDeleteRequest"];
export type BatchDeleteResponse = components["schemas"]["BatchDeleteResponse"];
export type CreateProjectRequest = components["schemas"]["CreateProjectRequest"];
export type UpdateProjectRequest = components["schemas"]["UpdateProjectRequest"];
export type ProjectListResponse = components["schemas"]["ProjectListResponse"];
export type ProjectDetailResponse = components["schemas"]["ProjectDetailResponse"];
export type ProjectDeleteResponse = components["schemas"]["ProjectDeleteResponse"];
export type ProjectIndexResponse = components["schemas"]["ProjectIndexResponse"];
export type ProjectConfigReloadResponse = components["schemas"]["ProjectConfigReloadResponse"];
export type ProjectConfigUpdateRequest = components["schemas"]["ProjectConfigUpdateRequest"];
export type ProjectConfigUpdateResponse = components["schemas"]["ProjectConfigUpdateResponse"];

export const indexApi = {
  // Full directory indexing
  runIndex: (data: IndexRequest) =>
    apiClient.post<IndexResponse>("/api/index", data),

  // Incremental indexing
  incrementalIndex: (data: IncrementalIndexRequest) =>
    apiClient.post<IncrementalIndexResponse>("/api/index/incremental", data),

  // Single file parse (language is auto-detected by the backend)
  parseFile: (filePath: string) =>
    apiClient.post<ParseResult>("/api/parse", { file_path: filePath }),

  // Get index statistics
  getStats: (projectId: number) =>
    apiClient.get<IndexStatsResponse>(`/api/index/stats?project_id=${projectId}`),

  // Clear index
  clearIndex: (projectId: number) =>
    apiClient.delete<ClearIndexResponse>("/api/index", {
      body: JSON.stringify({ project_id: projectId } as ClearIndexRequest),
    }),

  // Delete file from all backends
  deleteFile: (filePath: string, projectId: number) =>
    apiClient.delete<DeleteFileResponse>(
      `/api/index/file/${encodeURIComponent(filePath)}?project_id=${projectId}`,
    ),

  // Delete entity from all backends
  deleteEntity: (entityId: number, projectId: number) =>
    apiClient.delete<DeleteEntityResponse>(
      `/api/index/entity/${entityId}?project_id=${projectId}`,
    ),

  // Batch delete files and entities
  batchDelete: (projectId: number, data: BatchDeleteRequest) =>
    apiClient.delete<BatchDeleteResponse>(
      `/api/index/batch?project_id=${projectId}`,
      { body: JSON.stringify(data) },
    ),
};

export const projectApi = {
  // List all projects
  listProjects: () => apiClient.get<ProjectListResponse>("/api/project"),

  // Get project details
  getProject: (id: string) =>
    apiClient.get<ProjectDetailResponse>(`/api/project/${id}`),

  // Create new project
  createProject: (data: CreateProjectRequest) =>
    apiClient.post<ProjectDetailResponse>("/api/project", data),

  // Update project
  updateProject: (id: string, data: UpdateProjectRequest) =>
    apiClient.put<ProjectDetailResponse>(`/api/project/${id}`, data),

  // Delete project
  deleteProject: (id: string) =>
    apiClient.delete<ProjectDeleteResponse>(`/api/project/${id}`),

  // Trigger project indexing
  indexProject: (id: string) =>
    apiClient.post<ProjectIndexResponse>(`/api/project/${id}/index`),

  // Reload project configuration from file system
  reloadProject: (id: string) =>
    apiClient.post<ProjectConfigReloadResponse>(`/api/project/${id}/reload`),

  // Update project configuration
  updateProjectConfig: (id: string, config: Record<string, unknown>) =>
    apiClient.put<ProjectConfigUpdateResponse>(`/api/project/${id}/config`, {
      config,
    } as ProjectConfigUpdateRequest),
};
