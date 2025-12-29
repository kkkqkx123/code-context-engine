import { Container } from 'inversify';
import { TYPES } from '../../../../types';
import { NebulaSpaceManager, INebulaSpaceManager } from '../NebulaSpaceManager';
import { DatabaseLoggerService } from '../../../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../../../utils/ErrorHandlerService';
import { ConfigService } from '../../../../config/ConfigService';
import { NebulaSpaceInfo, NebulaSpaceConfig } from '../../NebulaTypes';
import { INebulaConnectionManager } from '../../NebulaConnectionManager';
import { INebulaQueryBuilder } from '../../query/NebulaQueryBuilder';
import { INebulaSchemaManager } from '../../NebulaSchemaManager';
import { ISpaceNameUtils } from '../../SpaceNameUtils';
import { DatabaseEventType } from '../../../common/DatabaseEventTypes';
import { LoggerService } from '../../../../utils/LoggerService';
import { CacheService } from '../../../../infrastructure/caching/CacheService';
import { PerformanceMonitor } from '../../../../infrastructure/monitoring/PerformanceMonitor';

// Mock services to avoid real instances with timers and event listeners
jest.mock('../../../../utils/LoggerService');
jest.mock('../../../../utils/ErrorHandlerService');
jest.mock('../../../common/DatabaseLoggerService');

