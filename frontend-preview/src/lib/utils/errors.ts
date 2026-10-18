/**
 * Shared error handling utilities.
 *
 * All stores grade backend and transport failures through the helpers here
 * so banners, toasts, and machine-readable codes stay consistent. Domain
 * stores may attach extra details (e.g. symbol candidates) but must reuse
 * `errorCodeOf` for the code and `errorMessage` for the text.
 */

/** Minimal structural view of a backend failure, without importing the client. */
interface FailureShape {
	message?: unknown;
	code?: unknown;
	status?: unknown;
}

function asFailure(error: unknown): FailureShape | null {
	return typeof error === 'object' && error !== null
		? (error as FailureShape)
		: null;
}

/** Map an HTTP status to a stable machine-readable code. */
export function statusToErrorCode(status: number): string {
	if (status === 401 || status === 403) return 'UNAUTHENTICATED';
	if (status === 404) return 'NOT_FOUND';
	if (status >= 400 && status < 500) return 'INVALID_ARGUMENT';
	if (status >= 500) return 'SERVER_ERROR';
	if (status === 0) return 'NETWORK';
	return 'UNKNOWN';
}

/**
 * Extract the most specific machine-readable code: backend error code first,
 * then the status-derived code, otherwise UNKNOWN.
 */
export function errorCodeOf(error: unknown): string {
	const failure = asFailure(error);
	const code = failure?.code;
	if (typeof code === 'string' && code.length > 0) return code;
	const status = failure?.status;
	if (typeof status === 'number') return statusToErrorCode(status);
	return 'UNKNOWN';
}

/** Extract a readable message from an unknown thrown value. */
export function errorMessage(error: unknown): string {
	const failure = asFailure(error);
	const message = failure?.message;
	if (typeof message === 'string' && message.length > 0) {
		if (typeof failure?.status === 'number' && failure.status === 401) {
			return `${message} (check the API token)`;
		}
		return message;
	}
	if (error instanceof Error) return error.message;
	return String(error);
}
