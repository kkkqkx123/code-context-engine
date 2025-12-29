import {
  QdrantConfig,
  CollectionInfo,
  VectorDistance,
  ConnectionStatus,
  CollectionStatus,
  PayloadIndexField,
  CollectionCreateOptions,
  VectorUpsertOptions,
  VectorSearchOptions,
  QueryFilter,
  ProjectInfo,
  BatchResult,
  CollectionStats,
  DEFAULT_QDRANT_CONFIG,
  DEFAULT_COLLECTION_OPTIONS,
  DEFAULT_VECTOR_UPSERT_OPTIONS,
  DEFAULT_VECTOR_SEARCH_OPTIONS,
  SUPPORTED_VECTOR_DISTANCES,
  SUPPORTED_PAYLOAD_INDEX_TYPES,
  COMMON_PAYLOAD_INDEX_FIELDS,
  ERROR_MESSAGES,
  QdrantEventType,
  QdrantEvent
} from '../../qdrant/QdrantTypes';
import { VectorPoint, SearchOptions, SearchResult } from '../../qdrant/IVectorStore';

describe('QdrantTypes', () => {
  describe('Interfaces', () => {
    it('should define QdrantConfig interface', () => {
      const config: QdrantConfig = {
        host: 'localhost',
        port: 6333,
        useHttps: false,
        timeout: 30000,
        collection: 'test-collection'
      };

      expect(config.host).toBe('localhost');
      expect(config.port).toBe(6333);
      expect(config.useHttps).toBe(false);
      expect(config.timeout).toBe(30000);
      expect(config.collection).toBe('test-collection');
    });

    it('should define CollectionInfo interface', () => {
      const collectionInfo: CollectionInfo = {
        name: 'test-collection',
        vectors: {
          size: 128,
          distance: 'Cosine'
        },
        pointsCount: 100,
        status: 'green'
      };

      expect(collectionInfo.name).toBe('test-collection');
      expect(collectionInfo.vectors.size).toBe(128);
      expect(collectionInfo.vectors.distance).toBe('Cosine');
      expect(collectionInfo.pointsCount).toBe(100);
      expect(collectionInfo.status).toBe('green');
    });

    it('should define PayloadIndexField interface', () => {
      const payloadIndexField: PayloadIndexField = {
        fieldName: 'testField',
        fieldType: 'keyword'
      };

      expect(payloadIndexField.fieldName).toBe('testField');
      expect(payloadIndexField.fieldType).toBe('keyword');
    });

    it('should define CollectionCreateOptions interface', () => {
      const collectionCreateOptions: CollectionCreateOptions = {
        vectorSize: 128,
        distance: 'Cosine'
      };

      expect(collectionCreateOptions.vectorSize).toBe(128);
      expect(collectionCreateOptions.distance).toBe('Cosine');
    });

    it('should define VectorUpsertOptions interface', () => {
      const vectorUpsertOptions: VectorUpsertOptions = {
        batchSize: 100
      };

      expect(vectorUpsertOptions.batchSize).toBe(100);
    });

    it('should define VectorSearchOptions interface', () => {
      const vectorSearchOptions: VectorSearchOptions = {
        limit: 10,
        useHybridSearch: true
      };

      expect(vectorSearchOptions.limit).toBe(10);
      expect(vectorSearchOptions.useHybridSearch).toBe(true);
    });

    it('should define QueryFilter interface', () => {
      const queryFilter: QueryFilter = {
        language: ['typescript'],
        chunkType: ['function'],
        filePath: ['/test/file.ts'],
        projectId: 'test-project-id',
        snippetType: ['class'],
        customFilters: {
          'metadata.version': '1.0.0'
        }
      };

      expect(queryFilter.language).toContain('typescript');
      expect(queryFilter.chunkType).toContain('function');
      expect(queryFilter.filePath).toContain('/test/file.ts');
      expect(queryFilter.projectId).toBe('test-project-id');
      expect(queryFilter.snippetType).toContain('class');
      expect(queryFilter.customFilters!['metadata.version']).toBe('1.0.0');
    });

    it('should define ProjectInfo interface', () => {
      const projectInfo: ProjectInfo = {
        id: 'test-project-id',
        path: '/test/project',
        collectionName: 'project-test-project-id'
      };

      expect(projectInfo.id).toBe('test-project-id');
      expect(projectInfo.path).toBe('/test/project');
      expect(projectInfo.collectionName).toBe('project-test-project-id');
    });

    it('should define BatchResult interface', () => {
      const batchResult: BatchResult = {
        success: true,
        processedCount: 100,
        failedCount: 0
      };

      expect(batchResult.success).toBe(true);
      expect(batchResult.processedCount).toBe(100);
      expect(batchResult.failedCount).toBe(0);
    });

    it('should define CollectionStats interface', () => {
      const collectionStats: CollectionStats = {
        name: 'test-collection',
        vectorsCount: 1000,
        vectorSize: 128,
        distance: 'Cosine',
        status: CollectionStatus.READY
      };

      expect(collectionStats.name).toBe('test-collection');
      expect(collectionStats.vectorsCount).toBe(1000);
      expect(collectionStats.vectorSize).toBe(128);
      expect(collectionStats.distance).toBe('Cosine');
      expect(collectionStats.status).toBe(CollectionStatus.READY);
    });

    it('should define QdrantEvent interface', () => {
      const qdrantEvent: QdrantEvent = {
        type: QdrantEventType.CONNECTED,
        timestamp: new Date(),
        data: { test: 'data' },
        error: new Error('Test error')
      };

      expect(qdrantEvent.type).toBe(QdrantEventType.CONNECTED);
      expect(qdrantEvent.timestamp).toBeInstanceOf(Date);
      expect(qdrantEvent.data).toEqual({ test: 'data' });
      expect(qdrantEvent.error).toBeInstanceOf(Error);
    });
  });

  describe('Enums', () => {
    it('should define ConnectionStatus enum', () => {
      expect(ConnectionStatus.DISCONNECTED).toBe('disconnected');
      expect(ConnectionStatus.CONNECTING).toBe('connecting');
      expect(ConnectionStatus.CONNECTED).toBe('connected');
      expect(ConnectionStatus.ERROR).toBe('error');
    });

    it('should define CollectionStatus enum', () => {
      expect(CollectionStatus.CREATING).toBe('creating');
      expect(CollectionStatus.READY).toBe('ready');
      expect(CollectionStatus.UPDATING).toBe('updating');
      expect(CollectionStatus.DELETING).toBe('deleting');
      expect(CollectionStatus.ERROR).toBe('error');
    });

    it('should define QdrantEventType enum', () => {
      expect(QdrantEventType.CONNECTING).toBe('connecting');
      expect(QdrantEventType.CONNECTED).toBe('connected');
      expect(QdrantEventType.DISCONNECTED).toBe('disconnected');
      expect(QdrantEventType.COLLECTION_CREATED).toBe('collection_created');
      expect(QdrantEventType.COLLECTION_DELETED).toBe('collection_deleted');
      expect(QdrantEventType.VECTORS_UPSERTED).toBe('vectors_upserted');
      expect(QdrantEventType.VECTORS_SEARCHED).toBe('vectors_searched');
      expect(QdrantEventType.POINTS_DELETED).toBe('points_deleted');
      expect(QdrantEventType.ERROR).toBe('error');
    });
  });

  describe('Type aliases', () => {
    it('should define VectorDistance type', () => {
      const distance: VectorDistance = 'Cosine';
      expect(distance).toBe('Cosine');

      // Test all supported distances
      const distances: VectorDistance[] = ['Cosine', 'Euclid', 'Dot', 'Manhattan'];
      expect(distances).toEqual(SUPPORTED_VECTOR_DISTANCES);
    });
  });

  describe('Constants', () => {
    it('should define DEFAULT_QDRANT_CONFIG', () => {
      expect(DEFAULT_QDRANT_CONFIG.useHttps).toBe(false);
      expect(DEFAULT_QDRANT_CONFIG.timeout).toBe(30000);
      expect(DEFAULT_QDRANT_CONFIG.port).toBe(6333);
    });

    it('should define DEFAULT_COLLECTION_OPTIONS', () => {
      expect(DEFAULT_COLLECTION_OPTIONS.distance).toBe('Cosine');
      expect(DEFAULT_COLLECTION_OPTIONS.recreateIfExists).toBe(false);
      expect(DEFAULT_COLLECTION_OPTIONS.optimizersConfig).toBeDefined();
      expect(DEFAULT_COLLECTION_OPTIONS.replicationFactor).toBe(1);
      expect(DEFAULT_COLLECTION_OPTIONS.writeConsistencyFactor).toBe(1);

      // Check payload indexes
      expect(DEFAULT_COLLECTION_OPTIONS.payloadIndexes).toContain('language');
      expect(DEFAULT_COLLECTION_OPTIONS.payloadIndexes).toContain('chunkType');
      expect(DEFAULT_COLLECTION_OPTIONS.payloadIndexes).toContain('filePath');
      expect(DEFAULT_COLLECTION_OPTIONS.payloadIndexes).toContain('projectId');
      expect(DEFAULT_COLLECTION_OPTIONS.payloadIndexes).toContain('snippetMetadata.snippetType');
    });

    it('should define DEFAULT_VECTOR_UPSERT_OPTIONS', () => {
      expect(DEFAULT_VECTOR_UPSERT_OPTIONS.batchSize).toBe(100);
      expect(DEFAULT_VECTOR_UPSERT_OPTIONS.validateDimensions).toBe(true);
      expect(DEFAULT_VECTOR_UPSERT_OPTIONS.skipInvalidPoints).toBe(false);
    });

    it('should define DEFAULT_VECTOR_SEARCH_OPTIONS', () => {
      expect(DEFAULT_VECTOR_SEARCH_OPTIONS.limit).toBe(10);
      expect(DEFAULT_VECTOR_SEARCH_OPTIONS.withPayload).toBe(true);
      expect(DEFAULT_VECTOR_SEARCH_OPTIONS.withVector).toBe(false);
      expect(DEFAULT_VECTOR_SEARCH_OPTIONS.useHybridSearch).toBe(false);
      expect(DEFAULT_VECTOR_SEARCH_OPTIONS.exactSearch).toBe(false);
      expect(DEFAULT_VECTOR_SEARCH_OPTIONS.searchStrategy).toBe('default');
    });

    it('should define SUPPORTED_VECTOR_DISTANCES', () => {
      expect(SUPPORTED_VECTOR_DISTANCES).toEqual(['Cosine', 'Euclid', 'Dot', 'Manhattan']);
    });

    it('should define SUPPORTED_PAYLOAD_INDEX_TYPES', () => {
      expect(SUPPORTED_PAYLOAD_INDEX_TYPES).toEqual([
        'keyword',
        'integer',
        'float',
        'bool',
        'datetime'
      ]);
    });

    it('should define COMMON_PAYLOAD_INDEX_FIELDS', () => {
      expect(COMMON_PAYLOAD_INDEX_FIELDS).toEqual([
        { fieldName: 'language', fieldType: 'keyword' },
        { fieldName: 'chunkType', fieldType: 'keyword' },
        { fieldName: 'filePath', fieldType: 'keyword' },
        { fieldName: 'projectId', fieldType: 'keyword' },
        { fieldName: 'snippetMetadata.snippetType', fieldType: 'keyword' }
      ]);
    });

    it('should define ERROR_MESSAGES', () => {
      expect(ERROR_MESSAGES.CONNECTION_FAILED).toBe('Failed to connect to Qdrant');
      expect(ERROR_MESSAGES.COLLECTION_NOT_FOUND).toBe('Collection not found');
      expect(ERROR_MESSAGES.COLLECTION_ALREADY_EXISTS).toBe('Collection already exists');
      expect(ERROR_MESSAGES.INVALID_VECTOR_DIMENSIONS).toBe('Invalid vector dimensions');
      expect(ERROR_MESSAGES.VECTOR_DIMENSION_MISMATCH).toBe('Vector dimension mismatch');
      expect(ERROR_MESSAGES.INVALID_POINT_ID).toBe('Invalid point ID');
      expect(ERROR_MESSAGES.INVALID_VECTOR_DATA).toBe('Invalid vector data');
      expect(ERROR_MESSAGES.INVALID_PAYLOAD_DATA).toBe('Invalid payload data');
      expect(ERROR_MESSAGES.UPSERT_FAILED).toBe('Failed to upsert vectors');
      expect(ERROR_MESSAGES.SEARCH_FAILED).toBe('Failed to search vectors');
      expect(ERROR_MESSAGES.DELETE_FAILED).toBe('Failed to delete points');
      expect(ERROR_MESSAGES.INDEX_CREATION_FAILED).toBe('Failed to create payload index');
      expect(ERROR_MESSAGES.PROJECT_NOT_FOUND).toBe('Project not found');
      expect(ERROR_MESSAGES.COLLECTION_NAME_NOT_FOUND).toBe('Collection name not found');
    });
  });
});