/**
 * Configuration Management API
 * Handles config inspection, reload, and validation.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { apiClient } from './client';
import type { components } from './schema';

export type ConfigInfoResponse = components['schemas']['ConfigInfoResponse'];
export type ConfigReloadResponse = components['schemas']['ConfigReloadResponse'];
export type ConfigValidateResponse = components['schemas']['ConfigValidateResponse'];
export type ConfigWarningInfo = components['schemas']['ConfigWarningInfo'];

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
