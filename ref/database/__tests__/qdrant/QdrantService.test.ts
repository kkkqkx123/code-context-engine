import { QdrantService } from '../../qdrant/QdrantService';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { ConfigService } from '../../../config/ConfigService';
import { ProjectIdManager } from '../../ProjectIdManager';
import { IQdrantConnectionManager } from '../../qdrant/QdrantConnectionManager';
import { IQdrantCollectionManager } from '../../qdrant/QdrantCollectionManager';
import { IQdrantVectorOperations } from '../../qdrant/QdrantVectorOperations';
import { IQdrantQueryUtils } from '../../qdrant/QdrantQueryUtils';
import { IQdrantProjectManager } from '../../qdrant/QdrantProjectManager';
import { IVectorStore, VectorPoint, CollectionInfo, SearchOptions, SearchResult } from '../../qdrant/IVectorStore';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import {
  VectorDistance,
  QdrantEventType,
} from '../../qdrant/QdrantTypes';

// Mock dependencies
const mockLogger = {
  info: jest.fn(),
  warn: jest.fn(),
  error: jest.fn(),
};

const mockErrorHandler = {
  handleError: jest.fn(),
};


const mockConfigService = {
  get: jest.fn(),
};

const mockProjectIdManager = {};

const mockConnectionManager = {
  initialize: jest.fn(),
  connect: jest.fn(),
  disconnect: jest.fn(),
  close: jest.fn(),
  isConnected: jest.fn(),
  getConnectionStatus: jest.fn(),
  getConfig: jest.fn(),
  updateConfig: jest.fn(),
  subscribe: jest.fn(),
};

const mockCollectionManager = {
  createCollection: jest.fn(),
  createCollectionWithOptions: jest.fn(),
  collectionExists: jest.fn(),
  deleteCollection: jest.fn(),
  getCollectionInfo: jest.fn(),
  getCollectionStats: jest.fn(),
  createPayloadIndex: jest.fn(),
  createPayloadIndexes: jest.fn(),
  listCollections: jest.fn(),
  subscribe: jest.fn(),
};

const mockVectorOperations = {
  upsertVectors: jest.fn(),
  upsertVectorsWithOptions: jest.fn(),
  searchVectors: jest.fn(),
  searchVectorsWithOptions: jest.fn(),
  deletePoints: jest.fn(),
  clearCollection: jest.fn(),
  getPointCount: jest.fn(),
  subscribe: jest.fn(),
};

const mockQueryUtils = {
  getChunkIdsByFiles: jest.fn(),
  getExistingChunkIds: jest.fn(),
  scrollPoints: jest.fn(),
  countPoints: jest.fn(),
  buildFilter: jest.fn(),
  buildAdvancedFilter: jest.fn(),
  subscribe: jest.fn(),
};

