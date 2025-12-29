import { QdrantVectorOperations, IQdrantVectorOperations } from '../../qdrant/QdrantVectorOperations';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { IQdrantConnectionManager } from '../../qdrant/QdrantConnectionManager';
import { IQdrantCollectionManager } from '../../qdrant/QdrantCollectionManager';
import { VectorPoint, SearchOptions, SearchResult } from '../../qdrant/IVectorStore';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import {
  VectorUpsertOptions,
  VectorSearchOptions,
  BatchResult,
  CollectionInfo
} from '../../qdrant/QdrantTypes';

// Mock dependencies
const mockLogger = {
  debug: jest.fn(),
  info: jest.fn(),
  warn: jest.fn(),
  error: jest.fn(),
};

const mockErrorHandler = {
  handleError: jest.fn(),
};

const mockConnectionManager = {
  getClient: jest.fn(),
};

const mockCollectionManager = {
  getCollectionInfo: jest.fn(),
};

const mockClient = {
  upsert: jest.fn(),
  search: jest.fn(),
  delete: jest.fn(),
  getCollection: jest.fn(),
};

const mockDatabaseLogger = {
  logDatabaseEvent: jest.fn(),
  logConnectionEvent: jest.fn(),
  logBatchOperation: jest.fn(),
  logCollectionOperation: jest.fn(),
  logVectorOperation: jest.fn(),
  logQueryOperation: jest.fn(),
  logProjectOperation: jest.fn(),
};

const mockPerformanceMonitor = {
  recordOperation: jest.fn(),
  getOperationStats: jest.fn(),
};

