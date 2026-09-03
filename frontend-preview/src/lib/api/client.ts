/**
 * API Client - Fetch wrapper with base URL configuration
 * Uses native Fetch API with error handling and type safety
 * Supports mock mode when VITE_USE_MOCK=true
 */

import { isMockMode } from '../mock/data';
import { mockClient } from '../mock/client';

const BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:9000';

export interface ApiError {
	message: string;
	status: number;
}

/** Response shape with a success field used by many API endpoints */
export interface ApiSuccessResponse<T = unknown> {
	success: boolean;
	result?: T;
	error?: string;
}

/**
 * Unwrap a response that has { success, result, error } shape.
 * If success is false, throws an ApiError with the error message.
 * Otherwise returns the full response (caller can access .result).
 */
export function unwrapResponse<T>(response: ApiSuccessResponse<T>): ApiSuccessResponse<T> {
	if (!response.success) {
		throw {
			message: response.error || 'Request failed',
			status: 0,
		} as ApiError;
	}
	return response;
}

/**
 * Fetch with retry logic using exponential backoff
 * @param url - The URL to fetch
 * @param options - Fetch options
 * @param retries - Number of retry attempts (default: 3)
 * @param backoffMs - Initial backoff in milliseconds (default: 1000)
 */
async function fetchWithRetry<T>(
	url: string,
	options: RequestInit = {},
	retries = 3,
	backoffMs = 1000
): Promise<T> {
	let lastError: any;

	for (let i = 0; i < retries; i++) {
		try {
			const response = await fetch(url, options);

			if (!response.ok) {
				const errorData = await response.json().catch(() => ({}));
				throw {
					message: errorData.message || `HTTP ${response.status}: ${response.statusText}`,
					status: response.status,
				} as ApiError;
			}

			// Handle empty responses
			const contentType = response.headers.get('content-type');
			if (contentType && contentType.includes('application/json')) {
				return await response.json();
			}

			return {} as T;
		} catch (error: any) {
			lastError = error;

			// Don't retry on client errors (4xx)
			if (error.status >= 400 && error.status < 500) {
				throw error;
			}

			// Retry on server errors (5xx) or network errors
			if (i < retries - 1) {
				const delay = backoffMs * Math.pow(2, i); // Exponential backoff
				await new Promise(resolve => setTimeout(resolve, delay));
			}
		}
	}

	throw lastError!;
}

export class ApiClient {
	private baseUrl: string;

	constructor(baseUrl: string = BASE_URL) {
		this.baseUrl = baseUrl;
	}

	private async request<T>(
		endpoint: string,
		options: RequestInit = {}
	): Promise<T> {
		const url = `${this.baseUrl}${endpoint}`;

		const config: RequestInit = {
			headers: {
				'Content-Type': 'application/json',
				...options.headers,
			},
			...options,
		};

		try {
			return await fetchWithRetry<T>(url, config);
		} catch (error) {
			if ((error as ApiError).status) {
				throw error;
			}
			throw {
				message: error instanceof Error ? error.message : 'Network error occurred',
				status: 0,
			} as ApiError;
		}
	}

	async get<T>(endpoint: string, options?: RequestInit): Promise<T> {
		return this.request<T>(endpoint, { ...options, method: 'GET' });
	}

	async post<T>(endpoint: string, data?: any, options?: RequestInit): Promise<T> {
		return this.request<T>(endpoint, {
			...options,
			method: 'POST',
			body: data ? JSON.stringify(data) : undefined,
		});
	}

	async put<T>(endpoint: string, data?: any, options?: RequestInit): Promise<T> {
		return this.request<T>(endpoint, {
			...options,
			method: 'PUT',
			body: data ? JSON.stringify(data) : undefined,
		});
	}

	async delete<T>(endpoint: string, options?: RequestInit): Promise<T> {
		return this.request<T>(endpoint, { ...options, method: 'DELETE' });
	}
}

/**
 * Mock-aware API client wrapper
 * Intercepts all requests when VITE_USE_MOCK=true
 */
class MockAwareApiClient {
	private realClient: ApiClient;

	constructor() {
		this.realClient = new ApiClient();
	}

	private get client() {
		return isMockMode ? mockClient : this.realClient;
	}

	async get<T>(endpoint: string, options?: RequestInit): Promise<T> {
		return this.client.get<T>(endpoint, options);
	}

	async post<T>(endpoint: string, data?: any, options?: RequestInit): Promise<T> {
		return this.client.post<T>(endpoint, data, options);
	}

	async put<T>(endpoint: string, data?: any, options?: RequestInit): Promise<T> {
		return this.client.put<T>(endpoint, data, options);
	}

	async delete<T>(endpoint: string, options?: RequestInit): Promise<T> {
		return this.client.delete<T>(endpoint, options);
	}
}

// Export singleton instance with mock support
export const apiClient = new MockAwareApiClient();
