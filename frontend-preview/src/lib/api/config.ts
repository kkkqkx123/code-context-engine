/**
 * Configuration Management API
 * Handles config inspection, reload, and validation
 */

import { apiClient } from './client';

export interface ConfigInfoResponse {
	initialized: boolean;
	database: Record<string, unknown>;
	embedder: Record<string, unknown>;
	project_count: number;
}

export interface ConfigReloadResponse {
	success: boolean;
	message: string;
}

export interface ConfigValidateResponse {
	valid: boolean;
	errors: string[];
	warnings: string[];
	dependency_warnings: Array<{
		level: string;
		message: string;
		module: string;
	}>;
}

export const configApi = {
	/** GET /api/config — return current active configuration info */
	getInfo: () =>
		apiClient.get<ConfigInfoResponse>('/api/config'),

	/** POST /api/config/reload?project_id=N — reload configuration */
	reload: (projectId: number) =>
		apiClient.post<ConfigReloadResponse>(`/api/config/reload?project_id=${projectId}`),

	/** GET /api/config/validate — validate current configuration */
	validate: () =>
		apiClient.get<ConfigValidateResponse>('/api/config/validate'),
};