import { BASE_URL } from '#api/client';

export interface LogEntry {
	timestamp: string;
	level: string;
	target: string;
	message: string;
	fields: Record<string, unknown>;
}

export interface LogsResponse {
	entries: LogEntry[];
	total: number;
	page: number;
	page_size: number;
}

export interface LogFileInfo {
	name: string;
	size: number;
	modified: string;
}

export interface LogConfig {
	level: string;
	output: string;
	format: string;
	file: string | null;
	filter: string | null;
	max_file_size_mb: number | null;
	max_files: number | null;
	max_total_size_mb: number | null;
	enable_http_api: boolean;
	ws_capacity: number;
	memory_buffer_size: number;
}

export const logsApi = {
	async getLogs(params: {
		level?: string;
		module?: string;
		query?: string;
		start_time?: string;
		end_time?: string;
		page?: number;
		page_size?: number;
	}): Promise<LogsResponse> {
		const qs = new URLSearchParams();
		if (params.level) qs.set('level', params.level);
		if (params.module) qs.set('module', params.module);
		if (params.query) qs.set('query', params.query);
		if (params.start_time) qs.set('start_time', params.start_time);
		if (params.end_time) qs.set('end_time', params.end_time);
		if (params.page) qs.set('page', String(params.page));
		if (params.page_size) qs.set('page_size', String(params.page_size));
		const res = await fetch(`${BASE_URL}/api/logs?${qs.toString()}`);
		if (!res.ok) throw new Error(`HTTP ${res.status}`);
		return res.json();
	},

	async listFiles(): Promise<LogFileInfo[]> {
		const res = await fetch(`${BASE_URL}/api/logs/files`);
		if (!res.ok) throw new Error(`HTTP ${res.status}`);
		return res.json();
	},

	downloadUrl(name: string): string {
		return `${BASE_URL}/api/logs/files/${encodeURIComponent(name)}/download`;
	},

	async deleteFile(name: string): Promise<void> {
		const res = await fetch(
			`${BASE_URL}/api/logs/files/${encodeURIComponent(name)}`,
			{ method: 'DELETE' },
		);
		if (!res.ok) throw new Error(`HTTP ${res.status}`);
	},

	async rotate(): Promise<void> {
		const res = await fetch(`${BASE_URL}/api/logs/rotate`, { method: 'POST' });
		if (!res.ok) throw new Error(`HTTP ${res.status}`);
	},

	async getConfig(): Promise<LogConfig> {
		const res = await fetch(`${BASE_URL}/api/logs/config`);
		if (!res.ok) throw new Error(`HTTP ${res.status}`);
		return res.json();
	},

	async updateConfig(data: {
		level?: string;
		format?: string;
		filter?: string;
	}): Promise<void> {
		const res = await fetch(`${BASE_URL}/api/logs/config`, {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(data),
		});
		if (!res.ok) throw new Error(`HTTP ${res.status}`);
	},
};

export function wsLogsUrl(): string {
	const base = BASE_URL || window.location.origin;
	const url = new URL('/ws/logs', base);
	url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
	return url.toString();
}
