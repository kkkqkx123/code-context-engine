/**
 * Mock Data - Hardcoded data for all API responses
 * Used when VITE_USE_MOCK=true to preview UI without backend
 */

export const isMockMode = import.meta.env.VITE_USE_MOCK === 'true';

// ─── Delay helper ──────────────────────────────────────────────────

export function mockDelay(ms: number = 100): Promise<void> {
	return new Promise(resolve => setTimeout(resolve, ms));
}

// ─── Index & Project Mock Data ─────────────────────────────────────

export const mockProjects = [
	{
		id: '1',
		name: 'code-context-engine',
		root_path: '/home/user/projects/code-context-engine',
		extensions: ['rs', 'toml', 'md'],
		exclude_dirs: ['target', 'node_modules'],
		respect_gitignore: true,
		created_at: '2024-01-15T10:30:00Z',
		last_indexed: '2024-03-20T14:25:00Z'
	},
	{
		id: '2',
		name: 'frontend-app',
		root_path: '/home/user/projects/frontend-app',
		extensions: ['ts', 'tsx', 'svelte'],
		exclude_dirs: ['dist', '.svelte-kit'],
		respect_gitignore: true,
		created_at: '2024-02-01T09:00:00Z',
		last_indexed: '2024-03-19T16:00:00Z'
	},
	{
		id: '3',
		name: 'api-service',
		root_path: '/home/user/projects/api-service',
		extensions: ['py', 'yaml', 'json'],
		exclude_dirs: ['__pycache__', '.venv'],
		respect_gitignore: true,
		created_at: '2024-02-20T11:00:00Z',
		last_indexed: null
	}
];

export const mockIndexStats = {
	success: true,
	statistics: {
		total_entities: 12847,
		total_relations: 3421,
		total_vectors: 12847,
		total_bm25_documents: 12847,
		total_files: 342
	},
	elapsed_ms: 15230
};

// ─── Health Mock Data ──────────────────────────────────────────────

export const mockHealthStatus = {
	healthy: true,
	qdrant: { reachable: true, message: 'Connected to Qdrant v1.8.0' },
	bm25: { reachable: true, message: 'BM25 index loaded (12847 documents)' },
	embedding: { reachable: true, message: 'Model: text-embedding-3-small' }
};

export const mockQdrantHealth = {
	healthy: true,
	circuit_breaker: 'Closed',
	diagnostic: {
		reachable: true,
		version: '1.8.0',
		collection_exists: true,
		points_count: 12847,
		error: null
	}
};

export const mockEmbeddingHealth = {
	healthy: true,
	model_name: 'text-embedding-3-small',
	message: 'Embedding service ready'
};

export const mockBm25Health = {
	enabled: true,
	connected: true,
	index_path: '/data/bm25_index'
};

export const mockRetryQueueStatus = {
	pending_count: 0,
	is_empty: true
};

// ─── Search Mock Data ──────────────────────────────────────────────

export const mockSearchResults = {
	success: true,
	total: 5,
	items: [
		{
			entity_ids: [101],
			score: 0.92,
			file_path: 'crates/cce-parser/src/grouper.rs',
			code_chunk: `pub fn group_entities(entities: Vec<Entity>) -> Vec<EntityGroup> {\n    let mut groups = Vec::new();\n    // Group by file and proximity\n    for entity in entities {\n        // ... grouping logic\n    }\n    groups\n}`,
			start_line: 45,
			end_line: 55,
			entity_type: 'function',
			source: 'vector',
			call_chain: [
				{ function_id: '101', function_name: 'group_entities', file_path: 'crates/cce-parser/src/grouper.rs', depth: 0, relation_type: 'root' },
				{ function_id: '102', function_name: 'process_chunk', file_path: 'crates/cce-parser/src/chunker.rs', depth: 1, relation_type: 'callee' }
			]
		},
		{
			entity_ids: [201],
			score: 0.87,
			file_path: 'crates/cce-parser/src/ast_to_nl.rs',
			code_chunk: `pub fn ast_to_natural_language(node: &Node, source: &[u8]) -> String {\n    let mut result = String::new();\n    // Convert AST nodes to NL descriptions\n    traverse_node(node, source, &mut result);\n    result\n}`,
			start_line: 23,
			end_line: 30,
			entity_type: 'function',
			source: 'vector',
			call_chain: []
		},
		{
			entity_ids: [301],
			score: 0.81,
			file_path: 'crates/cce-infrastructure/src/storage/mod.rs',
			code_chunk: `pub struct StorageManager {\n    qdrant: QdrantClient,\n    bm25: Bm25Index,\n    sqlite: SqlitePool,\n    cache: CacheStore,\n}`,
			start_line: 12,
			end_line: 18,
			entity_type: 'struct',
			source: 'bm25',
			call_chain: []
		},
		{
			entity_ids: [401],
			score: 0.76,
			file_path: 'crates/cce-orchestrator/src/lib.rs',
			code_chunk: `pub async fn run_index(config: IndexConfig) -> Result<IndexResult> {\n    let scanner = Scanner::new(&config.root_path);\n    let files = scanner.scan().await?;\n    // ... indexing pipeline\n    Ok(IndexResult { files_indexed: files.len() })\n}`,
			start_line: 67,
			end_line: 75,
			entity_type: 'function',
			source: 'hybrid',
			call_chain: []
		},
		{
			entity_ids: [501],
			score: 0.69,
			file_path: 'crates/cce-server/src/routes/mod.rs',
			code_chunk: `pub fn create_router() -> Router {\n    Router::new()\n        .route("/api/health", get(health_check))\n        .route("/api/search", post(search_handler))\n        .route("/api/index", post(index_handler))\n}`,
			start_line: 15,
			end_line: 22,
			entity_type: 'function',
			source: 'bm25',
			call_chain: []
		}
	],
	elapsed_ms: 45,
	sources_used: ['vector', 'bm25']
};