const mockProjectManager = {
  createCollectionForProject: jest.fn(),
  upsertVectorsForProject: jest.fn(),
  searchVectorsForProject: jest.fn(),
  getCollectionInfoForProject: jest.fn(),
  deleteCollectionForProject: jest.fn(),
  getProjectInfo: jest.fn(),
  listProjects: jest.fn(),
  deleteVectorsForProject: jest.fn(),
  clearProject: jest.fn(),
  subscribe: jest.fn(),
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

describe('QdrantService', () => {
  let qdrantService: QdrantService;

  beforeEach(() => {
    // Reset all mocks before each test
    jest.clearAllMocks();

    // Create a new instance of QdrantService with mocked dependencies
    qdrantService = new QdrantService(
      mockConfigService as unknown as ConfigService,
      mockLogger as unknown as LoggerService,
      mockErrorHandler as unknown as ErrorHandlerService,
      mockProjectIdManager as unknown as ProjectIdManager,
      mockConnectionManager as unknown as IQdrantConnectionManager,
      mockCollectionManager as unknown as IQdrantCollectionManager,
      mockVectorOperations as unknown as IQdrantVectorOperations,
      mockQueryUtils as unknown as IQdrantQueryUtils,
      mockProjectManager as unknown as IQdrantProjectManager,
      mockDatabaseLogger as unknown as DatabaseLoggerService,
      mockPerformanceMonitor as unknown as PerformanceMonitor
    );
  });

  describe('initialize', () => {
    it('should initialize the service', async () => {
      mockConnectionManager.initialize.mockResolvedValue(true);
      mockConnectionManager.connect.mockResolvedValue(true);

      const result = await qdrantService.initialize();

      expect(result).toBe(true);
      expect(mockConnectionManager.initialize).toHaveBeenCalled();
    });
  });

  describe('createCollection', () => {
    it('should create a collection', async () => {
      const collectionName = 'test-collection';
      const vectorSize = 128;
      const distance: VectorDistance = 'Cosine';
      const recreateIfExists = false;

      mockCollectionManager.createCollection.mockResolvedValue(true);

      const result = await qdrantService.createCollection(collectionName, vectorSize, distance, recreateIfExists);

      expect(result).toBe(true);
      expect(mockCollectionManager.createCollection).toHaveBeenCalledWith(
        collectionName,
        vectorSize,
        distance,
        recreateIfExists
      );
    });
  });

  describe('collectionExists', () => {
    it('should check if collection exists', async () => {
      const collectionName = 'test-collection';

      mockCollectionManager.collectionExists.mockResolvedValue(true);

      const result = await qdrantService.collectionExists(collectionName);

      expect(result).toBe(true);
      expect(mockCollectionManager.collectionExists).toHaveBeenCalledWith(collectionName);
    });
  });

  describe('deleteCollection', () => {
    it('should delete a collection', async () => {
      const collectionName = 'test-collection';

      mockCollectionManager.deleteCollection.mockResolvedValue(true);

      const result = await qdrantService.deleteCollection(collectionName);

      expect(result).toBe(true);
      expect(mockCollectionManager.deleteCollection).toHaveBeenCalledWith(collectionName);
    });
  });

  describe('getCollectionInfo', () => {
    it('should get collection info', async () => {
      const collectionName = 'test-collection';
      const collectionInfo: CollectionInfo = {
        name: collectionName,
        vectors: {
          size: 128,
          distance: 'Cosine'
        },
        pointsCount: 100,
        status: 'green'
      };

      mockCollectionManager.getCollectionInfo.mockResolvedValue(collectionInfo);

      const result = await qdrantService.getCollectionInfo(collectionName);

      expect(result).toEqual(collectionInfo);
      expect(mockCollectionManager.getCollectionInfo).toHaveBeenCalledWith(collectionName);
    });
  });

  describe('upsertVectors', () => {
    it('should upsert vectors', async () => {
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

      mockVectorOperations.upsertVectors.mockResolvedValue(true);

      const result = await qdrantService.upsertVectors(collectionName, vectors);

      expect(result).toBe(true);
      expect(mockVectorOperations.upsertVectors).toHaveBeenCalledWith(collectionName, vectors);
    });
  });

  describe('searchVectors', () => {
    it('should search vectors', async () => {
      const collectionName = 'test-collection';
      const query = [0.1, 0.2, 0.3];
      const options: SearchOptions = { limit: 10 };
      const searchResults: SearchResult[] = [
        {
          id: '1',
          score: 0.95,
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

      mockVectorOperations.searchVectors.mockResolvedValue(searchResults);

      const result = await qdrantService.searchVectors(collectionName, query, options);

      expect(result).toEqual(searchResults);
      expect(mockVectorOperations.searchVectors).toHaveBeenCalledWith(collectionName, query, options);
    });
  });

  describe('deletePoints', () => {
    it('should delete points', async () => {
      const collectionName = 'test-collection';
      const pointIds = ['1', '2', '3'];

      mockVectorOperations.deletePoints.mockResolvedValue(true);

      const result = await qdrantService.deletePoints(collectionName, pointIds);

      expect(result).toBe(true);
      expect(mockVectorOperations.deletePoints).toHaveBeenCalledWith(collectionName, pointIds);
    });
  });

  describe('clearCollection', () => {
    it('should clear collection', async () => {
      const collectionName = 'test-collection';

      mockVectorOperations.clearCollection.mockResolvedValue(true);

      const result = await qdrantService.clearCollection(collectionName);

      expect(result).toBe(true);
      expect(mockVectorOperations.clearCollection).toHaveBeenCalledWith(collectionName);
    });
  });

  describe('getPointCount', () => {
    it('should get point count', async () => {
      const collectionName = 'test-collection';
      const count = 42;

      mockVectorOperations.getPointCount.mockResolvedValue(count);

      const result = await qdrantService.getPointCount(collectionName);

      expect(result).toBe(count);
      expect(mockVectorOperations.getPointCount).toHaveBeenCalledWith(collectionName);
    });
  });

  describe('createCollectionForProject', () => {
    it('should create collection for project', async () => {
      const projectPath = '/test/project';
      const vectorSize = 128;
      const distance: VectorDistance = 'Cosine';

      mockProjectManager.createCollectionForProject.mockResolvedValue(true);

      const result = await qdrantService.createCollectionForProject(projectPath, vectorSize, distance);

      expect(result).toBe(true);
      expect(mockProjectManager.createCollectionForProject).toHaveBeenCalledWith(
        projectPath,
        vectorSize,
        distance
      );
    });
  });

  describe('upsertVectorsForProject', () => {
    it('should upsert vectors for project', async () => {
      const projectPath = '/test/project';
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

      mockProjectManager.upsertVectorsForProject.mockResolvedValue(true);

      const result = await qdrantService.upsertVectorsForProject(projectPath, vectors);

      expect(result).toBe(true);
      expect(mockProjectManager.upsertVectorsForProject).toHaveBeenCalledWith(projectPath, vectors);
    });
  });

  describe('searchVectorsForProject', () => {
    it('should search vectors for project', async () => {
      const projectPath = '/test/project';
      const query = [0.1, 0.2, 0.3];
      const options: SearchOptions = { limit: 10 };
      const searchResults: SearchResult[] = [
        {
          id: '1',
          score: 0.95,
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

      mockProjectManager.searchVectorsForProject.mockResolvedValue(searchResults);

      const result = await qdrantService.searchVectorsForProject(projectPath, query, options);

      expect(result).toEqual(searchResults);
      expect(mockProjectManager.searchVectorsForProject).toHaveBeenCalledWith(
        projectPath,
        query,
        options
      );
    });
  });

  describe('isConnected', () => {
    it('should check if connected', () => {
      mockConnectionManager.isConnected.mockReturnValue(true);

      const result = qdrantService.isConnected();

      expect(result).toBe(true);
      expect(mockConnectionManager.isConnected).toHaveBeenCalled();
    });
  });

  describe('close', () => {
    it('should close the connection', async () => {
      mockConnectionManager.disconnect.mockResolvedValue(undefined);
      mockConnectionManager.close.mockResolvedValue(undefined);

      await qdrantService.close();

      expect(mockConnectionManager.disconnect).toHaveBeenCalled();
    });
  });

  describe('subscribe', () => {
    it('should subscribe to events from all modules', () => {
      const type: QdrantEventType = QdrantEventType.CONNECTED;
      const listener = jest.fn();
      const mockSubscription = {
        id: 'test-id',
        eventType: type,
        handler: listener,
        unsubscribe: jest.fn()
      };

      // Mock subscribe to return subscription objects
      mockConnectionManager.subscribe.mockReturnValue(mockSubscription);
      mockCollectionManager.subscribe.mockReturnValue(mockSubscription);
      mockVectorOperations.subscribe.mockReturnValue(mockSubscription);
      mockQueryUtils.subscribe.mockReturnValue(mockSubscription);
      mockProjectManager.subscribe.mockReturnValue(mockSubscription);

      const subscription = qdrantService.subscribe(type, listener);

      expect(mockConnectionManager.subscribe).toHaveBeenCalledWith(type, listener);
      expect(mockCollectionManager.subscribe).toHaveBeenCalledWith(type, listener);
      expect(mockVectorOperations.subscribe).toHaveBeenCalledWith(type, listener);
      expect(mockQueryUtils.subscribe).toHaveBeenCalledWith(type, listener);
      expect(mockProjectManager.subscribe).toHaveBeenCalledWith(type, listener);

      expect(subscription).toHaveProperty('unsubscribe');
    });
  });
});