describe('NebulaSpaceManager', () => {
  let container: Container;
  let spaceManager: INebulaSpaceManager;
  let mockNebulaConnection: jest.Mocked<INebulaConnectionManager>;
  let mockNebulaQueryBuilder: jest.Mocked<INebulaQueryBuilder>;
  let mockSchemaManager: jest.Mocked<INebulaSchemaManager>;
  let mockSpaceNameUtils: jest.Mocked<ISpaceNameUtils>;
  let mockDatabaseLogger: jest.Mocked<DatabaseLoggerService>;
  let mockErrorHandler: jest.Mocked<ErrorHandlerService>;
  let mockConfigService: jest.Mocked<ConfigService>;
  let mockCacheService: jest.Mocked<CacheService>;
  let mockPerformanceMonitor: jest.Mocked<PerformanceMonitor>;

  beforeEach(() => {
    container = new Container();

    // Create mock instances for services
    mockNebulaConnection = {
      executeQuery: jest.fn(),
      isConnected: jest.fn(),
      connect: jest.fn(),
      disconnect: jest.fn(),
      executeTransaction: jest.fn(),
      getConnectionStatus: jest.fn(),
      getConfig: jest.fn(),
      updateConfig: jest.fn(),
      subscribe: jest.fn()
    } as any;

    mockNebulaQueryBuilder = {
      insertVertex: jest.fn(),
      batchInsertVertices: jest.fn(),
      insertEdge: jest.fn(),
      batchInsertEdges: jest.fn(),
      go: jest.fn(),
      buildComplexTraversal: jest.fn(),
      buildShortestPath: jest.fn(),
      updateVertex: jest.fn(),
      updateEdge: jest.fn(),
      deleteVertices: jest.fn(),
      deleteEdges: jest.fn(),
      buildNodeCountQuery: jest.fn(),
      buildRelationshipCountQuery: jest.fn()
    } as any;

    mockSchemaManager = {
      createGraphSchema: jest.fn(),
      createTag: jest.fn(),
      createEdgeType: jest.fn(),
      tagExists: jest.fn(),
      edgeTypeExists: jest.fn(),
      getAllTags: jest.fn(),
      getAllEdgeTypes: jest.fn()
    } as any;

    mockSpaceNameUtils = {
      generateSpaceName: jest.fn(),
      validateSpaceName: jest.fn(),
      generateSpaceNameFromPath: jest.fn(),
      sanitizeName: jest.fn()
    } as any;

    mockDatabaseLogger = {
      logDatabaseEvent: jest.fn().mockResolvedValue(undefined),
      logConnectionEvent: jest.fn().mockResolvedValue(undefined),
      logQueryPerformance: jest.fn().mockResolvedValue(undefined),
      logBatchOperation: jest.fn().mockResolvedValue(undefined),
      logCollectionOperation: jest.fn().mockResolvedValue(undefined),
      logVectorOperation: jest.fn().mockResolvedValue(undefined),
      logQueryOperation: jest.fn().mockResolvedValue(undefined),
      logProjectOperation: jest.fn().mockResolvedValue(undefined),
      updateLogLevel: jest.fn()
    } as any;

    mockErrorHandler = {
      handleError: jest.fn(),
      getErrorReport: jest.fn(),
      getAllErrorReports: jest.fn(),
      clearErrorReport: jest.fn(),
      clearAllErrorReports: jest.fn(),
      getErrorStats: jest.fn()
    } as any;

    mockConfigService = {
      get: jest.fn(),
      getAll: jest.fn(),
      getQdrantConfig: jest.fn(),
      getEnvironmentConfig: jest.fn(),
      getEmbeddingConfig: jest.fn()
    } as any;

    mockCacheService = {
      getDatabaseSpecificCache: jest.fn(),
      setDatabaseSpecificCache: jest.fn(),
      invalidateDatabaseCache: jest.fn(),
      get: jest.fn(),
      set: jest.fn(),
      invalidate: jest.fn(),
      clear: jest.fn()
    } as any;

    mockPerformanceMonitor = {
      startOperation: jest.fn().mockReturnValue('op-123'),
      endOperation: jest.fn(),
      recordCacheHit: jest.fn(),
      recordCacheMiss: jest.fn(),
      getMetrics: jest.fn()
    } as any;

    // Bind mocks to container
    container.bind<INebulaConnectionManager>(TYPES.INebulaConnectionManager).toConstantValue(mockNebulaConnection);
    container.bind<INebulaQueryBuilder>(TYPES.INebulaQueryBuilder).toConstantValue(mockNebulaQueryBuilder);
    container.bind<INebulaSchemaManager>(TYPES.INebulaSchemaManager).toConstantValue(mockSchemaManager);
    container.bind<ISpaceNameUtils>(TYPES.ISpaceNameUtils).toConstantValue(mockSpaceNameUtils);
    container.bind<DatabaseLoggerService>(TYPES.DatabaseLoggerService).toConstantValue(mockDatabaseLogger);
    container.bind<ErrorHandlerService>(TYPES.ErrorHandlerService).toConstantValue(mockErrorHandler);
    container.bind<ConfigService>(TYPES.ConfigService).toConstantValue(mockConfigService);

    // Create instance
    spaceManager = new NebulaSpaceManager(
      mockNebulaConnection,
      mockNebulaQueryBuilder,
      mockSchemaManager,
      mockSpaceNameUtils,
      mockDatabaseLogger,
      mockErrorHandler,
      mockConfigService,
      mockCacheService,
      mockPerformanceMonitor
    );

    // Clear mocks
    jest.clearAllMocks();
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('createSpace', () => {
    test('should create space successfully with default config', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery.mockResolvedValue({} as any);
      mockSchemaManager.createGraphSchema.mockResolvedValue(true);

      // Mock waitForSpaceReady by making DESCRIBE SPACE succeed immediately
      mockNebulaConnection.executeQuery
        .mockResolvedValueOnce({} as any) // CREATE SPACE
        .mockResolvedValueOnce({ data: [{ name: spaceName }] } as any); // DESCRIBE SPACE

      const result = await spaceManager.createSpace(projectId);

      expect(result).toBe(true);
      expect(mockSpaceNameUtils.generateSpaceName).toHaveBeenCalledWith(projectId);
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith(
        expect.stringContaining(`CREATE SPACE IF NOT EXISTS \`${spaceName}\``)
      );
      expect(mockSchemaManager.createGraphSchema).toHaveBeenCalledWith(projectId, {});
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.SPACE_CREATED,
          data: expect.objectContaining({
            message: expect.stringContaining('Successfully created space')
          })
        })
      );
    });

    test('should create space with custom config', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';
      const config: NebulaSpaceConfig = {
        partitionNum: 20,
        replicaFactor: 2,
        vidType: 'FIXED_STRING(64)'
      };

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery.mockResolvedValue({} as any);
      mockSchemaManager.createGraphSchema.mockResolvedValue(true);

      // Mock waitForSpaceReady by making DESCRIBE SPACE succeed immediately
      mockNebulaConnection.executeQuery
        .mockResolvedValueOnce({} as any) // CREATE SPACE
        .mockResolvedValueOnce({ data: [{ name: spaceName }] } as any); // DESCRIBE SPACE

      const result = await spaceManager.createSpace(projectId, config);

      expect(result).toBe(true);
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith(
        expect.stringContaining('partition_num = 20')
      );
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith(
        expect.stringContaining('replica_factor = 2')
      );
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith(
        expect.stringContaining('vid_type = FIXED_STRING(64)')
      );
    });

    test('should handle space creation failure', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery.mockRejectedValue(new Error('Connection failed'));

      const result = await spaceManager.createSpace(projectId);

      expect(result).toBe(false);
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.ERROR_OCCURRED,
          data: expect.objectContaining({
            message: expect.stringContaining('Failed to create space')
          })
        })
      );
    });

    test('should return false for invalid space name', async () => {
      const projectId = '';

      const result = await spaceManager.createSpace(projectId);

      expect(result).toBe(false);
      expect(mockNebulaConnection.executeQuery).not.toHaveBeenCalled();
    });

    test('should use existing space name if it starts with project_', async () => {
      const projectId = 'project_existing';
      const config: NebulaSpaceConfig = {};

      // Setup mocks
      mockNebulaConnection.executeQuery.mockResolvedValue({} as any);
      mockSchemaManager.createGraphSchema.mockResolvedValue(true);

      // Mock waitForSpaceReady by making DESCRIBE SPACE succeed immediately
      mockNebulaConnection.executeQuery
        .mockResolvedValueOnce({} as any) // CREATE SPACE
        .mockResolvedValueOnce({ data: [{ name: projectId }] } as any); // DESCRIBE SPACE

      const result = await spaceManager.createSpace(projectId, config);

      expect(result).toBe(true);
      expect(mockSpaceNameUtils.generateSpaceName).not.toHaveBeenCalled();
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith(
        expect.stringContaining(`CREATE SPACE IF NOT EXISTS \`${projectId}\``)
      );
    });
  });

  describe('deleteSpace', () => {
    test('should delete space successfully', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery.mockResolvedValue({} as any);

      const result = await spaceManager.deleteSpace(projectId);

      expect(result).toBe(true);
      expect(mockSpaceNameUtils.generateSpaceName).toHaveBeenCalledWith(projectId);
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith(
        expect.stringContaining(`DROP SPACE IF EXISTS \`${spaceName}\``)
      );
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.SPACE_DELETED,
          data: expect.objectContaining({
            message: expect.stringContaining('Successfully deleted space')
          })
        })
      );
    });

    test('should handle space deletion failure', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery.mockRejectedValue(new Error('Connection failed'));

      const result = await spaceManager.deleteSpace(projectId);

      expect(result).toBe(false);
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.ERROR_OCCURRED,
          data: expect.objectContaining({
            message: expect.stringContaining('Failed to delete space')
          })
        })
      );
    });

    test('should return false for invalid space name', async () => {
      const projectId = '';

      const result = await spaceManager.deleteSpace(projectId);

      expect(result).toBe(false);
      expect(mockNebulaConnection.executeQuery).not.toHaveBeenCalled();
    });
  });

  describe('listSpaces', () => {
    test('should list spaces successfully', async () => {
      const mockSpaces = [
        { Name: 'space1' },
        { Name: 'space2' },
        { name: 'space3' }
      ];

      // Setup mocks
      mockNebulaConnection.executeQuery.mockResolvedValue({ data: mockSpaces } as any);

      const result = await spaceManager.listSpaces();

      expect(result).toEqual(['space1', 'space2', 'space3']);
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith('SHOW SPACES');
    });

    test('should handle empty spaces result', async () => {
      // Setup mocks
      mockNebulaConnection.executeQuery.mockResolvedValue({ data: [] } as any);

      const result = await spaceManager.listSpaces();

      expect(result).toEqual([]);
    });

    test('should handle null result from SHOW SPACES', async () => {
      // Setup mocks
      mockNebulaConnection.executeQuery.mockResolvedValue(null as any);

      const result = await spaceManager.listSpaces();

      expect(result).toEqual([]);
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.WARNING,
          data: expect.objectContaining({
            message: 'SHOW SPACES returned null result'
          })
        })
      );
    });

    test('should handle non-array data from SHOW SPACES', async () => {
      // Setup mocks
      mockNebulaConnection.executeQuery.mockResolvedValue({ data: 'invalid' } as any);

      const result = await spaceManager.listSpaces();

      expect(result).toEqual([]);
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.WARNING,
          data: expect.objectContaining({
            message: expect.stringContaining('SHOW SPACES returned non-array data')
          })
        })
      );
    });
  });

  describe('getSpaceInfo', () => {
    test('should get space info successfully', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';
      const mockSpaceInfo = {
        partition_num: 10,
        replica_factor: 1,
        vid_type: 'FIXED_STRING(32)',
        charset: 'utf8',
        collate: 'utf8_bin'
      };

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery.mockResolvedValue({
        data: [mockSpaceInfo]
      } as any);

      const result = await spaceManager.getSpaceInfo(projectId);

      expect(result).toEqual({
        name: spaceName,
        ...mockSpaceInfo
      });
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith(
        expect.stringContaining(`DESCRIBE SPACE \`${spaceName}\``)
      );
    });

    test('should return null when space info is not found', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery.mockResolvedValue({ data: [] } as any);

      const result = await spaceManager.getSpaceInfo(projectId);

      expect(result).toBeNull();
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.DEBUG,
          data: expect.objectContaining({
            message: expect.stringContaining('No space info found')
          })
        })
      );
    });

    test('should handle DESCRIBE SPACE failure', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery.mockRejectedValue(new Error('Connection failed'));

      const result = await spaceManager.getSpaceInfo(projectId);

      expect(result).toBeNull();
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.ERROR_OCCURRED,
          data: expect.objectContaining({
            message: expect.stringContaining('Failed to get space info')
          })
        })
      );
    });

    test('should return null for invalid space name', async () => {
      const projectId = '';

      const result = await spaceManager.getSpaceInfo(projectId);

      expect(result).toBeNull();
      expect(mockNebulaConnection.executeQuery).not.toHaveBeenCalled();
    });
  });

  describe('checkSpaceExists', () => {
    test('should return true when space exists', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      spaceManager.listSpaces = jest.fn().mockResolvedValue([spaceName, 'other-space']);

      const result = await spaceManager.checkSpaceExists(projectId);

      expect(result).toBe(true);
    });

    test('should return false when space does not exist', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      spaceManager.listSpaces = jest.fn().mockResolvedValue(['other-space1', 'other-space2']);

      const result = await spaceManager.checkSpaceExists(projectId);

      expect(result).toBe(false);
    });

    test('should handle listSpaces failure', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      spaceManager.listSpaces = jest.fn().mockResolvedValue([]);

      const result = await spaceManager.checkSpaceExists(projectId);

      expect(result).toBe(false);
    });

    test('should return false for invalid space name', async () => {
      const projectId = '';

      const result = await spaceManager.checkSpaceExists(projectId);

      expect(result).toBe(false);
    });
  });

  describe('clearSpace', () => {
    test('should clear space successfully', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';
      const mockTags = [{ Name: 'tag1' }, { Name: 'tag2' }];
      const mockEdges = [{ Name: 'edge1' }, { Name: 'edge2' }];

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery
        .mockResolvedValueOnce({} as any) // USE
        .mockResolvedValueOnce({ data: mockTags } as any) // SHOW TAGS
        .mockResolvedValueOnce({ data: mockEdges } as any) // SHOW EDGES
        .mockResolvedValueOnce({} as any) // DELETE EDGE
        .mockResolvedValueOnce({} as any) // DELETE EDGE
        .mockResolvedValueOnce({} as any); // DELETE VERTEX

      const result = await spaceManager.clearSpace(projectId);

      expect(result).toBe(true);
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith(
        expect.stringContaining(`USE \`${spaceName}\``)
      );
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith('SHOW TAGS');
      expect(mockNebulaConnection.executeQuery).toHaveBeenCalledWith('SHOW EDGES');
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.SPACE_CLEARED,
          data: expect.objectContaining({
            message: expect.stringContaining('Successfully cleared space')
          })
        })
      );
    });

    test('should handle clear space failure', async () => {
      const projectId = 'test-project';
      const spaceName = 'project_test-project';

      // Setup mocks
      mockSpaceNameUtils.generateSpaceName.mockReturnValue(spaceName);
      mockNebulaConnection.executeQuery.mockRejectedValue(new Error('Connection failed'));

      const result = await spaceManager.clearSpace(projectId);

      expect(result).toBe(false);
      expect(mockDatabaseLogger.logDatabaseEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: DatabaseEventType.SPACE_ERROR,
          data: expect.objectContaining({
            message: expect.stringContaining('Failed to clear space')
          })
        })
      );
    });

    test('should return false for invalid space name', async () => {
      const projectId = '';

      const result = await spaceManager.clearSpace(projectId);

      expect(result).toBe(false);
      expect(mockNebulaConnection.executeQuery).not.toHaveBeenCalled();
    });
  });
});