// ─── Entity Mock Data ──────────────────────────────────────────────

export const mockFunctionDetail = {
	success: true,
	function: {
		id: '101',
		name: 'group_entities',
		signature: 'pub fn group_entities(entities: Vec<Entity>) -> Vec<EntityGroup>',
		parameters: [{ name: 'entities', type_name: 'Vec<Entity>' }],
		return_type: 'Vec<EntityGroup>',
		file_path: 'crates/cce-parser/src/grouper.rs',
		start_line: 45,
		end_line: 68,
		doc_comment: 'Groups related entities into logical clusters based on file proximity and semantic similarity.'
	}
};

export const mockFunctionCalls = {
	success: true,
	relation_epoch: 1,
	function_id: '101',
	function_name: 'group_entities',
	callees: [
		{ function_id: '102', function_name: 'process_chunk', file_path: 'crates/cce-parser/src/chunker.rs', depth: 1, relation_type: 'callee', call_line: 48 },
		{ function_id: '103', function_name: 'calculate_similarity', file_path: 'crates/cce-parser/src/similarity.rs', depth: 1, relation_type: 'callee', call_line: 52 }
	],
	total_callees: 2
};

export const mockFunctionCallers = {
	success: true,
	relation_epoch: 1,
	function_id: '101',
	function_name: 'group_entities',
	callers: [
		{ function_id: '201', function_name: 'parse_file', file_path: 'crates/cce-parser/src/lib.rs', depth: 1, relation_type: 'caller', call_line: 34 },
		{ function_id: '202', function_name: 'batch_parse', file_path: 'crates/cce-parser/src/batch.rs', depth: 1, relation_type: 'caller', call_line: 67 }
	],
	total_callers: 2
};

export const mockCallChain = {
	success: true,
	relation_epoch: 1,
	function_id: '101',
	function_name: 'group_entities',
	direction: 'down',
	call_chain: [
		{ function_id: '101', function_name: 'group_entities', file_path: 'crates/cce-parser/src/grouper.rs', depth: 0, relation_type: 'root' },
		{ function_id: '102', function_name: 'process_chunk', file_path: 'crates/cce-parser/src/chunker.rs', depth: 1, relation_type: 'callee', call_line: 48 },
		{ function_id: '103', function_name: 'calculate_similarity', file_path: 'crates/cce-parser/src/similarity.rs', depth: 2, relation_type: 'callee', call_line: 15 },
		{ function_id: '104', function_name: 'normalize_text', file_path: 'crates/cce-parser/src/utils.rs', depth: 3, relation_type: 'callee', call_line: 22 }
	]
};

export const mockClassInheritance = {
	success: true,
	relation_epoch: 1,
	class_id: '301',
	class_name: 'StorageManager',
	base_classes: [
		{ class_id: '302', class_name: 'BaseManager', file_path: 'crates/cce-core/src/manager.rs', depth: 1 }
	],
	derived_classes: [
		{ class_id: '303', class_name: 'IndexedStorageManager', file_path: 'crates/cce-infrastructure/src/storage/indexed.rs', depth: 1 }
	]
};

export const mockClassImplementations = {
	success: true,
	relation_epoch: 1,
	class_id: '301',
	class_name: 'StorageManager',
	implemented_interfaces: [
		{ interface_id: '401', interface_name: 'Storable', file_path: 'crates/cce-core/src/traits.rs' },
		{ interface_id: '402', interface_name: 'Queryable', file_path: 'crates/cce-core/src/traits.rs' }
	],
	implementing_classes: []
};

// ─── Storage Mock Data ─────────────────────────────────────────────

