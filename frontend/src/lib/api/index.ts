/**
 * Index Management API
 * Handles project lifecycle and indexing operations.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from '#api/client';
import type { components } from './schema';

export type Project = components['schemas']['ProjectConfig'];
export type IncrementalIndexRequest =
	components['schemas']['IncrementalIndexRequest'];
export type IncrementalIndexResponse =
	components['schemas']['IncrementalIndexResponse'];
export type ParseRequest = components['schemas']['ParseRequest'];
export type ParseResult = components['schemas']['ParseResponse'];
export type ClearIndexRequest = components['schemas']['ClearIndexRequest'];
export type ClearIndexResponse = components['schemas']['ClearIndexResponse'];
export type IndexStatsResponse = components['schemas']['IndexStatsResponse'];
export type DeleteFileResponse = components['schemas']['DeleteFileResponse'];
export type DeleteEntityResponse =
	components['schemas']['DeleteEntityResponse'];
export type BatchDeleteRequest = components['schemas']['BatchDeleteRequest'];
export type BatchDeleteResponse = components['schemas']['BatchDeleteResponse'];
export type CreateProjectRequest =
	components['schemas']['CreateProjectRequest'];
export type UpdateProjectRequest =
	components['schemas']['UpdateProjectRequest'];
export type ProjectListResponse = components['schemas']['ProjectListResponse'];
export type ProjectDetailResponse =
	components['schemas']['ProjectDetailResponse'];
export type ProjectDeleteResponse =
	components['schemas']['ProjectDeleteResponse'];
export type ProjectIndexResponse =
	components['schemas']['ProjectIndexResponse'];
export type ProjectConfigReloadResponse =
	components['schemas']['ProjectConfigReloadResponse'];
export type ProjectConfigUpdateRequest =
	components['schemas']['ProjectConfigUpdateRequest'];
export type ProjectConfigUpdateResponse =
	components['schemas']['ProjectConfigUpdateResponse'];
export type ProjectConfigGetResponse =
	components['schemas']['ProjectConfigGetResponse'];

export const indexApi = {
	// Incremental indexing
	incrementalIndex: (
		data: IncrementalIndexRequest,
	): Promise<IncrementalIndexResponse> =>
		call(client.POST('/api/index/incremental', { body: data })),

	// Single file parse (language is auto-detected by the backend)
	parseFile: (filePath: string): Promise<ParseResult> =>
		call(client.POST('/api/parse', { body: { file_path: filePath } })),

	// Get index statistics
	getStats: (projectId: number): Promise<IndexStatsResponse> =>
		call(
			client.GET('/api/index/stats', {
				params: { query: { project_id: projectId } },
			}),
		),

	// Clear index
	clearIndex: (projectId: number): Promise<ClearIndexResponse> =>
		call(
			client.DELETE('/api/index', {
				body: { project_id: projectId },
			}),
		),

	// Delete file from all backends
	deleteFile: (
		filePath: string,
		projectId: number,
	): Promise<DeleteFileResponse> =>
		call(
			client.DELETE('/api/index/file/{path}', {
				params: {
					path: { path: filePath },
					query: { project_id: projectId },
				},
			}),
		),

	// Delete entity from all backends
	deleteEntity: (
		entityId: number,
		projectId: number,
	): Promise<DeleteEntityResponse> =>
		call(
			client.DELETE('/api/index/entity/{id}', {
				params: {
					path: { id: entityId },
					query: { project_id: projectId },
				},
			}),
		),

	// Batch delete files and entities
	batchDelete: (
		projectId: number,
		data: BatchDeleteRequest,
	): Promise<BatchDeleteResponse> =>
		call(
			client.DELETE('/api/index/batch', {
				params: { query: { project_id: projectId } },
				body: data,
			}),
		),
};

export const projectApi = {
	// List all projects
	listProjects: (): Promise<ProjectListResponse> =>
		call(client.GET('/api/project')),

	// Get project details
	getProject: (id: string): Promise<ProjectDetailResponse> =>
		call(
			client.GET('/api/project/{project_id}', {
				params: { path: { project_id: Number(id) } },
			}),
		),

	// Create new project
	createProject: (data: CreateProjectRequest): Promise<ProjectDetailResponse> =>
		call(client.POST('/api/project', { body: data })),

	// Update project
	updateProject: (
		id: string,
		data: UpdateProjectRequest,
	): Promise<ProjectDetailResponse> =>
		call(
			client.PUT('/api/project/{project_id}', {
				params: { path: { project_id: id } },
				body: data,
			}),
		),

	// Delete project
	deleteProject: (id: string): Promise<ProjectDeleteResponse> =>
		call(
			client.DELETE('/api/project/{project_id}', {
				params: { path: { project_id: Number(id) } },
			}),
		),

	// Trigger project indexing
	indexProject: (id: string): Promise<ProjectIndexResponse> =>
		call(
			client.POST('/api/project/{project_id}/index', {
				params: { path: { project_id: Number(id) } },
			}),
		),

	// Reload project configuration from file system
	reloadProject: (id: string): Promise<ProjectConfigReloadResponse> =>
		call(
			client.POST('/api/project/{project_id}/reload', {
				params: { path: { project_id: Number(id) } },
			}),
		),

	// Read merged project configuration (global defaults plus project overrides)
	getProjectConfig: (id: string): Promise<ProjectConfigGetResponse> =>
		call(
			client.GET('/api/project/{project_id}/config', {
				params: { path: { project_id: Number(id) } },
			}),
		),

	// Update project configuration (full runtime config document)
	updateProjectConfig: (
		id: string,
		config: Record<string, unknown>,
	): Promise<ProjectConfigUpdateResponse> =>
		call(
			client.PUT('/api/project/{project_id}/config', {
				params: { path: { project_id: Number(id) } },
				body: { config },
			}),
		),
};
