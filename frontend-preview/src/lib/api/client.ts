/**
 * Preview API client: same typed surface as the main app (`client` with
 * GET/POST/PUT/DELETE plus `call`/`ApiError`/`BASE_URL`).
 *
 * In mock mode (VITE_USE_MOCK=true) requests are answered from the static
 * payloads in `../mock/client`; otherwise they delegate to the real
 * openapi-fetch client. This file is preview-only and is never overwritten
 * by `scripts/sync-frontend-preview.sh`.
 */

import createClient from 'openapi-fetch';
import type { paths } from './schema';
import { isMockMode, mockDelay } from '../mock/data';
import { mockClient } from '../mock/client';

export const BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:9000';

export class ApiError extends Error {
	status: number;

	constructor(message: string, status: number) {
		super(message);
		this.name = 'ApiError';
		this.status = status;
	}
}

/** Error payload returned by the backend on failure. */
interface BackendError {
	error?: {
		code?: string;
		message?: string;
	};
	message?: string;
}

function errorMessage(error: unknown, status: number): string {
	if (error && typeof error === 'object') {
		const body = error as BackendError;
		if (typeof body.error?.message === 'string' && body.error.message) {
			return body.error.message;
		}
		if (typeof body.message === 'string' && body.message) {
			return body.message;
		}
	}
	return `HTTP ${status}`;
}

/**
 * Await an openapi-fetch call: throw ApiError on transport or backend
 * failure, otherwise return the bare success payload.
 */
export async function call<T>(
	promise: Promise<{
		data?: unknown;
		error?: unknown;
		response?: Response;
	}>
): Promise<T> {
	const res = await promise;
	if (res.error !== undefined && res.error !== null) {
		throw new ApiError(errorMessage(res.error, res.response?.status ?? 0), res.response?.status ?? 0);
	}
	return res.data as T;
}

const realClient = createClient<paths>({
	baseUrl: BASE_URL,
	headers: { 'Content-Type': 'application/json' }
});

interface CallOptions {
	params?: {
		path?: Record<string, string | number>;
		query?: Record<string, unknown>;
	};
	body?: unknown;
}

/** Fill a `{param}` path template with concrete values. */
function concretePath(template: string, pathParams?: Record<string, string | number>): string {
	let out = template;
	for (const [key, value] of Object.entries(pathParams ?? {})) {
		out = out.replace(`{${key}}`, encodeURIComponent(String(value)));
	}
	return out;
}

/**
 * Dispatch one call: mock data in mock mode, real backend otherwise.
 * Mock handlers ignore query values and return static payloads.
 */
async function dispatch(
	method: 'GET' | 'POST' | 'PUT' | 'DELETE',
	path: string,
	opts?: CallOptions
): Promise<{ data?: unknown; error?: unknown }> {
	const real = (realClient as unknown as Record<string, (p: string, o?: object) => Promise<unknown>>)[method];
	if (!isMockMode) {
		return (await real(path, opts ?? {})) as { data?: unknown; error?: unknown };
	}
	await mockDelay();
	const endpoint = concretePath(path, opts?.params?.path);
	const mock = mockClient as unknown as Record<string, (e: string, b?: unknown) => Promise<unknown>>;
	const handler = mock[method.toLowerCase()] ?? mock.get;
	const data = await handler.call(mockClient, endpoint, opts?.body);
	return { data };
}

export const client = {
	GET: (path: string, opts?: CallOptions): Promise<{ data?: unknown; error?: unknown }> =>
		dispatch('GET', path, opts),
	POST: (path: string, opts?: CallOptions): Promise<{ data?: unknown; error?: unknown }> =>
		dispatch('POST', path, opts),
	PUT: (path: string, opts?: CallOptions): Promise<{ data?: unknown; error?: unknown }> =>
		dispatch('PUT', path, opts),
	DELETE: (path: string, opts?: CallOptions): Promise<{ data?: unknown; error?: unknown }> =>
		dispatch('DELETE', path, opts)
};
