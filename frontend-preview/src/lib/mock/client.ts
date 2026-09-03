/**
 * Mock API Client - Returns hardcoded data when VITE_USE_MOCK=true
 * Wraps the original API client to intercept all requests
 */

import { isMockMode, mockDelay } from './data';
import * as mockData from './data';

// Type imports for mock data
import type { IndexStatsResponse, Project } from '../api/index';
import type { HealthStatus, QdrantHealthStatus, EmbeddingHealthStatus, Bm25HealthStatus, RetryQueueStatus } from '../api/health';
import type { SearchResponse } from '../api/search';
import type { FunctionDetailResponse, FunctionCallsResponse, FunctionCallersResponse, CallChainResponse, ClassInheritanceResponse, ClassImplementationsResponse } from '../api/entities';
import type { StorageStatusResponse } from '../api/storage';
import type { MetricsData } from '../api/metrics';
import type { WatchStatus } from '../api/watch';
import type { ConfigInfoResponse, ConfigValidateResponse } from '../api/config';
import type { CompressApiResponse, DiagnoseResponse, GetSymbolsResponse, ToolApiResponse } from '../api/tools';
import type { SummaryResponse } from '../api/summary';
import type { QdrantProcessStatusResponse, QdrantActionResponse } from '../api/qdrant';

export interface MockApiResponse<T> {
	success: boolean;
	data?: T;
	error?: string;
}

// Mock client that intercepts API calls
export const mockClient = {
	async get<T>(endpoint: string): Promise<T> {
		await mockDelay();

		// Route to appropriate mock data based on endpoint
		if (endpoint === '/api/health') return mockData.mockHealthStatus as T;
		if (endpoint === '/api/health/qdrant') return mockData.mockQdrantHealth as T;
		if (endpoint === '/api/health/embedding') return mockData.mockEmbeddingHealth as T;
		if (endpoint === '/api/health/bm25') return mockData.mockBm25Health as T;
		if (endpoint === '/api/retry-queue') return mockData.mockRetryQueueStatus as T;
		if (endpoint.startsWith('/api/index/stats')) return mockData.mockIndexStats as T;
		if (endpoint === '/api/project') return { success: true, projects: mockData.mockProjects, total: mockData.mockProjects.length } as T;
		if (endpoint.match(/\/api\/project\/\w+$/)) return { success: true, project: mockData.mockProjects[0] } as T;
		if (endpoint.match(/\/api\/project\/\w+\/function\/\w+$/)) return mockData.mockFunctionDetail as T;
		if (endpoint.match(/\/api\/project\/\w+\/function\/\w+\/calls$/)) return mockData.mockFunctionCalls as T;
		if (endpoint.match(/\/api\/project\/\w+\/function\/\w+\/callers$/)) return mockData.mockFunctionCallers as T;
		if (endpoint.match(/\/api\/project\/\w+\/call-chain\/\w+/)) return mockData.mockCallChain as T;
		if (endpoint.match(/\/api\/project\/\w+\/class\/\w+\/inheritance$/)) return mockData.mockClassInheritance as T;
		if (endpoint.match(/\/api\/project\/\w+\/class\/\w+\/implementations$/)) return mockData.mockClassImplementations as T;
		if (endpoint === '/api/storage/status') return mockData.mockStorageStatus as T;
		if (endpoint === '/api/metrics/json') return mockData.mockMetricsData as T;
		if (endpoint.match(/\/api\/project\/\w+\/watch\/status$/)) return mockData.mockWatchStatus as T;
		if (endpoint === '/api/config') return mockData.mockConfigInfo as T;
		if (endpoint === '/api/config/validate') return mockData.mockConfigValidation as T;
		if (endpoint === '/api/qdrant/process/status') return mockData.mockQdrantProcessStatus as T;

		console.warn(`[Mock] Unhandled GET endpoint: ${endpoint}`);
		return {} as T;
	},

	async post<T>(endpoint: string, data?: unknown): Promise<T> {
		await mockDelay();

		if (endpoint === '/api/search') return mockData.mockSearchResults as T;
		if (endpoint === '/api/search/aggregated') return mockData.mockSearchResults as T;
		if (endpoint === '/api/entities/search') return { success: true, total: 0, items: [], elapsed_ms: 10 } as T;
		if (endpoint === '/api/index') return { success: true, message: 'Indexing started (mock)' } as T;
		if (endpoint === '/api/index/incremental') return { success: true, message: 'Incremental indexing started (mock)' } as T;
		if (endpoint === '/api/parse') return { entities: [], language: 'rust', file_path: '' } as T;
		if (endpoint === '/api/retry-queue/process') return { processed: 0, message: 'No items to process' } as T;
		if (endpoint.match(/\/api\/project$/)) return { success: true, project: mockData.mockProjects[0] } as T;
		if (endpoint.match(/\/api\/project\/\w+\/index$/)) return { success: true } as T;
		if (endpoint.match(/\/api\/project\/\w+\/reload$/)) return { success: true } as T;
		if (endpoint.match(/\/api\/project\/\w+\/config$/)) return { success: true } as T;
		if (endpoint.match(/\/api\/project\/\w+\/watch\/start$/)) return { success: true } as T;
		if (endpoint.match(/\/api\/project\/\w+\/watch\/stop$/)) return { success: true } as T;
		if (endpoint === '/api/config/reload') return { success: true, message: 'Configuration reloaded' } as T;
		if (endpoint === '/api/tools/compress') return mockData.mockCompressResult as T;
		if (endpoint === '/api/tools/compress/batch') return { successes: [], failures: [] } as T;
		if (endpoint === '/api/tools/diagnose') return mockData.mockDiagnoseResult as T;
		if (endpoint === '/api/tools/symbols') return mockData.mockSymbolsResult as T;
		if (endpoint === '/api/tools/references') return { success: true, result: {} } as T;
		if (endpoint === '/api/tools/definition') return { success: true, result: {} } as T;
		if (endpoint === '/api/tools/keyword-search') return { success: true, data: { query: '', total: 0, results: [] } } as T;
		if (endpoint === '/api/summary') return mockData.mockSummaryResult as T;
		if (endpoint === '/api/qdrant/process/start') return mockData.mockQdrantActionResponse as T;
		if (endpoint === '/api/qdrant/process/stop') return mockData.mockQdrantActionResponse as T;
		if (endpoint === '/api/qdrant/process/restart') return mockData.mockQdrantActionResponse as T;

		console.warn(`[Mock] Unhandled POST endpoint: ${endpoint}`);
		return {} as T;
	},

	async put<T>(endpoint: string, data?: unknown): Promise<T> {
		await mockDelay();

		if (endpoint.match(/\/api\/project\/\w+$/)) return { success: true, project: mockData.mockProjects[0] } as T;
		if (endpoint.match(/\/api\/project\/\w+\/config$/)) return { success: true } as T;

		console.warn(`[Mock] Unhandled PUT endpoint: ${endpoint}`);
		return {} as T;
	},

	async delete<T>(endpoint: string): Promise<T> {
		await mockDelay();

		if (endpoint.match(/\/api\/project\/\w+$/)) return { success: true } as T;
		if (endpoint === '/api/index') return { success: true } as T;
		if (endpoint === '/api/retry-queue') return { cleared: 0, message: 'Queue already empty' } as T;
		if (endpoint.match(/\/api\/metrics\/cleanup/)) return { success: true, deleted_count: 0 } as T;

		console.warn(`[Mock] Unhandled DELETE endpoint: ${endpoint}`);
		return {} as T;
	}
};
