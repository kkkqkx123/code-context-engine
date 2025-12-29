import { VectorPoint, SearchOptions, SearchResult } from './IVectorStore';

/** 
 * Qdrant 配置接口
 */
export interface QdrantConfig {
  host: string;
  port: number;
  apiKey?: string;
  useHttps: boolean;
  timeout: number;
  collection: string;
}

/**
 * 集合信息接口
 */
export interface CollectionInfo {
  name: string;
  vectors: {
    size: number;
    distance: 'Cosine' | 'Euclid' | 'Dot' | 'Manhattan';
  };
  pointsCount: number;
  status: 'green' | 'yellow' | 'red' | 'grey';
}

/**
 * 向量距离度量类型
 */
export type VectorDistance = 'Cosine' | 'Euclid' | 'Dot' | 'Manhattan';

/**
 * 连接状态枚举
 */
export enum ConnectionStatus {
  DISCONNECTED = 'disconnected',
  CONNECTING = 'connecting',
  CONNECTED = 'connected',
  ERROR = 'error'
}

/**
 * 集合状态枚举
 */
export enum CollectionStatus {
  CREATING = 'creating',
  READY = 'ready',
  UPDATING = 'updating',
  DELETING = 'deleting',
  ERROR = 'error'
}

/**
 * 有效载荷索引字段配置
 */
export interface PayloadIndexField {
  fieldName: string;
  fieldType: 'keyword' | 'integer' | 'float' | 'bool' | 'datetime';
}

/**
 * 集合创建选项
 */
export interface CollectionCreateOptions {
  vectorSize: number;
  distance: VectorDistance;
  recreateIfExists?: boolean;
  optimizersConfig?: {
    defaultSegmentNumber?: number;
    indexingThreshold?: number;
    flushIntervalSec?: number;
    maxOptimizationThreads?: number;
  };
  replicationFactor?: number;
  writeConsistencyFactor?: number;
  payloadIndexes?: string[];
}

/**
 * 向量插入选项
 */
export interface VectorUpsertOptions {
  batchSize?: number;
  validateDimensions?: boolean;
  skipInvalidPoints?: boolean;
}

/**
 * 向量搜索选项扩展
 */
export interface VectorSearchOptions extends SearchOptions {
  useHybridSearch?: boolean;
  hybridSearchWeight?: number;
  exactSearch?: boolean;
  searchStrategy?: 'default' | 'hnsw' | 'flat';
}

/**
 * 查询过滤器接口
 */
export interface QueryFilter {
  language?: string[];
  chunkType?: string[];
  filePath?: string[];
  projectId?: string;
  snippetType?: string[];
  customFilters?: Record<string, any>;
}

/**
 * 项目信息接口
 */
export interface ProjectInfo {
  id: string;
  path: string;
  collectionName: string;
  vectorSize?: number;
  distance?: VectorDistance;
  createdAt?: Date;
  updatedAt?: Date;
}

/**
 * 批处理结果接口
 */
export interface BatchResult {
  success: boolean;
  processedCount: number;
  failedCount: number;
  errors?: Error[];
  batchNumber?: number;
  totalBatches?: number;
}

/**
 * 集合统计信息接口
 */
export interface CollectionStats {
  name: string;
  vectorsCount: number;
  vectorSize: number;
  distance: VectorDistance;
  status: CollectionStatus;
  indexedVectorsCount?: number;
  payloadSchema?: Record<string, any>;
  diskUsage?: number;
  memoryUsage?: number;
}

/**
 * 默认配置常量
 */
export const DEFAULT_QDRANT_CONFIG: Partial<QdrantConfig> = {
  useHttps: false,
  timeout: 30000,
  port: 6333
};

/**
 * 默认集合创建选项
 */
export const DEFAULT_COLLECTION_OPTIONS: Partial<CollectionCreateOptions> = {
  distance: 'Cosine',
  recreateIfExists: false,
  optimizersConfig: {
    defaultSegmentNumber: 2,
    indexingThreshold: 20000,
    flushIntervalSec: 5,
    maxOptimizationThreads: 1
  },
  replicationFactor: 1,
  writeConsistencyFactor: 1,
  payloadIndexes: [
    'language',
    'chunkType',
    'filePath',
    'projectId',
    'snippetMetadata.snippetType'
  ]
};

/**
 * 默认向量插入选项
 */
export const DEFAULT_VECTOR_UPSERT_OPTIONS: Partial<VectorUpsertOptions> = {
  batchSize: 100,
  validateDimensions: true,
  skipInvalidPoints: false
};

/**
 * 默认向量搜索选项
 */
export const DEFAULT_VECTOR_SEARCH_OPTIONS: Partial<VectorSearchOptions> = {
  limit: 10,
  withPayload: true,
  withVector: false,
  useHybridSearch: false,
  exactSearch: false,
  searchStrategy: 'default'
};

/**
 * 支持的向量距离度量类型
 */
export const SUPPORTED_VECTOR_DISTANCES: VectorDistance[] = [
  'Cosine',
  'Euclid',
  'Dot',
  'Manhattan'
];

/**
 * 支持的有效载荷索引字段类型
 */
export const SUPPORTED_PAYLOAD_INDEX_TYPES = [
  'keyword',
  'integer',
  'float',
  'bool',
  'datetime'
];

/**
 * 常用有效载荷索引字段
 */
export const COMMON_PAYLOAD_INDEX_FIELDS: PayloadIndexField[] = [
  { fieldName: 'language', fieldType: 'keyword' },
  { fieldName: 'chunkType', fieldType: 'keyword' },
  { fieldName: 'filePath', fieldType: 'keyword' },
  { fieldName: 'projectId', fieldType: 'keyword' },
  { fieldName: 'snippetMetadata.snippetType', fieldType: 'keyword' }
];

/**
 * 错误消息常量
 */
export const ERROR_MESSAGES = {
  CONNECTION_FAILED: 'Failed to connect to Qdrant',
  COLLECTION_NOT_FOUND: 'Collection not found',
  COLLECTION_ALREADY_EXISTS: 'Collection already exists',
  INVALID_VECTOR_DIMENSIONS: 'Invalid vector dimensions',
  VECTOR_DIMENSION_MISMATCH: 'Vector dimension mismatch',
  INVALID_POINT_ID: 'Invalid point ID',
  INVALID_VECTOR_DATA: 'Invalid vector data',
  INVALID_PAYLOAD_DATA: 'Invalid payload data',
  UPSERT_FAILED: 'Failed to upsert vectors',
  SEARCH_FAILED: 'Failed to search vectors',
  DELETE_FAILED: 'Failed to delete points',
  INDEX_CREATION_FAILED: 'Failed to create payload index',
  PROJECT_NOT_FOUND: 'Project not found',
  COLLECTION_NAME_NOT_FOUND: 'Collection name not found'
} as const;

/**
 * 事件类型枚举
 */
export enum QdrantEventType {
  CONNECTING = 'connecting',
  CONNECTED = 'connected',
  DISCONNECTED = 'disconnected',
  COLLECTION_CREATED = 'collection_created',
  COLLECTION_DELETED = 'collection_deleted',
  VECTORS_UPSERTED = 'vectors_upserted',
  VECTORS_SEARCHED = 'vectors_searched',
  POINTS_DELETED = 'points_deleted',
  ERROR = 'error'
}

/**
 * 事件数据接口
 */
export interface QdrantEvent {
  type: QdrantEventType;
  timestamp: Date;
  data?: any;
  error?: Error;
}