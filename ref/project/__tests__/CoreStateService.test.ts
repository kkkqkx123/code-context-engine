import { CoreStateService } from '../services/CoreStateService';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { ProjectIdManager } from '../../../database/ProjectIdManager';
import { QdrantService } from '../../../database/qdrant/QdrantService';
import { NebulaClient } from '../../../database/nebula/client/NebulaClient';
import { VectorIndexService } from '../../index/VectorIndexService';
import { ProjectState, ProjectStats, StorageStatus } from '../ProjectStateManager';
import { ProjectStateStorageUtils } from '../utils/ProjectStateStorageUtils';
import { ProjectStateValidator } from '../utils/ProjectStateValidator';
import { HotReloadConfigService } from '../../filesystem/HotReloadConfigService';

// Mock dependencies
jest.mock('../../../utils/LoggerService');
jest.mock('../../../utils/ErrorHandlerService');
jest.mock('../../../database/ProjectIdManager');
jest.mock('../../../database/qdrant/QdrantService');
jest.mock('../../../database/nebula/NebulaClient');
jest.mock('../../index/IndexService');
jest.mock('../utils/ProjectStateStorageUtils');
jest.mock('../utils/ProjectStateValidator');
jest.mock('../../filesystem/HotReloadConfigService');

describe('CoreStateService', () => {
  let coreStateService: CoreStateService;
  let loggerService: jest.Mocked<LoggerService>;
  let errorHandlerService: jest.Mocked<ErrorHandlerService>;
  let projectIdManager: jest.Mocked<ProjectIdManager>;
  let qdrantService: jest.Mocked<QdrantService>;
  let nebulaService: jest.Mocked<NebulaClient>;
  let indexService: jest.Mocked<VectorIndexService>;
  let hotReloadConfigService: jest.Mocked<HotReloadConfigService>;

  beforeEach(() => {
    // Reset all mocks
    jest.clearAllMocks();

    // Create mock instances
    loggerService = new LoggerService() as jest.Mocked<LoggerService>;
    errorHandlerService = new ErrorHandlerService(loggerService) as jest.Mocked<ErrorHandlerService>;
    projectIdManager = new ProjectIdManager(
      {} as any,
      {} as any,
      {} as any,
      loggerService,
      errorHandlerService,
      {} as any // Mock SqliteProjectManager
    ) as jest.Mocked<ProjectIdManager>;
    qdrantService = {
      getCollectionInfoForProject: jest.fn(),
      deleteCollectionForProject: jest.fn()
    } as unknown as jest.Mocked<QdrantService>;
    nebulaService = {
      deleteSpaceForProject: jest.fn()
    } as unknown as jest.Mocked<NebulaClient>;
    indexService = {
      getIndexStatus: jest.fn()
    } as unknown as jest.Mocked<VectorIndexService>;

    hotReloadConfigService = {
      getGlobalConfig: jest.fn().mockReturnValue({
        enabled: true,
        defaultDebounceInterval: 500,
        defaultWatchPatterns: ['**/*.{js,ts,jsx,tsx,json,md,py,go,java,css,html,scss}'],
        defaultIgnorePatterns: [
          '**/node_modules/**',
          '**/.git/**',
          '**/dist/**',
          '**/build/**',
          '**/target/**',
          '**/venv/**',
          '**/.vscode/**',
          '**/.idea/**',
          '**/logs/**',
          '**/*.log',
          '**/coverage/**',
          '**/tmp/**',
          '**/temp/**'
        ],
        defaultMaxFileSize: 10 * 1024 * 1024,
        defaultErrorHandling: {
          maxRetries: 3,
          alertThreshold: 5,
          autoRecovery: true
        },
        enableDetailedLogging: false,
        maxConcurrentProjects: 5
      })
    } as unknown as jest.Mocked<HotReloadConfigService>;

    // Create service instance
    coreStateService = new CoreStateService(
      loggerService,
      errorHandlerService,
      projectIdManager,
      qdrantService,
      nebulaService,
      hotReloadConfigService
    );
  });

  // Helper function to create a mock project state
  const createMockProjectState = (overrides: Partial<ProjectState> = {}): ProjectState => {
    return {
      projectId: overrides.projectId || 'test-project-id',
      projectPath: overrides.projectPath || '/test/project',
      name: overrides.name || 'Test Project',
      description: overrides.description || 'A test project',
      status: overrides.status || 'inactive',
      vectorStatus: overrides.vectorStatus || {
        status: 'pending',
        progress: 0,
        lastUpdated: new Date()
      },
      graphStatus: overrides.graphStatus || {
        status: 'pending',
        progress: 0,
        lastUpdated: new Date()
      },
      createdAt: overrides.createdAt || new Date(),
      updatedAt: overrides.updatedAt || new Date(),
      lastIndexedAt: overrides.lastIndexedAt,
      indexingProgress: overrides.indexingProgress,
      totalFiles: overrides.totalFiles,
      indexedFiles: overrides.indexedFiles,
      failedFiles: overrides.failedFiles,
      collectionInfo: overrides.collectionInfo,
      hotReload: overrides.hotReload || {
        enabled: false,
        config: {
          debounceInterval: 500,
          watchPatterns: ['**/*.ts', '**/*.js', '**/*.tsx', '**/*.jsx'],
          ignorePatterns: ['node_modules/**', 'dist/**', 'build/**']
        }
      },
      settings: {
        autoIndex: overrides.settings?.autoIndex ?? true,
        watchChanges: overrides.settings?.watchChanges ?? true,
        ...overrides.settings
      },
      metadata: overrides.metadata || {}
    };
  };

  describe('createOrUpdateProjectState', () => {
    it('should create a new project state', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const options = {
        name: 'Test Project',
        description: 'A test project',
        settings: { autoIndex: false }
      };

      // Mock dependencies
      projectIdManager.generateProjectId.mockResolvedValue(projectId);
      projectIdManager.getProjectId.mockReturnValue(undefined); // No existing project
      qdrantService.getCollectionInfoForProject.mockResolvedValue({
        name: 'project-test-project-id',
        vectors: {
          size: 1536,
          distance: 'Cosine'
        },
        pointsCount: 10,
        status: 'green'
      });
      (ProjectStateValidator.initializeStorageStatus as jest.Mock).mockReturnValue({
        status: 'pending',
        progress: 0,
        lastUpdated: new Date()
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = await coreStateService.createOrUpdateProjectState(
        projectStates,
        projectPath,
        storagePath,
        options
      );

      // Verify results
      expect(result.projectId).toBe(projectId);
      expect(result.projectPath).toBe(projectPath);
      expect(result.name).toBe('Test Project');
      expect(result.description).toBe('A test project');
      expect(result.settings.autoIndex).toBe(false);
      expect(projectStates.has(projectId)).toBe(true);
      expect(loggerService.info).toHaveBeenCalledWith(
        `Project state updated for ${projectId}`,
        expect.any(Object)
      );
    });

    it('should update an existing project state', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const options = {
        name: 'Updated Project',
        description: 'Updated description',
        settings: { autoIndex: false }
      };

      // Create initial project state
      const initialState = createMockProjectState({
        projectId,
        projectPath,
        name: 'Test Project',
        description: 'A test project',
        settings: { autoIndex: true, watchChanges: true }
      });

      // Mock dependencies
      projectIdManager.generateProjectId.mockResolvedValue(projectId);
      projectIdManager.getProjectId.mockReturnValue(projectId); // Existing project
      qdrantService.getCollectionInfoForProject.mockResolvedValue({
        name: 'project-test-project-id',
        vectors: {
          size: 1536,
          distance: 'Cosine'
        },
        pointsCount: 10,
        status: 'green'
      });

      // Create project states map with initial state
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, initialState);

      // Call the method
      const result = await coreStateService.createOrUpdateProjectState(
        projectStates,
        projectPath,
        storagePath,
        options
      );

      // Verify results
      expect(result.projectId).toBe(projectId);
      expect(result.name).toBe('Updated Project');
      expect(result.description).toBe('Updated description');
      expect(result.settings.autoIndex).toBe(false);
      expect(result.settings.watchChanges).toBe(true); // Should remain unchanged
      expect(loggerService.info).toHaveBeenCalledWith(
        `Project state updated for ${projectId}`,
        expect.any(Object)
      );
    });

    it('should handle reindexing with allowReindex option', async () => {
      const projectPath = '/test/project';
      const oldProjectId = 'old-project-id';
      const newProjectId = 'new-project-id';
      const storagePath = './data/project-states.json';
      const options = {
        name: 'Reindexed Project',
        allowReindex: true
      };

      // Create initial project state with different ID
      const initialState = createMockProjectState({
        projectId: oldProjectId,
        projectPath,
        name: 'Old Project'
      });

      // Mock dependencies
      projectIdManager.generateProjectId.mockResolvedValue(newProjectId);
      projectIdManager.getProjectId.mockReturnValue(oldProjectId); // Existing project with different ID
      qdrantService.deleteCollectionForProject.mockResolvedValue(true as any);
      nebulaService.deleteSpaceForProject.mockResolvedValue(true as any);
      qdrantService.getCollectionInfoForProject.mockResolvedValue({
        name: 'project-new-project-id',
        vectors: {
          size: 1536,
          distance: 'Cosine'
        },
        pointsCount: 10,
        status: 'green'
      });
      (ProjectStateValidator.initializeStorageStatus as jest.Mock).mockReturnValue({
        status: 'pending',
        progress: 0,
        lastUpdated: new Date()
      });

      // Create project states map with initial state
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(oldProjectId, initialState);

      // Call the method
      const result = await coreStateService.createOrUpdateProjectState(
        projectStates,
        projectPath,
        storagePath,
        options
      );

      // Verify results
      expect(result.projectId).toBe(newProjectId);
      expect(result.name).toBe('Reindexed Project');
      expect(projectStates.has(oldProjectId)).toBe(false); // Old project should be removed
      expect(projectStates.has(newProjectId)).toBe(true); // New project should be added
      expect(qdrantService.deleteCollectionForProject).toHaveBeenCalledWith(projectPath);
      expect(nebulaService.deleteSpaceForProject).toHaveBeenCalledWith(projectPath);
      expect(loggerService.info).toHaveBeenCalledWith(
        `重新索引项目，清理旧项目状态和资源: ${oldProjectId}`,
        expect.any(Object)
      );
    });

    it('should prevent duplicate project paths when allowReindex is false', async () => {
      const projectPath = '/test/project';
      const existingProjectId = 'existing-project-id';
      const newProjectId = 'new-project-id';
      const storagePath = './data/project-states.json';
      const options = {
        name: 'New Project',
        allowReindex: false
      };

      // Create initial project state
      const initialState = createMockProjectState({
        projectId: existingProjectId,
        projectPath,
        name: 'Existing Project'
      });

      // Mock dependencies
      projectIdManager.generateProjectId.mockResolvedValue(newProjectId);
      projectIdManager.getProjectId.mockReturnValue(existingProjectId); // Existing project with different ID

      // Create project states map with initial state
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(existingProjectId, initialState);

      // Call the method and expect error
      await expect(coreStateService.createOrUpdateProjectState(
        projectStates,
        projectPath,
        storagePath,
        options
      )).rejects.toThrow(`项目路径 "${projectPath}" 已被项目 "${existingProjectId}" 使用，不能重复添加`);
    });

    it('should handle errors during project state creation', async () => {
      const projectPath = '/test/project';
      const storagePath = './data/project-states.json';
      const error = new Error('Generation failed');

      // Mock dependencies
      projectIdManager.generateProjectId.mockRejectedValue(error);

      // Create project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method and expect error
      await expect(coreStateService.createOrUpdateProjectState(
        projectStates,
        projectPath,
        storagePath
      )).rejects.toThrow('Generation failed');

      // Verify error was handled
      expect(errorHandlerService.handleError).toHaveBeenCalledWith(
        new Error(`Failed to create or update project state: ${error.message}`),
        { component: 'CoreStateService', operation: 'createOrUpdateProjectState', projectPath, options: {} }
      );
    });

    it('should set graph status to disabled when NEBULA_ENABLED is false', async () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';

      // Mock NEBULA_ENABLED environment variable
      const originalNebulaEnabled = process.env.NEBULA_ENABLED;
      process.env.NEBULA_ENABLED = 'false';

      // Mock dependencies
      projectIdManager.generateProjectId.mockResolvedValue(projectId);
      projectIdManager.getProjectId.mockReturnValue(undefined); // No existing project
      qdrantService.getCollectionInfoForProject.mockResolvedValue({
        name: 'project-test-project-id',
        vectors: {
          size: 1536,
          distance: 'Cosine'
        },
        pointsCount: 10,
        status: 'green'
      });
      (ProjectStateValidator.initializeStorageStatus as jest.Mock).mockReturnValue({
        status: 'pending',
        progress: 0,
        lastUpdated: new Date()
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = await coreStateService.createOrUpdateProjectState(
        projectStates,
        projectPath,
        storagePath
      );

      // Verify results
      expect(result.graphStatus.status).toBe('disabled');

      // Restore original environment variable
      process.env.NEBULA_ENABLED = originalNebulaEnabled;
    });
  });

  describe('getProjectState', () => {
    it('should return project state for existing project', () => {
      const projectId = 'test-project-id';
      const projectState = createMockProjectState({ projectId });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      const result = coreStateService.getProjectState(projectStates, projectId);

      // Verify results
      expect(result).toBe(projectState);
    });

    it('should return null for non-existent project', () => {
      const projectId = 'non-existent-project-id';

      // Create project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = coreStateService.getProjectState(projectStates, projectId);

      // Verify results
      expect(result).toBeNull();
    });
  });

  describe('getAllProjectStates', () => {
    it('should return all project states', () => {
      const projectState1 = createMockProjectState({ projectId: 'project-1' });
      const projectState2 = createMockProjectState({ projectId: 'project-2' });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set('project-1', projectState1);
      projectStates.set('project-2', projectState2);

      // Call the method
      const result = coreStateService.getAllProjectStates(projectStates);

      // Verify results
      expect(result).toHaveLength(2);
      expect(result).toContain(projectState1);
      expect(result).toContain(projectState2);
    });

    it('should return empty array when no projects exist', () => {
      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = coreStateService.getAllProjectStates(projectStates);

      // Verify results
      expect(result).toHaveLength(0);
    });
  });

  describe('getProjectStateByPath', () => {
    it('should return project state for existing path', () => {
      const projectPath = '/test/project';
      const projectId = 'test-project-id';
      const projectState = createMockProjectState({ projectId, projectPath });

      // Mock dependencies
      projectIdManager.getProjectId.mockReturnValue(projectId);

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      const result = coreStateService.getProjectStateByPath(projectStates, projectPath);

      // Verify results
      expect(result).toBe(projectState);
    });

    it('should return null for non-existent path', () => {
      const projectPath = '/non-existent/project';

      // Mock dependencies
      projectIdManager.getProjectId.mockReturnValue(undefined);

      // Create project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = coreStateService.getProjectStateByPath(projectStates, projectPath);

      // Verify results
      expect(result).toBeNull();
    });
  });

  describe('deleteProjectState', () => {
    it('should delete project state successfully', async () => {
      const projectId = 'test-project-id';
      const projectPath = '/test/project';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({ projectId, projectPath });

      // Mock dependencies
      qdrantService.deleteCollectionForProject.mockResolvedValue(true as any);
      nebulaService.deleteSpaceForProject.mockResolvedValue(true as any);

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      const result = await coreStateService.deleteProjectState(
        projectStates,
        projectId,
        storagePath
      );

      // Verify results
      expect(result).toBe(true);
      expect(projectStates.has(projectId)).toBe(false);
      expect(qdrantService.deleteCollectionForProject).toHaveBeenCalledWith(projectPath);
      expect(nebulaService.deleteSpaceForProject).toHaveBeenCalledWith(projectPath);
      expect(loggerService.info).toHaveBeenCalledWith(`Deleted project state for ${projectId}`);
    });

    it('should return false for non-existent project', async () => {
      const projectId = 'non-existent-project-id';
      const storagePath = './data/project-states.json';

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = await coreStateService.deleteProjectState(
        projectStates,
        projectId,
        storagePath
      );

      // Verify results
      expect(result).toBe(false);
    });

    it('should handle errors during deletion', async () => {
      const projectId = 'test-project-id';
      const projectPath = '/test/project';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({ projectId, projectPath });
      const error = new Error('Deletion failed');

      // Mock dependencies
      qdrantService.deleteCollectionForProject.mockRejectedValue(error);
      nebulaService.deleteSpaceForProject.mockRejectedValue(error);

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      const result = await coreStateService.deleteProjectState(
        projectStates,
        projectId,
        storagePath
      );

      // Verify results
      expect(result).toBe(true);
      expect(projectStates.has(projectId)).toBe(false);
      expect(loggerService.warn).toHaveBeenCalledWith(
        `Failed to delete Qdrant collection for project ${projectId}`,
        { error }
      );
      expect(loggerService.warn).toHaveBeenCalledWith(
        `Failed to delete Nebula space for project ${projectId}`,
        { error }
      );
    });

    it('should continue with deletion even if Qdrant deletion fails', async () => {
      const projectId = 'test-project-id';
      const projectPath = '/test/project';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({ projectId, projectPath });

      // Mock dependencies
      qdrantService.deleteCollectionForProject.mockRejectedValue(new Error('Qdrant deletion failed'));
      nebulaService.deleteSpaceForProject.mockResolvedValue(true as any);

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      const result = await coreStateService.deleteProjectState(
        projectStates,
        projectId,
        storagePath
      );

      // Verify results
      expect(result).toBe(true);
      expect(projectStates.has(projectId)).toBe(false);
      expect(loggerService.warn).toHaveBeenCalledWith(
        `Failed to delete Qdrant collection for project ${projectId}`,
        { error: expect.any(Error) }
      );
    });
  });

  describe('getProjectStats', () => {
    it('should return correct project statistics', () => {
      const projectState1 = createMockProjectState({
        projectId: 'project-1',
        status: 'active',
        collectionInfo: { name: 'collection-1', vectorsCount: 100, status: 'green' },
        totalFiles: 50,
        indexingProgress: 75
      });
      const projectState2 = createMockProjectState({
        projectId: 'project-2',
        status: 'indexing',
        collectionInfo: { name: 'collection-2', vectorsCount: 200, status: 'green' },
        totalFiles: 100,
        indexingProgress: 50
      });
      const projectState3 = createMockProjectState({
        projectId: 'project-3',
        status: 'inactive',
        collectionInfo: { name: 'collection-3', vectorsCount: 300, status: 'green' },
        totalFiles: 150,
        indexingProgress: 25
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set('project-1', projectState1);
      projectStates.set('project-2', projectState2);
      projectStates.set('project-3', projectState3);

      // Call the method
      const result = coreStateService.getProjectStats(projectStates);

      // Verify results
      expect(result.totalProjects).toBe(3);
      expect(result.activeProjects).toBe(1);
      expect(result.indexingProjects).toBe(1);
      expect(result.totalVectors).toBe(600);
      expect(result.totalFiles).toBe(300);
      expect(result.averageIndexingProgress).toBe(50); // (75 + 50 + 25) / 3
    });

    it('should return zero stats when no projects exist', () => {
      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = coreStateService.getProjectStats(projectStates);

      // Verify results
      expect(result.totalProjects).toBe(0);
      expect(result.activeProjects).toBe(0);
      expect(result.indexingProjects).toBe(0);
      expect(result.totalVectors).toBe(0);
      expect(result.totalFiles).toBe(0);
      expect(result.averageIndexingProgress).toBe(0);
    });
  });

  describe('updateProjectStatus', () => {
    it('should update project status', async () => {
      const projectId = 'test-project-id';
      const status: ProjectState['status'] = 'active';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({ projectId, status: 'inactive' });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await coreStateService.updateProjectStatus(projectStates, projectId, status, storagePath);

      // Verify results
      expect(projectState.status).toBe(status);
      expect(projectState.updatedAt).toBeInstanceOf(Date);
      expect(loggerService.debug).toHaveBeenCalledWith(`Updated project status for ${projectId}: ${status}`);
    });

    it('should not update status for non-existent project', async () => {
      const projectId = 'non-existent-project-id';
      const status: ProjectState['status'] = 'active';
      const storagePath = './data/project-states.json';

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      await coreStateService.updateProjectStatus(projectStates, projectId, status, storagePath);

      // Verify no error was thrown
      expect(loggerService.debug).not.toHaveBeenCalled();
    });
  });

  describe('updateProjectIndexingProgress', () => {
    it('should update project indexing progress', async () => {
      const projectId = 'test-project-id';
      const progress = 75;
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({ projectId });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await coreStateService.updateProjectIndexingProgress(projectStates, projectId, progress, storagePath);

      // Verify results
      expect(projectState.indexingProgress).toBe(progress);
      expect(projectState.updatedAt).toBeInstanceOf(Date);
      expect(loggerService.debug).toHaveBeenCalledWith(`Updated indexing progress for ${projectId}: ${progress}%`);
    });

    it('should not update progress for non-existent project', async () => {
      const projectId = 'non-existent-project-id';
      const progress = 75;
      const storagePath = './data/project-states.json';

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      await coreStateService.updateProjectIndexingProgress(projectStates, projectId, progress, storagePath);

      // Verify no error was thrown
      expect(loggerService.debug).not.toHaveBeenCalled();
    });
  });

  describe('updateProjectLastIndexed', () => {
    it('should update project last indexed time', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({ projectId });
      const indexStatus = {
        totalFiles: 100,
        indexedFiles: 95,
        failedFiles: 5
      };

      // Mock dependencies
      indexService.getIndexStatus.mockReturnValue(indexStatus as any);

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await coreStateService.updateProjectLastIndexed(projectStates, projectId, storagePath);

      // Verify results
      expect(projectState.lastIndexedAt).toBeInstanceOf(Date);
      expect(projectState.updatedAt).toBeInstanceOf(Date);
      expect(projectState.totalFiles).toBe(indexStatus.totalFiles);
      expect(projectState.indexedFiles).toBe(indexStatus.indexedFiles);
      expect(projectState.failedFiles).toBe(indexStatus.failedFiles);
      expect(loggerService.debug).toHaveBeenCalledWith(`Updated last indexed time for ${projectId}`);
    });

    it('should not update for non-existent project', async () => {
      const projectId = 'non-existent-project-id';
      const storagePath = './data/project-states.json';

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      await coreStateService.updateProjectLastIndexed(projectStates, projectId, storagePath);

      // Verify no error was thrown
      expect(loggerService.debug).not.toHaveBeenCalled();
    });
  });

  describe('updateProjectMetadata', () => {
    it('should update project metadata', async () => {
      const projectId = 'test-project-id';
      const metadata = { key: 'value', anotherKey: 'anotherValue' };
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({ projectId, metadata: { existingKey: 'existingValue' } });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await coreStateService.updateProjectMetadata(projectStates, projectId, metadata, storagePath);

      // Verify results
      expect(projectState.metadata).toEqual({
        existingKey: 'existingValue',
        key: 'value',
        anotherKey: 'anotherValue'
      });
      expect(projectState.updatedAt).toBeInstanceOf(Date);
      expect(loggerService.debug).toHaveBeenCalledWith(`Updated metadata for ${projectId}`);
    });

    it('should not update metadata for non-existent project', async () => {
      const projectId = 'non-existent-project-id';
      const metadata = { key: 'value' };
      const storagePath = './data/project-states.json';

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      await coreStateService.updateProjectMetadata(projectStates, projectId, metadata, storagePath);

      // Verify no error was thrown
      expect(loggerService.debug).not.toHaveBeenCalled();
    });
  });

  describe('refreshProjectState', () => {
    it('should refresh project state', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({ projectId });
      const collectionInfo = {
        name: 'project-test-project-id',
        pointsCount: 100,
        status: 'green'
      };

      // Mock dependencies
      projectIdManager.getProjectPath.mockReturnValue('/test/project');
      qdrantService.getCollectionInfoForProject.mockResolvedValue({
        ...collectionInfo,
        vectors: {
          size: 1536,
          distance: 'Cosine'
        },
        status: 'green' as const
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      const result = await coreStateService.refreshProjectState(projectStates, projectId, storagePath);

      // Verify results
      expect(result).toBe(projectState);
      expect(projectState.collectionInfo).toEqual({
        name: collectionInfo.name,
        vectorsCount: collectionInfo.pointsCount,
        status: collectionInfo.status
      });
      expect(projectState.updatedAt).toBeInstanceOf(Date);
    });

    it('should return null for non-existent project', async () => {
      const projectId = 'non-existent-project-id';
      const storagePath = './data/project-states.json';

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = await coreStateService.refreshProjectState(projectStates, projectId, storagePath);

      // Verify results
      expect(result).toBeNull();
    });

    it('should handle errors during refresh', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({ projectId });

      // Mock dependencies to simulate an error that gets caught and logged but doesn't bubble up
      projectIdManager.getProjectPath.mockImplementation(() => { throw new Error('Refresh failed'); });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      const result = await coreStateService.refreshProjectState(projectStates, projectId, storagePath);

      // Verify results - since the error is caught and handled internally, we still get the state back
      expect(result).toBe(projectState);
      expect(loggerService.warn).toHaveBeenCalledWith(
        `Failed to update collection info for ${projectId} after 3 attempts`,
        expect.any(Object)
      );
    });
  });

  describe('refreshAllProjectStates', () => {
    it('should refresh all project states', async () => {
      const storagePath = './data/project-states.json';
      const projectState1 = createMockProjectState({ projectId: 'project-1' });
      const projectState2 = createMockProjectState({ projectId: 'project-2' });

      // Mock dependencies
      projectIdManager.getProjectPath.mockImplementation((id) => {
        if (id === 'project-1') return '/test/project1';
        if (id === 'project-2') return '/test/project2';
        return undefined;
      });
      qdrantService.getCollectionInfoForProject.mockResolvedValue({
        name: 'project-test-project-id',
        pointsCount: 100,
        status: 'green',
        vectors: {
          size: 1536,
          distance: 'Cosine'
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set('project-1', projectState1);
      projectStates.set('project-2', projectState2);

      // Call the method
      await coreStateService.refreshAllProjectStates(projectStates, storagePath);

      // Verify results
      expect(projectIdManager.getProjectPath).toHaveBeenCalledWith('project-1');
      expect(projectIdManager.getProjectPath).toHaveBeenCalledWith('project-2');
      expect(qdrantService.getCollectionInfoForProject).toHaveBeenCalledTimes(2);
      expect(loggerService.info).toHaveBeenCalledWith('Refreshed all project states');
    });

    it('should handle errors during refresh', async () => {
      const storagePath = './data/project-states.json';

      // Mock dependencies to simulate an error that gets caught and logged but doesn't bubble up
      projectIdManager.getProjectPath.mockImplementation(() => { throw new Error('Refresh failed'); });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set('project-1', createMockProjectState({ projectId: 'project-1' }));

      // Call the method - should not throw since errors are handled internally
      await expect(coreStateService.refreshAllProjectStates(projectStates, storagePath)).resolves.toBeUndefined();

      // Verify error was handled
      expect(loggerService.warn).toHaveBeenCalledWith(
        `Failed to update collection info for project-1 after 3 attempts`,
        expect.any(Object)
      );
    });
  });

  describe('cleanupInvalidStates', () => {
    it('should clean up invalid project states', async () => {
      const storagePath = './data/project-states.json';
      const validProjectState = createMockProjectState({ projectId: 'valid-project' });
      const invalidProjectState = createMockProjectState({ projectId: 'invalid-project' });

      // Mock dependencies
      (ProjectStateValidator.isProjectStateValid as jest.Mock).mockImplementation(async (state: ProjectState) => {
        return state.projectId === 'valid-project';
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set('valid-project', validProjectState);
      projectStates.set('invalid-project', invalidProjectState);

      // Call the method
      const result = await coreStateService.cleanupInvalidStates(projectStates, storagePath);

      // Verify results
      expect(result).toBe(1);
      expect(projectStates.size).toBe(1);
      expect(projectStates.has('valid-project')).toBe(true);
      expect(projectStates.has('invalid-project')).toBe(false);
      expect(loggerService.info).toHaveBeenCalledWith(`Cleaned up 1 invalid project states, 1 states remaining`);
    });

    it('should handle errors during cleanup', async () => {
      const storagePath = './data/project-states.json';
      const error = new Error('Cleanup failed');

      // Mock dependencies
      (ProjectStateValidator.isProjectStateValid as jest.Mock).mockRejectedValue(error);

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set('project-1', createMockProjectState({ projectId: 'project-1' }));

      // Call the method and expect error
      await expect(coreStateService.cleanupInvalidStates(projectStates, storagePath)).rejects.toThrow('Cleanup failed');

      // Verify error was handled
      expect(errorHandlerService.handleError).toHaveBeenCalledWith(
        new Error(`Failed to cleanup invalid project states: ${error.message}`),
        { component: 'CoreStateService', operation: 'cleanupInvalidStates' }
      );
    });
  });
});