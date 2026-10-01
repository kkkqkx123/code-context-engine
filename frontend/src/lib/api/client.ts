/**
 * Typed API client built on openapi-fetch.
 * Paths, methods, params and bodies are checked against the generated
 * OpenAPI contract (schema.d.ts); success payloads are the bare backend
 * types, errors throw ApiError.
 */

import createClient from 'openapi-fetch';
import type { paths } from './schema';

export const BASE_URL =
	import.meta.env.VITE_API_BASE_URL || 'http://localhost:9000';

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
	}>,
): Promise<T> {
	const res = await promise;
	if (res.error !== undefined && res.error !== null) {
		throw new ApiError(
			errorMessage(res.error, res.response?.status ?? 0),
			res.response?.status ?? 0,
		);
	}
	return res.data as T;
}

export const client = createClient<paths>({
	baseUrl: BASE_URL,
	headers: { 'Content-Type': 'application/json' },
});
