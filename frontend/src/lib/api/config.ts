/**
 * Configuration Management API
 * Handles config inspection, reload, and validation.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from '#api/client';
import type { components } from './schema';

export type ConfigInfoResponse = components['schemas']['ConfigInfoResponse'];
export type ConfigReloadResponse =
	components['schemas']['ConfigReloadResponse'];
export type ConfigValidateResponse =
	components['schemas']['ConfigValidateResponse'];
export type ConfigWarningInfo = components['schemas']['ConfigWarningInfo'];
export type ProjectLlmOverviewResponse =
	components['schemas']['ProjectLlmOverviewResponse'];
export type ProjectLlmUpdateRequest =
	components['schemas']['ProjectLlmUpdateRequest'];
export type ProjectLlmUpdateResponse =
	components['schemas']['ProjectLlmUpdateResponse'];
export type LlmTestRequest = components['schemas']['LlmTestRequest'];
export type LlmTestResponse = components['schemas']['LlmTestResponse'];

export const configApi = {
	/** GET /api/config — return current active configuration info */
	getInfo: (): Promise<ConfigInfoResponse> => call(client.GET('/api/config')),

	/** POST /api/config/reload?project_id=N — reload configuration */
	reload: (projectId: number): Promise<ConfigReloadResponse> =>
		call(
			client.POST('/api/config/reload', {
				params: { query: { project_id: projectId } },
			}),
		),

	/** GET /api/config/validate — validate current configuration */
	validate: (): Promise<ConfigValidateResponse> =>
		call(client.GET('/api/config/validate')),
};

export const llmApi = {
	/** GET /api/project/{id}/llm/overview — masked model overview with diagnostics */
	getOverview: (projectId: string): Promise<ProjectLlmOverviewResponse> =>
		call(
			client.GET('/api/project/{project_id}/llm/overview', {
				params: { path: { project_id: Number(projectId) } },
			}),
		),

	/** PUT /api/project/{id}/llm/config — typed chat/rerank update (ephemeral) */
	updateConfig: (
		projectId: string,
		body: ProjectLlmUpdateRequest,
	): Promise<ProjectLlmUpdateResponse> =>
		call(
			client.PUT('/api/project/{project_id}/llm/config', {
				params: { path: { project_id: Number(projectId) } },
				body,
			}),
		),

	/** POST /api/project/{id}/llm/test — connectivity probe with stored config */
	testModel: (
		projectId: string,
		body: LlmTestRequest,
	): Promise<LlmTestResponse> =>
		call(
			client.POST('/api/project/{project_id}/llm/test', {
				params: { path: { project_id: Number(projectId) } },
				body,
			}),
		),
};
