import { QdrantProjectManager, IQdrantProjectManager } from '../../qdrant/QdrantProjectManager';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { ProjectIdManager } from '../../ProjectIdManager';
import { IQdrantCollectionManager } from '../../qdrant/QdrantCollectionManager';
import { IQdrantVectorOperations } from '../../qdrant/QdrantVectorOperations';
import { IQdrantQueryUtils } from '../../qdrant/QdrantQueryUtils';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { VectorPoint, CollectionInfo, SearchOptions, SearchResult } from '../../qdrant/IVectorStore';
import { VectorDistance, ProjectInfo } from '../../qdrant/QdrantTypes';

// Mock dependencies
const mockLogger = {
  info: jest.fn(),
  warn: jest.fn(),
  error: jest.fn(),
};

const mockErrorHandler = {
  handleError: jest.fn(),
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

const mockProjectIdManager = {
  generateProjectId: jest.fn(),
  getProjectId: jest.fn(),
  getCollectionName: jest.fn(),
  removeProject: jest.fn(),
  listAllProjects: jest.fn(),
};

const mockCollectionManager = {
  createCollection: jest.fn(),
  getCollectionInfo: jest.fn(),
  deleteCollection: jest.fn(),
};

const mockVectorOperations = {
  upsertVectors: jest.fn(),
  searchVectors: jest.fn(),
  deletePoints: jest.fn(),
  clearCollection: jest.fn(),
};

const mockQueryUtils = {
  buildFilter: jest.fn().mockImplementation((filter) => filter || {})
};

describe('QdrantProjectManager', () => {
  let projectManager: QdrantProjectManager;

  beforeEach(() => {
    // Reset all mocks before each test
    jest.clearAllMocks();

    // Create a new instance of QdrantProjectManager with mocked dependencies
    projectManager = new QdrantProjectManager(
      mockLogger as unknown as LoggerService,
      mockErrorHandler as unknown as ErrorHandlerService,
      mockDatabaseLogger as unknown as DatabaseLoggerService,
      mockPerformanceMonitor as unknown as PerformanceMonitor,
      mockProjectIdManager as unknown as ProjectIdManager,
      mockCollectionManager as unknown as IQdrantCollectionManager,
      mockVectorOperations as unknown as IQdrantVectorOperations,
      mockQueryUtils as unknown as IQdrantQueryUtils
    );
  });

  describe('createCollectionForProject', () => {
    it('should create a collection for a project', async () => {
      const projectPath = '/test/project';
      const vectorSize = 128;
      const distance: VectorDistance = 'Cosine';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';

      mockProjectIdManager.generateProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockCollectionManager.createCollection.mockResolvedValue(true);

      const result = await projectManager.createCollectionForProject(projectPath, vectorSize, distance);

      expect(result).toBe(true);
      expect(mockProjectIdManager.generateProjectId).toHaveBeenCalledWith(projectPath);
      expect(mockCollectionManager.createCollection).toHaveBeenCalledWith(collectionName, vectorSize, distance);
    });

    it('should return false if collection creation fails', async () => {
      const projectPath = '/test/project';
      const vectorSize = 128;
      const distance: VectorDistance = 'Cosine';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';

      mockProjectIdManager.generateProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockCollectionManager.createCollection.mockResolvedValue(false);

      const result = await projectManager.createCollectionForProject(projectPath, vectorSize, distance);

      expect(result).toBe(false);
    });
  });

  describe('upsertVectorsForProject', () => {
    it('should upsert vectors for a project', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';
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

      mockProjectIdManager.getProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockVectorOperations.upsertVectors.mockResolvedValue(true);

      const result = await projectManager.upsertVectorsForProject(projectPath, vectors);

      expect(result).toBe(true);
      expect(mockVectorOperations.upsertVectors).toHaveBeenCalledWith(
        collectionName,
        expect.arrayContaining([
          expect.objectContaining({
            payload: expect.objectContaining({
              projectId
            })
          })
        ])
      );
    });

    it('should return false if upserting vectors fails', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';
      const vectors: VectorPoint[] = [];

      mockProjectIdManager.getProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockVectorOperations.upsertVectors.mockResolvedValue(false);

      const result = await projectManager.upsertVectorsForProject(projectPath, vectors);

      expect(result).toBe(false);
    });
  });

  describe('searchVectorsForProject', () => {
    it('should search vectors for a project', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';
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
            projectId
          }
        }
      ];

      mockProjectIdManager.getProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockVectorOperations.searchVectors.mockResolvedValue(searchResults);

      const result = await projectManager.searchVectorsForProject(projectPath, query, options);

      expect(result).toEqual(searchResults);
      expect(mockVectorOperations.searchVectors).toHaveBeenCalledWith(
        collectionName,
        query,
        expect.objectContaining({
          filter: expect.objectContaining({
            projectId
          })
        })
      );
    });

    it('should return empty array if searching vectors fails', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';
      const query = [0.1, 0.2, 0.3];

      mockProjectIdManager.getProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockVectorOperations.searchVectors.mockRejectedValue(new Error('Search failed'));

      const result = await projectManager.searchVectorsForProject(projectPath, query);

      expect(result).toEqual([]);
    });
  });

  describe('getCollectionInfoForProject', () => {
    it('should get collection info for a project', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';
      const collectionInfo: CollectionInfo = {
        name: collectionName,
        vectors: {
          size: 128,
          distance: 'Cosine'
        },
        pointsCount: 100,
        status: 'green'
      };

      mockProjectIdManager.getProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockCollectionManager.getCollectionInfo.mockResolvedValue(collectionInfo);

      const result = await projectManager.getCollectionInfoForProject(projectPath);

      expect(result).toEqual(collectionInfo);
      expect(mockCollectionManager.getCollectionInfo).toHaveBeenCalledWith(collectionName);
    });

    it('should return null if getting collection info fails', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';

      mockProjectIdManager.getProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockCollectionManager.getCollectionInfo.mockRejectedValue(new Error('Failed to get info'));

      const result = await projectManager.getCollectionInfoForProject(projectPath);

      expect(result).toBeNull();
    });
  });

  describe('deleteCollectionForProject', () => {
    it('should delete collection for a project', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';

      mockProjectIdManager.getProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockCollectionManager.deleteCollection.mockResolvedValue(true);

      const result = await projectManager.deleteCollectionForProject(projectPath);

      expect(result).toBe(true);
      expect(mockCollectionManager.deleteCollection).toHaveBeenCalledWith(collectionName);
      expect(mockProjectIdManager.removeProject).toHaveBeenCalledWith(projectPath);
    });

    it('should return false if deleting collection fails', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const collectionName = 'project-test-project-id';

      mockProjectIdManager.getProjectId.mockResolvedValue(projectId);
      mockProjectIdManager.getCollectionName.mockReturnValue(collectionName);
      mockCollectionManager.deleteCollection.mockResolvedValue(false);

      const result = await projectManager.deleteCollectionForProject(projectPath);

      expect(result).toBe(false);
    });
  });
});