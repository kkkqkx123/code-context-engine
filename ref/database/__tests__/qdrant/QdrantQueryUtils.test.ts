import { QdrantQueryUtils, IQdrantQueryUtils } from '../../qdrant/QdrantQueryUtils';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { IQdrantConnectionManager } from '../../qdrant/QdrantConnectionManager';
import { SearchOptions } from '../../qdrant/IVectorStore';
import { QueryFilter } from '../../qdrant/QdrantTypes';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';

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

const mockClient = {
  scroll: jest.fn(),
  count: jest.fn(),
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

describe('QdrantQueryUtils', () => {
  let queryUtils: QdrantQueryUtils;

  beforeEach(() => {
    // Reset all mocks before each test
    jest.clearAllMocks();

    // Set up mock connection manager to return mock client
    mockConnectionManager.getClient.mockReturnValue(mockClient);

    // Create a new instance of QdrantQueryUtils with mocked dependencies
    queryUtils = new QdrantQueryUtils(
      mockLogger as unknown as LoggerService,
      mockErrorHandler as unknown as ErrorHandlerService,
      mockConnectionManager as unknown as IQdrantConnectionManager,
      mockDatabaseLogger as unknown as DatabaseLoggerService,
      mockPerformanceMonitor as unknown as PerformanceMonitor
    );
  });

  describe('buildFilter', () => {
    it('should build a filter with language', () => {
      const filter: SearchOptions['filter'] = {
        language: ['typescript', 'javascript']
      };

      const result = queryUtils.buildFilter(filter);

      expect(result).toEqual({
        must: [
          {
            key: 'language',
            match: {
              any: ['typescript', 'javascript']
            }
          }
        ]
      });
    });

    it('should build a filter with chunkType', () => {
      const filter: SearchOptions['filter'] = {
        chunkType: ['function']
      };

      const result = queryUtils.buildFilter(filter);

      expect(result).toEqual({
        must: [
          {
            key: 'chunkType',
            match: {
              any: ['function']
            }
          }
        ]
      });
    });

    it('should build a filter with filePath', () => {
      const filter: SearchOptions['filter'] = {
        filePath: ['/test/file1.ts', '/test/file2.ts']
      };

      const result = queryUtils.buildFilter(filter);

      expect(result).toEqual({
        must: [
          {
            key: 'filePath',
            match: {
              any: ['/test/file1.ts', '/test/file2.ts']
            }
          }
        ]
      });
    });

    it('should build a filter with projectId', () => {
      const filter: SearchOptions['filter'] = {
        projectId: 'test-project-id'
      };

      const result = queryUtils.buildFilter(filter);

      expect(result).toEqual({
        must: [
          {
            key: 'projectId',
            match: {
              value: 'test-project-id'
            }
          }
        ]
      });
    });

    it('should build a filter with snippetType', () => {
      const filter: SearchOptions['filter'] = {
        snippetType: ['class', 'function']
      };

      const result = queryUtils.buildFilter(filter);

      expect(result).toEqual({
        must: [
          {
            key: 'snippetMetadata.snippetType',
            match: {
              any: ['class', 'function']
            }
          }
        ]
      });
    });

    it('should return undefined when no filter is provided', () => {
      const result = queryUtils.buildFilter(undefined);
      expect(result).toBeUndefined();
    });

    it('should return undefined when filter is empty', () => {
      const filter: SearchOptions['filter'] = {};
      const result = queryUtils.buildFilter(filter);
      expect(result).toBeUndefined();
    });
  });

  describe('buildAdvancedFilter', () => {
    it('should build an advanced filter with language', () => {
      const filter: QueryFilter = {
        language: ['typescript', 'javascript']
      };

      const result = queryUtils.buildAdvancedFilter(filter);

      expect(result).toEqual({
        must: [
          {
            key: 'language',
            match: {
              any: ['typescript', 'javascript']
            }
          }
        ]
      });
    });

    it('should build an advanced filter with custom filters', () => {
      const filter: QueryFilter = {
        customFilters: {
          'metadata.version': '1.0.0',
          'metadata.tags': ['important', 'review'],
          'metadata.score': {
            gte: 0.8
          }
        }
      };

      const result = queryUtils.buildAdvancedFilter(filter);

      expect(result).toEqual({
        must: [
          {
            key: 'metadata.version',
            match: {
              value: '1.0.0'
            }
          },
          {
            key: 'metadata.tags',
            match: {
              any: ['important', 'review']
            }
          },
          {
            key: 'metadata.score',
            range: {
              gte: 0.8
            }
          }
        ]
      });
    });

    it('should return undefined when no filter is provided', () => {
      const result = queryUtils.buildAdvancedFilter(undefined);
      expect(result).toBeUndefined();
    });

    it('should return undefined when filter is empty', () => {
      const filter: QueryFilter = {};
      const result = queryUtils.buildAdvancedFilter(filter);
      expect(result).toBeUndefined();
    });
  });

  describe('getChunkIdsByFiles', () => {
    it('should get chunk IDs by files', async () => {
      const collectionName = 'test-collection';
      const filePaths = ['/test/file1.ts', '/test/file2.ts'];
      const mockPoints = [
        { id: 'chunk-1' },
        { id: 'chunk-2' }
      ];

      mockClient.scroll.mockResolvedValue({ points: mockPoints });

      const result = await queryUtils.getChunkIdsByFiles(collectionName, filePaths);

      expect(result).toEqual(['chunk-1', 'chunk-2']);
      expect(mockClient.scroll).toHaveBeenCalledWith(collectionName, {
        filter: {
          must: [
            {
              key: 'filePath',
              match: {
                any: filePaths
              }
            }
          ]
        },
        with_payload: false,
        with_vector: false,
        limit: 1000
      });
    });

    it('should return empty array when client is not available', async () => {
      const collectionName = 'test-collection';
      const filePaths = ['/test/file1.ts'];

      mockConnectionManager.getClient.mockReturnValue(null);

      const result = await queryUtils.getChunkIdsByFiles(collectionName, filePaths);

      expect(result).toEqual([]);
    });
  });

  describe('getExistingChunkIds', () => {
    it('should get existing chunk IDs', async () => {
      const collectionName = 'test-collection';
      const chunkIds = ['chunk-1', 'chunk-2'];
      const mockPoints = [
        { id: 'chunk-1' }
      ];

      mockClient.scroll.mockResolvedValue({ points: mockPoints });

      const result = await queryUtils.getExistingChunkIds(collectionName, chunkIds);

      expect(result).toEqual(['chunk-1']);
      expect(mockClient.scroll).toHaveBeenCalledWith(collectionName, {
        filter: {
          must: [
            {
              key: 'id',
              match: {
                any: chunkIds
              }
            }
          ]
        },
        with_payload: false,
        with_vector: false,
        limit: 1000
      });
    });

    it('should return empty array when client is not available', async () => {
      const collectionName = 'test-collection';
      const chunkIds = ['chunk-1'];

      mockConnectionManager.getClient.mockReturnValue(null);

      const result = await queryUtils.getExistingChunkIds(collectionName, chunkIds);

      expect(result).toEqual([]);
    });
  });

  describe('scrollPoints', () => {
    it('should scroll points', async () => {
      const collectionName = 'test-collection';
      const filter = { must: [{ key: 'language', match: { value: 'typescript' } }] };
      const limit = 50;
      const mockPoints = [{ id: 'point-1' }, { id: 'point-2' }];

      mockClient.scroll.mockResolvedValue({ points: mockPoints });

      const result = await queryUtils.scrollPoints(collectionName, filter, limit);

      expect(result).toEqual(mockPoints);
      expect(mockClient.scroll).toHaveBeenCalledWith(collectionName, {
        with_payload: true,
        with_vector: false,
        limit: 50,
        filter
      });
    });

    it('should scroll points without filter', async () => {
      const collectionName = 'test-collection';
      const limit = 50;
      const mockPoints = [{ id: 'point-1' }, { id: 'point-2' }];

      mockClient.scroll.mockResolvedValue({ points: mockPoints });

      const result = await queryUtils.scrollPoints(collectionName, undefined, limit);

      expect(result).toEqual(mockPoints);
      expect(mockClient.scroll).toHaveBeenCalledWith(collectionName, {
        with_payload: true,
        with_vector: false,
        limit: 50
      });
    });
  });

  describe('countPoints', () => {
    it('should count points', async () => {
      const collectionName = 'test-collection';
      const filter = { must: [{ key: 'language', match: { value: 'typescript' } }] };
      const mockCount = { count: 42 };

      mockClient.count.mockResolvedValue(mockCount);

      const result = await queryUtils.countPoints(collectionName, filter);

      expect(result).toBe(42);
      expect(mockClient.count).toHaveBeenCalledWith(collectionName, { filter });
    });

    it('should count points without filter', async () => {
      const collectionName = 'test-collection';
      const mockCount = { count: 42 };

      mockClient.count.mockResolvedValue(mockCount);

      const result = await queryUtils.countPoints(collectionName);

      expect(result).toBe(42);
      expect(mockClient.count).toHaveBeenCalledWith(collectionName, {});
    });
  });
});