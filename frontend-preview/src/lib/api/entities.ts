/**
 * Entity API
 * Handles entity queries and relationship exploration
 */

import { apiClient } from './client';
import type { CallChainNode } from './search';

export interface ParameterInfo {
	name: string;
	type_name?: string;
}

export interface FunctionInfo {
	id: string;
	name: string;
	signature: string;
	parameters: ParameterInfo[];
	return_type?: string;
	file_path: string;
	start_line: number;
	end_line: number;
	doc_comment?: string;
}

export interface FunctionDetailResponse {
	success: boolean;
	function: FunctionInfo;
}

export interface FunctionCallsResponse {
	success: boolean;
	relation_epoch: number;
	function_id: string;
	function_name: string;
	callees: CallChainNode[];
	total_callees: number;
}

export interface FunctionCallersResponse {
	success: boolean;
	relation_epoch: number;
	function_id: string;
	function_name: string;
	callers: CallChainNode[];
	total_callers: number;
}

export interface CallChainResponse {
	success: boolean;
	relation_epoch: number;
	function_id: string;
	function_name: string;
	direction: string;
	call_chain: CallChainNode[];
}

export interface CallPathResponse {
	success: boolean;
	relation_epoch: number;
	start_function_id: string;
	end_function_id: string;
	path_found: boolean;
	path: CallChainNode[];
	path_length: number;
}

export interface ClassRelation {
	class_id: string;
	class_name: string;
	file_path: string;
	depth: number;
}

export interface ClassInheritanceResponse {
	success: boolean;
	relation_epoch: number;
	class_id: string;
	class_name: string;
	base_classes: ClassRelation[];
	derived_classes: ClassRelation[];
}

export interface InterfaceRelation {
	interface_id: string;
	interface_name: string;
	file_path: string;
}

export interface ClassImplementationsResponse {
	success: boolean;
	relation_epoch: number;
	class_id: string;
	class_name: string;
	implemented_interfaces: InterfaceRelation[];
	implementing_classes: ClassRelation[];
}

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