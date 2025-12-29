import { QdrantCollectionManager, IQdrantCollectionManager } from '../../qdrant/QdrantCollectionManager';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { IQdrantConnectionManager } from '../../qdrant/QdrantConnectionManager';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { VectorDistance, CollectionCreateOptions, DEFAULT_COLLECTION_OPTIONS } from '../../qdrant/QdrantTypes';

// Mock dependencies
const mockLogger = {
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

const mockClient = {
  createCollection: jest.fn(),
  deleteCollection: jest.fn(),
  getCollections: jest.fn(),
  getCollection: jest.fn(),
  createPayloadIndex: jest.fn(),
};

describe('QdrantCollectionManager', () => {
  let collectionManager: QdrantCollectionManager;

  beforeEach(() => {
    // Reset all mocks before each test
    jest.clearAllMocks();

    // Set up mock connection manager to return mock client
    mockConnectionManager.getClient.mockReturnValue(mockClient);

    // Create a new instance of QdrantCollectionManager with mocked dependencies
    collectionManager = new QdrantCollectionManager(
      mockLogger as unknown as LoggerService,
      mockErrorHandler as unknown as ErrorHandlerService,
      mockConnectionManager as unknown as IQdrantConnectionManager,
      mockDatabaseLogger as unknown as DatabaseLoggerService,
      mockPerformanceMonitor as unknown as PerformanceMonitor
    );
  });

  describe('createCollection', () => {
    it('should create a collection with default options', async () => {
      const collectionName = 'test-collection';
      const vectorSize = 128;
      const distance: VectorDistance = 'Cosine';

      // Mock collectionExists to return false
      collectionManager.collectionExists = jest.fn().mockResolvedValue(false);

      // Mock createPayloadIndex to return true
      collectionManager.createPayloadIndex = jest.fn().mockResolvedValue(true);

      const result = await collectionManager.createCollection(collectionName, vectorSize, distance);

      expect(result).toBe(true);
      expect(mockClient.createCollection).toHaveBeenCalledWith(collectionName, {
        vectors: {
          size: vectorSize,
          distance: distance,
        },
        optimizers_config: DEFAULT_COLLECTION_OPTIONS.optimizersConfig,
        replication_factor: DEFAULT_COLLECTION_OPTIONS.replicationFactor,
        write_consistency_factor: DEFAULT_COLLECTION_OPTIONS.writeConsistencyFactor,
      });
    });

    it('should return true if collection already exists with correct vector size', async () => {
      const collectionName = 'test-collection';
      const vectorSize = 128;
      const distance: VectorDistance = 'Cosine';

      // Mock collectionExists to return true
      collectionManager.collectionExists = jest.fn().mockResolvedValue(true);

      // Mock getCollectionInfo to return matching vector size
      collectionManager.getCollectionInfo = jest.fn().mockResolvedValue({
        name: collectionName,
        vectors: {
          size: vectorSize,
          distance: distance,
        },
        pointsCount: 0,
        status: 'green',
      });

      const result = await collectionManager.createCollection(collectionName, vectorSize, distance);

      expect(result).toBe(true);
      expect(mockClient.createCollection).not.toHaveBeenCalled();
    });
  });

  describe('collectionExists', () => {
    it('should return true if collection exists', async () => {
      const collectionName = 'test-collection';

      mockClient.getCollections.mockResolvedValue({
        collections: [{ name: collectionName }]
      });

      const result = await collectionManager.collectionExists(collectionName);

      expect(result).toBe(true);
    });

    it('should return false if collection does not exist', async () => {
      const collectionName = 'test-collection';

      mockClient.getCollections.mockResolvedValue({
        collections: [{ name: 'other-collection' }]
      });

      const result = await collectionManager.collectionExists(collectionName);

      expect(result).toBe(false);
    });

    it('should return false if client is not available', async () => {
      const collectionName = 'test-collection';

      mockConnectionManager.getClient.mockReturnValue(null);

      const result = await collectionManager.collectionExists(collectionName);

      expect(result).toBe(false);
    });
  });

  describe('deleteCollection', () => {
    it('should delete a collection', async () => {
      const collectionName = 'test-collection';

      const result = await collectionManager.deleteCollection(collectionName);

      expect(result).toBe(true);
      expect(mockClient.deleteCollection).toHaveBeenCalledWith(collectionName);
    });

    it('should return false if client is not available', async () => {
      const collectionName = 'test-collection';

      mockConnectionManager.getClient.mockReturnValue(null);

      const result = await collectionManager.deleteCollection(collectionName);

      expect(result).toBe(false);
      expect(mockErrorHandler.handleError).toHaveBeenCalled();
    });
  });

  describe('getCollectionInfo', () => {
    it('should return collection info', async () => {
      const collectionName = 'test-collection';
      const mockCollectionInfo = {
        config: {
          params: {
            vectors: {
              size: 128,
              distance: 'Cosine'
            }
          }
        },
        points_count: 100,
        status: 'green'
      };

      mockClient.getCollection.mockResolvedValue(mockCollectionInfo);

      const result = await collectionManager.getCollectionInfo(collectionName);

      expect(result).toEqual({
        name: collectionName,
        vectors: {
          size: 128,
          distance: 'Cosine'
        },
        pointsCount: 100,
        status: 'green'
      });
    });

    it('should return null if client is not available', async () => {
      const collectionName = 'test-collection';

      mockConnectionManager.getClient.mockReturnValue(null);

      const result = await collectionManager.getCollectionInfo(collectionName);

      expect(result).toBeNull();
    });
  });
});