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
	/** Backend error code (e.g. AMBIGUOUS_SYMBOL), when present. */
	code: string | null;
	/** Raw backend error payload, for structured details such as candidates. */
	payload: unknown;

	constructor(message: string, status: number, code?: string, payload?: unknown) {
		super(message);
		this.name = 'ApiError';
		this.status = status;
		this.code = code ?? null;
		this.payload = payload;
	}

	/** Parse `details` as a JSON array of symbol candidates (AMBIGUOUS_SYMBOL). */
	symbolCandidates(): SymbolCandidate[] {
		const detail = (this.payload as BackendError | undefined)?.error?.details;
		if (typeof detail !== 'string' || detail.length === 0) return [];
		try {
			const parsed = JSON.parse(detail);
			if (!Array.isArray(parsed)) return [];
			return parsed.filter(
				(c): c is SymbolCandidate =>
					c && typeof c === 'object' && typeof (c as SymbolCandidate).stable_id === 'string',
			);
		} catch {
			return [];
		}
	}
}

/** A disambiguation candidate returned for an AMBIGUOUS_SYMBOL error. */
export interface SymbolCandidate {
	stable_id: string;
	file_path: string;
	scoped_name: string;
	kind: string;
}

/** Error payload returned by the backend on failure. */
interface BackendError {
	error?: {
		code?: string;
		message?: string;
		details?: string;
	};
	message?: string;
}

function errorPayload(error: unknown): BackendError | undefined {
	return typeof error === 'object' && error !== null
		? (error as BackendError)
		: undefined;
}

function errorCode(error: unknown): string | undefined {
	return errorPayload(error)?.error?.code;
}

function errorMessage(error: unknown, status: number): string {
	const body = errorPayload(error);
	if (typeof body?.error?.message === 'string' && body.error.message) {
		return body.error.message;
	}
	if (typeof body?.message === 'string' && body.message) {
		return body.message;
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
		const status = res.response?.status ?? 0;
		throw new ApiError(
			errorMessage(res.error, status),
			status,
			errorCode(res.error),
			res.error,
		);
	}
	return res.data as T;
}

export const client = createClient<paths>({
	baseUrl: BASE_URL,
	headers: { 'Content-Type': 'application/json' },
});
