/**
 * Index Management API
 * Handles project lifecycle and indexing operations.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from './client';
import type { components } from './schema';

export type Project = components['schemas']['ProjectConfig'];
export type IndexRequest = components['schemas']['IndexRequest'];
export type IndexResponse = components['schemas']['IndexResponse'];
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

export const indexApi = {
	// Full directory indexing
	runIndex: (data: IndexRequest): Promise<IndexResponse> =>
		call(client.POST('/api/index', { body: data })),

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
			client.GET('/api/project/{id}', {
				params: { path: { id } },
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
			client.PUT('/api/project/{id}', {
				params: { path: { id } },
				body: data,
			}),
		),

	// Delete project
	deleteProject: (id: string): Promise<ProjectDeleteResponse> =>
		call(
			client.DELETE('/api/project/{id}', {
				params: { path: { id } },
			}),
		),

	// Trigger project indexing
	indexProject: (id: string): Promise<ProjectIndexResponse> =>
		call(
			client.POST('/api/project/{id}/index', {
				params: { path: { id } },
			}),
		),

	// Reload project configuration from file system
	reloadProject: (id: string): Promise<ProjectConfigReloadResponse> =>
		call(
			client.POST('/api/project/{id}/reload', {
				params: { path: { id } },
			}),
		),

	// Update project configuration
	updateProjectConfig: (
		id: string,
		config: Record<string, unknown>,
	): Promise<ProjectConfigUpdateResponse> =>
		call(
			client.PUT('/api/project/{id}/config', {
				params: { path: { id } },
				body: { config },
			}),
		),
};
