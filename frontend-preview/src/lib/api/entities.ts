/**
 * Entity API
 * Handles entity queries and relationship exploration.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { apiClient } from './client';
import type { components } from './schema';

export type ParameterInfo = components['schemas']['ParameterInfo'];
export type FunctionInfo = components['schemas']['FunctionInfo'];
export type FunctionDetailResponse = components['schemas']['FunctionDetailResponse'];
export type FunctionCallsResponse = components['schemas']['FunctionCallsResponse'];
export type FunctionCallersResponse = components['schemas']['FunctionCallersResponse'];
export type CallChainResponse = components['schemas']['CallChainResponse'];
export type CallPathResponse = components['schemas']['CallPathResponse'];
export type ClassRelation = components['schemas']['ClassRelation'];
export type ClassInheritanceResponse = components['schemas']['ClassInheritanceResponse'];
export type InterfaceRelation = components['schemas']['InterfaceRelation'];
export type ClassImplementationsResponse = components['schemas']['ClassImplementationsResponse'];

export const entityApi = {
	// Function details
	getFunction: (projectId: number, id: string) =>
		apiClient.get<FunctionDetailResponse>(`/api/project/${projectId}/function/${id}`),

	// Functions called by this function
	getCalls: (projectId: number, id: string) =>
		apiClient.get<FunctionCallsResponse>(`/api/project/${projectId}/function/${id}/calls`),

	// Functions calling this function
	getCallers: (projectId: number, id: string) =>
		apiClient.get<FunctionCallersResponse>(`/api/project/${projectId}/function/${id}/callers`),

	// Full call chain
	getCallChain: (projectId: number, id: string, direction: 'up' | 'down' = 'down') =>
		apiClient.get<CallChainResponse>(`/api/project/${projectId}/call-chain/${id}?direction=${direction}`),

	// Path between two functions
	getCallPath: (projectId: number, fromId: string, toId: string, maxDepth: number = 10) =>
		apiClient.get<CallPathResponse>(
			`/api/project/${projectId}/call-path?start_id=${fromId}&end_id=${toId}&max_depth=${maxDepth}`
		),

	// Class inheritance
	getInheritance: (projectId: number, id: string) =>
		apiClient.get<ClassInheritanceResponse>(`/api/project/${projectId}/class/${id}/inheritance`),

	// Class implementations
	getImplementations: (projectId: number, id: string) =>
		apiClient.get<ClassImplementationsResponse>(`/api/project/${projectId}/class/${id}/implementations`),
};
