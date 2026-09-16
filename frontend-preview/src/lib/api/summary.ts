/**
 * Summary Generation API
 * Generates temporary file summaries without storing them
 */

import { apiClient } from './client';

export interface SummaryRequest {
	file_paths?: string[];
	directory_paths?: string[];
	extensions?: string[];
	exclude_dirs?: string[];
	respect_gitignore?: boolean;
	ignore_patterns?: string[];
	recursive?: boolean;
	max_files?: number;
}

export interface FileSummaryItem {
	file_path: string;
	language: string;
	summary: string;
	main_entities: string[];
	imports: string[];
	exports: string[];
	entity_count: number;
	line_count: number;
	tags: string[];
	importance_level: string;
	success: boolean;
	error?: string;
}

export interface SummaryResponse {
	success: boolean;
	total_files: number;
	success_count: number;
	failed_count: number;
	summaries: FileSummaryItem[];
	elapsed_ms: number;
	warnings: string[];
}

export const summaryApi = {
	/** POST /api/summary — generate file summaries */
	generate: (request: SummaryRequest) =>
		apiClient.post<SummaryResponse>('/api/summary', request),
};