/**
 * Configuration Management API
 * Handles config inspection, reload, and validation.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from './client';
import type { components } from './schema';

export type ConfigInfoResponse = components['schemas']['ConfigInfoResponse'];
export type ConfigReloadResponse = components['schemas']['ConfigReloadResponse'];
export type ConfigValidateResponse = components['schemas']['ConfigValidateResponse'];
export type ConfigWarningInfo = components['schemas']['ConfigWarningInfo'];

export const configApi = {
	/** GET /api/config — return current active configuration info */
	getInfo: (): Promise<ConfigInfoResponse> => call(client.GET('/api/config')),

	/** POST /api/config/reload?project_id=N — reload configuration */
	reload: (projectId: number): Promise<ConfigReloadResponse> =>
		call(
			client.POST('/api/config/reload', {
				params: { query: { project_id: projectId } }
			})
		),

	/** GET /api/config/validate — validate current configuration */
	validate: (): Promise<ConfigValidateResponse> => call(client.GET('/api/config/validate'))
};
