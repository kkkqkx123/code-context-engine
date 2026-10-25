/**
 * Entity API
 * Handles entity queries and relationship exploration.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from '#api/client';
import type { components } from './schema';

export type ParameterInfo = components['schemas']['ParameterInfo'];
export type FunctionInfo = components['schemas']['FunctionInfo'];
export type FunctionDetailResponse =
	components['schemas']['FunctionDetailResponse'];
export type FunctionCallsResponse =
	components['schemas']['FunctionCallsResponse'];
export type FunctionCallersResponse =
	components['schemas']['FunctionCallersResponse'];
export type CallChainResponse = components['schemas']['CallChainResponse'];
export type CallPathResponse = components['schemas']['CallPathResponse'];
export type ClassRelation = components['schemas']['ClassRelation'];
export type ClassInheritanceResponse =
	components['schemas']['ClassInheritanceResponse'];
export type InterfaceRelation = components['schemas']['InterfaceRelation'];
export type ClassImplementationsResponse =
	components['schemas']['ClassImplementationsResponse'];
export type ClassificationStatsResponse =
	components['schemas']['ClassificationStatsResponse'];

/** Relation listing filter shared by calls, callers and call-chain. */
export interface RelationFilter {
	maxDepth?: number;
	offset?: number;
	limit?: number;
	excludeTests?: boolean;
	directoryPrefix?: string;
	excludedFiles?: string[];
	domains?: string;
	includeExternal?: boolean;
}

export const entityApi = {
	// Function details
	getFunction: (
		projectId: number,
		id: string,
	): Promise<FunctionDetailResponse> =>
		call(
			client.GET('/api/project/{project_id}/function/{id}', {
				params: { path: { project_id: projectId, id } },
			}),
		),

	// Functions called by this function
	getCalls: (
		projectId: number,
		id: string,
		filter?: RelationFilter,
	): Promise<FunctionCallsResponse> =>
		call(
			client.GET('/api/project/{project_id}/function/{id}/calls', {
				params: {
					path: { project_id: projectId, id },
					query: {
						max_depth: filter?.maxDepth,
						offset: filter?.offset,
						limit: filter?.limit,
						exclude_tests: filter?.excludeTests,
						directory_prefix: filter?.directoryPrefix,
						excluded_files: filter?.excludedFiles,
						domains: filter?.domains,
						include_external: filter?.includeExternal,
					},
				},
			}),
		),

	// Functions calling this function
	getCallers: (
		projectId: number,
		id: string,
		filter?: RelationFilter,
	): Promise<FunctionCallersResponse> =>
		call(
			client.GET('/api/project/{project_id}/function/{id}/callers', {
				params: {
					path: { project_id: projectId, id },
					query: {
						max_depth: filter?.maxDepth,
						offset: filter?.offset,
						limit: filter?.limit,
						exclude_tests: filter?.excludeTests,
						directory_prefix: filter?.directoryPrefix,
						excluded_files: filter?.excludedFiles,
						domains: filter?.domains,
						include_external: filter?.includeExternal,
					},
				},
			}),
		),

	// Full call chain
	getCallChain: (
		projectId: number,
		id: string,
		direction: 'up' | 'down' = 'down',
		filter?: RelationFilter,
	): Promise<CallChainResponse> =>
		call(
			client.GET('/api/project/{project_id}/call-chain/{id}', {
				params: {
					path: { project_id: projectId, id },
					query: {
						direction,
						max_depth: filter?.maxDepth,
						offset: filter?.offset,
						limit: filter?.limit,
						exclude_tests: filter?.excludeTests,
						directory_prefix: filter?.directoryPrefix,
						excluded_files: filter?.excludedFiles,
						domains: filter?.domains,
						include_external: filter?.includeExternal,
					},
				},
			}),
		),

	// Path between two functions
	getCallPath: (
		projectId: number,
		fromId: string,
		toId: string,
		maxDepth: number = 10,
	): Promise<CallPathResponse> =>
		call(
			client.GET('/api/project/{project_id}/call-path', {
				params: {
					path: { project_id: projectId },
					query: { start_id: fromId, end_id: toId, max_depth: maxDepth },
				},
			}),
		),

	// Class inheritance
	getInheritance: (
		projectId: number,
		id: string,
	): Promise<ClassInheritanceResponse> =>
		call(
			client.GET('/api/project/{project_id}/class/{id}/inheritance', {
				params: { path: { project_id: projectId, id } },
			}),
		),

	// Class implementations
	getImplementations: (
		projectId: number,
		id: string,
	): Promise<ClassImplementationsResponse> =>
		call(
			client.GET('/api/project/{project_id}/class/{id}/implementations', {
				params: { path: { project_id: projectId, id } },
			}),
		),

	// Counts of resolved relations grouped by external call classification
	getClassificationStats: (
		projectId: number,
	): Promise<ClassificationStatsResponse> =>
		call(
			client.GET('/api/project/{project_id}/relations/classification/stats', {
				params: { path: { project_id: projectId } },
			}),
		),

	// Relations in one external call classification category
	getRelationsByClassification: (
		projectId: number,
		classification: string,
		limit?: number,
	): Promise<unknown[]> =>
		call(
			client.GET(
				'/api/project/{project_id}/relations/classification/{classification}',
				{
					params: {
						path: { project_id: projectId, classification },
						query: { limit },
					},
				},
			),
		),
};