describe('QdrantVectorOperations', () => {
  let vectorOperations: QdrantVectorOperations;

  beforeEach(() => {
    // Reset all mocks before each test
    jest.clearAllMocks();

    // Set up mock connection manager to return mock client
    mockConnectionManager.getClient.mockReturnValue(mockClient);

    // Create a new instance of QdrantVectorOperations with mocked dependencies
    vectorOperations = new QdrantVectorOperations(
      mockLogger as unknown as LoggerService,
      mockErrorHandler as unknown as ErrorHandlerService,
      mockConnectionManager as unknown as IQdrantConnectionManager,
      mockCollectionManager as unknown as IQdrantCollectionManager,
      mockDatabaseLogger as unknown as DatabaseLoggerService,
      mockPerformanceMonitor as unknown as PerformanceMonitor
    );
  });

  describe('upsertVectors', () => {
    it('should upsert vectors successfully', async () => {
      const collectionName = 'test-collection';
      const vectors: VectorPoint[] = [
        {
          id: '1',
          vector: [0.1, 0.2, 0.3],
          payload: {
            content: 'test content',
            filePath: '/test/file.ts',
            language: 'typescript',
            chunkType: ['function'],
            startLine: 1,
            endLine: 10,
            metadata: {},
            timestamp: new Date(),
          }
        }
      ];

      mockClient.upsert.mockResolvedValue(undefined);

      const result = await vectorOperations.upsertVectors(collectionName, vectors);

      expect(result).toBe(true);
      expect(mockClient.upsert).toHaveBeenCalledWith(collectionName, {
        points: [
          {
            id: expect.any(Number), // ID会被转换为数字
            vector: [0.1, 0.2, 0.3],
            payload: expect.any(Object)
          }
        ]
      });
    });

    it('should return false when upserting vectors fails', async () => {
      const collectionName = 'test-collection';
      const vectors: VectorPoint[] = [
        {
          id: '1',
          vector: [0.1, 0.2, 0.3],
          payload: {
            content: 'test content',
            filePath: '/test/file.ts',
            language: 'typescript',
            chunkType: ['function'],
            startLine: 1,
            endLine: 10,
            metadata: {},
            timestamp: new Date(),
          }
        }
      ];

      mockClient.upsert.mockRejectedValue(new Error('Upsert failed'));

      const result = await vectorOperations.upsertVectors(collectionName, vectors);

      expect(result).toBe(false);
    });
  });

  describe('upsertVectorsWithOptions', () => {
    it('should upsert vectors with options successfully', async () => {
      const collectionName = 'test-collection';
      const vectors: VectorPoint[] = [
        {
          id: '1',
          vector: [0.1, 0.2, 0.3],
          payload: {
            content: 'test content',
            filePath: '/test/file.ts',
            language: 'typescript',
            chunkType: ['function'],
            startLine: 1,
            endLine: 10,
            metadata: {},
            timestamp: new Date(),
          }
        }
      ];
      const options: VectorUpsertOptions = {
        batchSize: 100,
        validateDimensions: true
      };

      mockClient.upsert.mockResolvedValue(undefined);
      mockCollectionManager.getCollectionInfo.mockResolvedValue({
        name: collectionName,
        vectors: {
          size: 3,
          distance: 'Cosine'
        },
        pointsCount: 0,
        status: 'green'
      });

      const result = await vectorOperations.upsertVectorsWithOptions(collectionName, vectors, options);

      expect(result).toEqual({
        success: true,
        processedCount: 1,
        failedCount: 0,
        errors: []
      });
    });

    it('should handle batch processing', async () => {
      const collectionName = 'test-collection';
      const vectors: VectorPoint[] = Array.from({ length: 150 }, (_, i) => ({
        id: `${i}`,
        vector: Array(128).fill(0.1),
        payload: {
          content: `test content ${i}`,
          filePath: `/test/file${i}.ts`,
          language: 'typescript',
          chunkType: ['function'],
          startLine: 1,
          endLine: 10,
          metadata: {},
          timestamp: new Date(),
        }
      }));
      const options: VectorUpsertOptions = {
        batchSize: 100
      };

      mockClient.upsert.mockResolvedValue(undefined);

      const result = await vectorOperations.upsertVectorsWithOptions(collectionName, vectors, options);

      // Should process in 2 batches
      expect(mockClient.upsert).toHaveBeenCalledTimes(2);
      expect(result).toEqual({
        success: true,
        processedCount: 150,
        failedCount: 0,
        errors: []
      });
    });
  });

  describe('searchVectors', () => {
    it('should search vectors successfully', async () => {
      const collectionName = 'test-collection';
      const query = [0.1, 0.2, 0.3];
      const options: SearchOptions = { limit: 10 };
      const searchResults = [
        {
          id: '1',
          version: 0,
          score: 0.95,
          payload: {
            content: 'test content',
            filePath: '/test/file.ts',
            language: 'typescript',
            chunkType: ['function'],
            startLine: 1,
            endLine: 10,
            metadata: {},
            timestamp: '2023-01-01T00:00:00.000Z',
          },
          vector: null
        }
      ];

      mockClient.search.mockResolvedValue(searchResults);

      const result = await vectorOperations.searchVectors(collectionName, query, options);

      expect(result).toEqual([
        {
          id: '1',
          score: 0.95,
          payload: expect.objectContaining({
            content: 'test content',
            filePath: '/test/file.ts',
            language: 'typescript',
            chunkType: ['function'],
            startLine: 1,
            endLine: 10,
            metadata: {},
            timestamp: new Date('2023-01-01T00:00:00.000Z') // 时间戳被转换为Date对象
          })
        }
      ]);
      expect(mockClient.search).toHaveBeenCalledWith(collectionName, {
        vector: query,
        limit: 10,
        with_payload: true,
        with_vector: false
      });
    });

    it('should return empty array when searching vectors fails', async () => {
      const collectionName = 'test-collection';
      const query = [0.1, 0.2, 0.3];

      mockClient.search.mockRejectedValue(new Error('Search failed'));

      const result = await vectorOperations.searchVectors(collectionName, query);

      expect(result).toEqual([]);
    });
  });

  describe('searchVectorsWithOptions', () => {
    it('should search vectors with options successfully', async () => {
      const collectionName = 'test-collection';
      const query = [0.1, 0.2, 0.3];
      const options: VectorSearchOptions = {
        limit: 5,
        withPayload: false,
        withVector: true,
        scoreThreshold: 0.8
      };
      const searchResults = [
        {
          id: '1',
          version: 0,
          score: 0.95,
          payload: null,
          vector: [0.1, 0.2, 0.3]
        }
      ];

      mockClient.search.mockResolvedValue(searchResults);

      const result = await vectorOperations.searchVectorsWithOptions(collectionName, query, options);

      expect(result).toEqual([
        {
          id: '1',
          score: 0.95,
          payload: {
            timestamp: expect.any(Date)
          }
        }
      ]);
      expect(mockClient.search).toHaveBeenCalledWith(collectionName, {
        vector: query,
        limit: 5,
        with_payload: false,
        with_vector: true,
        score_threshold: 0.8
      });
    });
  });

  describe('deletePoints', () => {
    it('should delete points successfully', async () => {
      const collectionName = 'test-collection';
      const pointIds = ['1', '2', '3'];

      mockClient.delete.mockResolvedValue(undefined);

      const result = await vectorOperations.deletePoints(collectionName, pointIds);

      expect(result).toBe(true);
      expect(mockClient.delete).toHaveBeenCalledWith(collectionName, {
        filter: {
          must: [
            {
              key: 'id',
              match: {
                any: pointIds
              }
            }
          ]
        }
      });
    });

    it('should return false when deleting points fails', async () => {
      const collectionName = 'test-collection';
      const pointIds = ['1', '2', '3'];

      mockClient.delete.mockRejectedValue(new Error('Delete failed'));

      const result = await vectorOperations.deletePoints(collectionName, pointIds);

      expect(result).toBe(false);
    });
  });

  describe('clearCollection', () => {
    it('should clear collection successfully', async () => {
      const collectionName = 'test-collection';

      mockClient.delete.mockResolvedValue(undefined);

      const result = await vectorOperations.clearCollection(collectionName);

      expect(result).toBe(true);
      expect(mockClient.delete).toHaveBeenCalledWith(collectionName, {
        filter: {
          must: [
            {
              key: 'id',
              match: {
                any: true
              }
            }
          ]
        }
      });
    });

    it('should return false when clearing collection fails', async () => {
      const collectionName = 'test-collection';

      mockClient.delete.mockRejectedValue(new Error('Clear failed'));

      const result = await vectorOperations.clearCollection(collectionName);

      expect(result).toBe(false);
    });
  });

  describe('getPointCount', () => {
    it('should get point count successfully', async () => {
      const collectionName = 'test-collection';
      const collectionInfo = {
        points_count: 42,
        config: {
          params: {
            vectors: {
              size: 128,
              distance: 'Cosine'
            }
          }
        },
        status: 'green'
      };

      mockClient.getCollection.mockResolvedValue(collectionInfo);

      const result = await vectorOperations.getPointCount(collectionName);

      expect(result).toBe(42);
    });

    it('should return 0 when getting point count fails', async () => {
      const collectionName = 'test-collection';

      mockClient.getCollection.mockRejectedValue(new Error('Get collection failed'));

      const result = await vectorOperations.getPointCount(collectionName);

      expect(result).toBe(0);
    });
  });

  describe('validateVectors', () => {
    it('should validate vectors successfully', async () => {
      const collectionName = 'test-collection';
      const vectors: VectorPoint[] = [
        {
          id: '1',
          vector: [0.1, 0.2, 0.3],
          payload: {
            content: 'test content',
            filePath: '/test/file.ts',
            language: 'typescript',
            chunkType: ['function'],
            startLine: 1,
            endLine: 10,
            metadata: {},
            timestamp: new Date(),
          }
        }
      ];

      mockCollectionManager.getCollectionInfo.mockResolvedValue({
        name: collectionName,
        vectors: {
          size: 3,
          distance: 'Cosine'
        },
        pointsCount: 0,
        status: 'green'
      });

      // Call private method through reflection for testing
      const validateVectors = (vectorOperations as any).validateVectors.bind(vectorOperations);
      await expect(validateVectors(collectionName, vectors)).resolves.toBeUndefined();
    });

    it('should throw error for inconsistent vector dimensions', async () => {
      const collectionName = 'test-collection';
      const vectors: VectorPoint[] = [
        {
          id: '1',
          vector: [0.1, 0.2, 0.3],
          payload: {
            content: 'test content',
            filePath: '/test/file.ts',
            language: 'typescript',
            chunkType: ['function'],
            startLine: 1,
            endLine: 10,
            metadata: {},
            timestamp: new Date(),
          }
        },
        {
          id: '2',
          vector: [0.1, 0.2], // Inconsistent dimension
          payload: {
            content: 'test content 2',
            filePath: '/test/file2.ts',
            language: 'typescript',
            chunkType: ['function'],
            startLine: 1,
            endLine: 10,
            metadata: {},
            timestamp: new Date(),
          }
        }
      ];

      // Call private method through reflection for testing
      const validateVectors = (vectorOperations as any).validateVectors.bind(vectorOperations);
      await expect(validateVectors(collectionName, vectors)).rejects.toThrow('Invalid vector dimensions');
    });
  });

  describe('processPoint', () => {
    it('should process point correctly', () => {
      const point: VectorPoint = {
        id: '1',
        vector: [0.1, 0.2, 0.3],
        payload: {
          content: 'test content',
          filePath: '/test/file.ts',
          language: 'typescript',
          chunkType: ['function'],
          startLine: 1,
          endLine: 10,
          metadata: {},
          timestamp: new Date(),
        }
      };

      // Call private method through reflection for testing
      const processPoint = (vectorOperations as any).processPoint.bind(vectorOperations);
      const result = processPoint(point);

      expect(result).toEqual({
        id: expect.any(Number), // ID会被转换为数字
        vector: [0.1, 0.2, 0.3],
        payload: expect.any(Object)
      });
    });

    it('should throw error for invalid vector data', () => {
      const point: any = {
        id: '1',
        vector: 'invalid-vector', // Invalid vector data
        payload: {
          content: 'test content',
          filePath: '/test/file.ts',
          language: 'typescript',
          chunkType: ['function'],
          startLine: 1,
          endLine: 10,
          metadata: {},
          timestamp: new Date(),
        }
      };

      // Call private method through reflection for testing
      const processPoint = (vectorOperations as any).processPoint.bind(vectorOperations);
      expect(() => processPoint(point)).toThrow('Invalid vector data');
    });
  });
});