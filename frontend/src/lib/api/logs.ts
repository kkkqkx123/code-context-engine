/**
 * Logs API
 * In-memory log buffer, log files and logger settings.
 * Wire types come from the generated OpenAPI contract (schema.d.ts), so this
 * module no longer hand-parses payloads or builds URLs beyond the two
 * non-JSON cases called out below.
 */

import { ApiError, BASE_URL, call, client } from '#api/client';
import type { components } from './schema';

export type LogEntry = components['schemas']['LogEntry'];
export type LogsResponse = components['schemas']['LogsResponse'];
export type LogFileInfo = components['schemas']['LogFileInfo'];
export type LogConfig = components['schemas']['LogConfig'];
export type UpdateLogConfigRequest =
	components['schemas']['UpdateLogConfigRequest'];

export type LogsQuery = {
	level?: string;
	module?: string;
	query?: string;
	start_time?: string;
	end_time?: string;
	page?: number;
	page_size?: number;
};

export const logsApi = {
	// One page of the captured buffer, filtered server side
	getLogs: (params: LogsQuery): Promise<LogsResponse> =>
		call(client.GET('/api/logs', { params: { query: params } })),

	// Files present in the configured log directory
	listFiles: (): Promise<LogFileInfo[]> => call(client.GET('/api/logs/files')),

	// Non-JSON exception: the download answers with an opaque attachment, so
	// there is no typed payload to return. The URL still comes from the
	// contract path, rendered here because the browser downloads it directly
	// instead of the client buffering the bytes.
	downloadUrl(name: string): string {
		return `${BASE_URL}/api/logs/files/${encodeURIComponent(name)}/download`;
	},

	// Remove one log file; the server refuses the active file with 403
	deleteFile: async (name: string): Promise<void> => {
		const res = await client.DELETE('/api/logs/files/{name}', {
			params: { path: { name } },
		});
		if (res.error !== undefined && res.error !== null) {
			throw new ApiError(
				'Log file could not be deleted',
				res.response?.status ?? 0,
				undefined,
				res.error,
			);
		}
	},

	// Apply the retention policy now
	rotate: async (): Promise<void> => {
		const res = await client.POST('/api/logs/rotate');
		if (res.error !== undefined && res.error !== null) {
			throw new ApiError(
				'Log retention could not be applied',
				res.response?.status ?? 0,
				undefined,
				res.error,
			);
		}
	},

	// Effective logger configuration of the running process
	getConfig: (): Promise<LogConfig> => call(client.GET('/api/logs/config')),

	// Change level, format or filter of the running logger
	updateConfig: async (data: UpdateLogConfigRequest): Promise<void> => {
		const res = await client.PUT('/api/logs/config', { body: data });
		if (res.error !== undefined && res.error !== null) {
			throw new ApiError(
				'Logger settings could not be saved',
				res.response?.status ?? 0,
				undefined,
				res.error,
			);
		}
	},
};

// Non-JSON exception: the realtime viewer needs a WebSocket URL, which is not
// an HTTP operation, so it stays outside the typed client.
export function wsLogsUrl(): string {
	const base = BASE_URL || window.location.origin;
	const url = new URL('/ws/logs', base);
	url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
	return url.toString();
}
