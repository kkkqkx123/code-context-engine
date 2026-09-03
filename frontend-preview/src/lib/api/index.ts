/**
 * Index Management API
 * Handles project lifecycle and indexing operations
 */

import { apiClient } from "./client";

export interface Project {
  id: string;
  name: string;
  root_path: string;
  extensions?: string[];
  exclude_dirs?: string[];
  respect_gitignore?: boolean;
  ignore_patterns?: string[];
  created_at?: string;
  last_indexed?: string;
}

export interface IndexRequest {
  project_id: number;
  path: string;
  extensions?: string[];
  exclude_dirs?: string[];
  respect_gitignore?: boolean;
  ignore_patterns?: string[];
  custom_gitignore?: string;
}

export interface IncrementalIndexRequest {
  project_id: number;
  files_to_index?: string[];
  files_to_remove?: string[];
  force_reindex?: boolean;
}

export interface ParseResult {
  entities: any[];
  language: string;
  file_path: string;
}

export interface ClearIndexRequest {
  project_id: number;
}

export interface IndexStatsResponse {
  success: boolean;
  statistics: {
    total_entities: number;
    total_relations: number;
    total_vectors: number;
    total_bm25_documents: number;
    total_files: number;
  };
  elapsed_ms: number;
}

export const indexApi = {
  // Full directory indexing
  runIndex: (data: IndexRequest) => apiClient.post("/api/index", data),

  // Incremental indexing
  incrementalIndex: (data: IncrementalIndexRequest) =>
    apiClient.post("/api/index/incremental", data),

  // Single file parse
  parseFile: (filePath: string, language?: string) =>
    apiClient.post("/api/parse", { file_path: filePath, language }),

  // Get index statistics
  getStats: (projectId: number) =>
    apiClient.get<IndexStatsResponse>(`/api/index/stats?project_id=${projectId}`),

  // Clear index
  clearIndex: (projectId: number) =>
    apiClient.delete("/api/index", {
      body: JSON.stringify({ project_id: projectId } as ClearIndexRequest),
    }),
};

export const projectApi = {
  // List all projects
  listProjects: () =>
    apiClient.get<{ success: boolean; projects: Project[]; total: number }>(
      "/api/project",
    ),

  // Get project details
  getProject: (id: string) =>
    apiClient.get<{ success: boolean; project: Project }>(`/api/project/${id}`),

  // Create new project
  createProject: (data: {
    root_path: string;
    name?: string;
    extensions?: string[];
    exclude_dirs?: string[];
    respect_gitignore?: boolean;
    ignore_patterns?: string[];
  }) =>
    apiClient.post<{ success: boolean; project: Project }>(
      "/api/project",
      data,
    ),

  // Update project
  updateProject: (
    id: string,
    data: {
      name?: string;
      extensions?: string[];
      exclude_dirs?: string[];
      respect_gitignore?: boolean;
      ignore_patterns?: string[];
    },
  ) =>
    apiClient.put<{ success: boolean; project: Project }>(
      `/api/project/${id}`,
      data,
    ),

  // Delete project
  deleteProject: (id: string) =>
    apiClient.delete<{ success: boolean }>(`/api/project/${id}`),

  // Trigger project indexing
  indexProject: (id: string) =>
    apiClient.post<{ success: boolean }>(`/api/project/${id}/index`),

  // Reload project configuration from file system
  reloadProject: (id: string) =>
    apiClient.post<{ success: boolean }>(`/api/project/${id}/reload`),

  // Update project configuration
  updateProjectConfig: (id: string, config: Record<string, unknown>) =>
    apiClient.put<{ success: boolean }>(`/api/project/${id}/config`, config),
};