export const mockStorageStatus = {
	success: true,
	status: {
		vector_storage: {
			connected: true,
			item_count: 12847,
			disk_usage_mb: 245.6,
			version: '1.8.0'
		},
		bm25_storage: {
			connected: true,
			item_count: 12847,
			disk_usage_mb: 89.2
		},
		relation_storage: {
			connected: true,
			item_count: 3421,
			disk_usage_mb: 12.4
		},
		cache_storage: {
			connected: true,
			item_count: 856,
			disk_usage_mb: 45.8
		},
		total_disk_usage_mb: 393.0,
		process_status: {
			managed: true,
			status: 'Running',
			running: true
		}
	}
};

// ─── Metrics Mock Data ─────────────────────────────────────────────

export const mockMetricsData = {
	server: {
		uptime_seconds: 86400,
		total_requests: 15234,
		active_connections: 3
	},
	indexing: {
		total_files_indexed: 342,
		total_entities_extracted: 12847,
		avg_processing_time_ms: 45.2
	},
	storage: {
		vector_count: 12847,
		total_size_mb: 393.0
	}
};

// ─── Watch Mock Data ───────────────────────────────────────────────

export const mockWatchStatus = {
	active: true,
	events_processed: 23,
	watched_dirs: ['/home/user/projects/code-context-engine/src'],
	started_at: '2024-03-20T14:30:00Z'
};

// ─── Config Mock Data ──────────────────────────────────────────────

export const mockConfigInfo = {
	initialized: true,
	database: {
		qdrant_url: 'http://localhost:6333',
		qdrant_collection: 'cce_entities',
		sqlite_path: '/data/cce.db'
	},
	embedder: {
		provider: 'openai',
		model: 'text-embedding-3-small',
		dimensions: 1536
	},
	project_count: 3
};

export const mockConfigValidation = {
	valid: true,
	errors: [],
	warnings: [],
	dependency_warnings: [
		{ level: 'info', message: 'Qdrant version 1.8.0 detected', module: 'storage' }
	]
};

// ─── Tools Mock Data ───────────────────────────────────────────────

export const mockCompressResult = {
	success: true,
	file_path: 'crates/cce-parser/src/grouper.rs',
	language: 'rust',
	file_hash: 'abc123def456',
	from_cache: false,
	semantic_text: 'Module for grouping related code entities into logical clusters. Implements proximity-based and semantic similarity grouping algorithms.'
};

export const mockDiagnoseResult = {
	success: true,
	result: {
		issues: [
			{ severity: 'warning', message: 'Function `group_entities` is too long (23 lines)', suggestion: 'Consider extracting helper functions', line: 45, column: 0 },
			{ severity: 'info', message: 'Doc comment could be more detailed', line: 44, column: 0 }
		]
	}
};

export const mockSymbolsResult = {
	success: true,
	result: {
		results: [
			{
				path: 'crates/cce-parser/src/grouper.rs',
				success: true,
				symbol_count: 3,
				symbols: [
					{ name: 'group_entities', type: 'function', line_start: 45, line_end: 68 },
					{ name: 'EntityGroup', type: 'struct', line_start: 12, line_end: 18 },
					{ name: 'calculate_proximity', type: 'function', line_start: 72, line_end: 85 }
				]
			}
		],
		success_count: 1,
		fail_count: 0
	}
};

// ─── Summary Mock Data ─────────────────────────────────────────────

export const mockSummaryResult = {
	success: true,
	total_files: 3,
	success_count: 3,
	failed_count: 0,
	summaries: [
		{
			file_path: 'crates/cce-parser/src/grouper.rs',
			language: 'rust',
			summary: 'Entity grouping module that clusters related code entities using proximity and semantic similarity.',
			main_entities: ['group_entities', 'EntityGroup', 'calculate_proximity'],
			imports: ['crate::types::Entity', 'crate::similarity::cosine_similarity'],
			exports: ['group_entities', 'EntityGroup'],
			entity_count: 3,
			loc: 120
		},
		{
			file_path: 'crates/cce-parser/src/chunker.rs',
			language: 'rust',
			summary: 'Code chunking module that splits source files into processable chunks.',
			main_entities: ['process_chunk', 'Chunk', 'ChunkConfig'],
			imports: ['tree_sitter::Tree'],
			exports: ['process_chunk', 'Chunk'],
			entity_count: 3,
			loc: 95
		},
		{
			file_path: 'crates/cce-parser/src/ast_to_nl.rs',
			language: 'rust',
			summary: 'AST to natural language conversion module for generating human-readable code descriptions.',
			main_entities: ['ast_to_natural_language', 'TraverseVisitor'],
			imports: ['tree_sitter::Node'],
			exports: ['ast_to_natural_language'],
			entity_count: 2,
		LOC: 78
		}
	],
	elapsed_ms: 2340,
	warnings: []
};

// ─── Qdrant Process Mock Data ──────────────────────────────────────

export const mockQdrantProcessStatus = {
	managed: true,
	status: 'Running'
};

export const mockQdrantActionResponse = {
	success: true,
	message: 'Qdrant process restarted successfully',
	status: 'Running'
};
