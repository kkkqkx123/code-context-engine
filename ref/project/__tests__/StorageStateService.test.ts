import { StorageStateService } from '../services/StorageStateService';
import { LoggerService } from '../../../utils/LoggerService';
import { ProjectState, StorageStatus } from '../ProjectStateManager';

// Mock dependencies
jest.mock('../../../utils/LoggerService');

describe('StorageStateService', () => {
  let storageStateService: StorageStateService;
  let loggerService: jest.Mocked<LoggerService>;

  beforeEach(() => {
    // Reset all mocks
    jest.clearAllMocks();

    // Create mock instances
    loggerService = new LoggerService() as jest.Mocked<LoggerService>;

    // Create service instance
    storageStateService = new StorageStateService(loggerService);
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

  describe('updateVectorStatus', () => {
    it('should update vector status', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const status: Partial<StorageStatus> = {
        status: 'indexing',
        progress: 50
      };
      const projectState = createMockProjectState({
        projectId,
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateVectorStatus(projectStates, projectId, status, storagePath);

      // Verify results
      expect(projectState.vectorStatus.status).toBe('indexing');
      expect(projectState.vectorStatus.progress).toBe(50);
      expect(projectState.vectorStatus.lastUpdated).toBeInstanceOf(Date);
      expect(loggerService.debug).toHaveBeenCalledWith(`Updated vector status for ${projectId}: indexing`);
    });

    it('should not update status for non-existent project', async () => {
      const projectId = 'non-existent-project-id';
      const storagePath = './data/project-states.json';
      const status: Partial<StorageStatus> = {
        status: 'indexing',
        progress: 50
      };

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      await storageStateService.updateVectorStatus(projectStates, projectId, status, storagePath);

      // Verify no error was thrown
      expect(loggerService.debug).not.toHaveBeenCalled();
    });

    it('should update main status based on vector status', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const status: Partial<StorageStatus> = {
        status: 'indexing',
        progress: 50
      };
      const projectState = createMockProjectState({
        projectId,
        status: 'inactive',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateVectorStatus(projectStates, projectId, status, storagePath);

      // Verify results
      expect(projectState.status).toBe('indexing');
    });
  });

  describe('updateGraphStatus', () => {
    it('should update graph status', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const status: Partial<StorageStatus> = {
        status: 'indexing',
        progress: 50
      };
      const projectState = createMockProjectState({
        projectId,
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateGraphStatus(projectStates, projectId, status, storagePath);

      // Verify results
      expect(projectState.graphStatus.status).toBe('indexing');
      expect(projectState.graphStatus.progress).toBe(50);
      expect(projectState.graphStatus.lastUpdated).toBeInstanceOf(Date);
      expect(loggerService.debug).toHaveBeenCalledWith(`Updated graph status for ${projectId}: indexing`);
    });

    it('should not update status for non-existent project', async () => {
      const projectId = 'non-existent-project-id';
      const storagePath = './data/project-states.json';
      const status: Partial<StorageStatus> = {
        status: 'indexing',
        progress: 50
      };

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      await storageStateService.updateGraphStatus(projectStates, projectId, status, storagePath);

      // Verify no error was thrown
      expect(loggerService.debug).not.toHaveBeenCalled();
    });

    it('should update main status based on graph status', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const status: Partial<StorageStatus> = {
        status: 'indexing',
        progress: 50
      };
      const projectState = createMockProjectState({
        projectId,
        status: 'inactive',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateGraphStatus(projectStates, projectId, status, storagePath);

      // Verify results
      expect(projectState.status).toBe('indexing');
    });
  });

  describe('getVectorStatus', () => {
    it('should return vector status for existing project', () => {
      const projectId = 'test-project-id';
      const vectorStatus: StorageStatus = {
        status: 'completed',
        progress: 100,
        lastUpdated: new Date()
      };
      const projectState = createMockProjectState({
        projectId,
        vectorStatus
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      const result = storageStateService.getVectorStatus(projectStates, projectId);

      // Verify results
      expect(result).toBe(vectorStatus);
    });

    it('should return null for non-existent project', () => {
      const projectId = 'non-existent-project-id';

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = storageStateService.getVectorStatus(projectStates, projectId);

      // Verify results
      expect(result).toBeNull();
    });
  });

  describe('getGraphStatus', () => {
    it('should return graph status for existing project', () => {
      const projectId = 'test-project-id';
      const graphStatus: StorageStatus = {
        status: 'completed',
        progress: 100,
        lastUpdated: new Date()
      };
      const projectState = createMockProjectState({
        projectId,
        graphStatus
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      const result = storageStateService.getGraphStatus(projectStates, projectId);

      // Verify results
      expect(result).toBe(graphStatus);
    });

    it('should return null for non-existent project', () => {
      const projectId = 'non-existent-project-id';

      // Create empty project states map
      const projectStates = new Map<string, ProjectState>();

      // Call the method
      const result = storageStateService.getGraphStatus(projectStates, projectId);

      // Verify results
      expect(result).toBeNull();
    });
  });

  describe('resetVectorStatus', () => {
    it('should reset vector status', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        vectorStatus: {
          status: 'completed',
          progress: 100,
          lastUpdated: new Date(),
          processedFiles: 50,
          failedFiles: 5,
          error: 'Some error'
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.resetVectorStatus(projectStates, projectId, storagePath);

      // Verify results
      expect(projectState.vectorStatus.status).toBe('pending');
      expect(projectState.vectorStatus.progress).toBe(0);
      expect(projectState.vectorStatus.processedFiles).toBe(0);
      expect(projectState.vectorStatus.failedFiles).toBe(0);
      expect(projectState.vectorStatus.error).toBeUndefined();
      expect(projectState.vectorStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('resetGraphStatus', () => {
    it('should reset graph status', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        graphStatus: {
          status: 'completed',
          progress: 100,
          lastUpdated: new Date(),
          processedFiles: 50,
          failedFiles: 5,
          error: 'Some error'
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.resetGraphStatus(projectStates, projectId, storagePath);

      // Verify results
      expect(projectState.graphStatus.status).toBe('pending');
      expect(projectState.graphStatus.progress).toBe(0);
      expect(projectState.graphStatus.processedFiles).toBe(0);
      expect(projectState.graphStatus.failedFiles).toBe(0);
      expect(projectState.graphStatus.error).toBeUndefined();
      expect(projectState.graphStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('startVectorIndexing', () => {
    it('should start vector indexing', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const totalFiles = 100;
      const projectState = createMockProjectState({
        projectId,
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.startVectorIndexing(projectStates, projectId, totalFiles, storagePath);

      // Verify results
      expect(projectState.vectorStatus.status).toBe('indexing');
      expect(projectState.vectorStatus.progress).toBe(0);
      expect(projectState.vectorStatus.totalFiles).toBe(totalFiles);
      expect(projectState.vectorStatus.processedFiles).toBe(0);
      expect(projectState.vectorStatus.failedFiles).toBe(0);
      expect(projectState.vectorStatus.error).toBeUndefined();
      expect(projectState.vectorStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('startGraphIndexing', () => {
    it('should start graph indexing', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const totalFiles = 100;
      const projectState = createMockProjectState({
        projectId,
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.startGraphIndexing(projectStates, projectId, totalFiles, storagePath);

      // Verify results
      expect(projectState.graphStatus.status).toBe('indexing');
      expect(projectState.graphStatus.progress).toBe(0);
      expect(projectState.graphStatus.totalFiles).toBe(totalFiles);
      expect(projectState.graphStatus.processedFiles).toBe(0);
      expect(projectState.graphStatus.failedFiles).toBe(0);
      expect(projectState.graphStatus.error).toBeUndefined();
      expect(projectState.graphStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('updateVectorIndexingProgress', () => {
    it('should update vector indexing progress', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const progress = 50;
      const processedFiles = 45;
      const failedFiles = 5;
      const projectState = createMockProjectState({
        projectId,
        vectorStatus: {
          status: 'indexing',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateVectorIndexingProgress(
        projectStates,
        projectId,
        progress,
        processedFiles,
        failedFiles,
        storagePath
      );

      // Verify results
      expect(projectState.vectorStatus.progress).toBe(progress);
      expect(projectState.vectorStatus.processedFiles).toBe(processedFiles);
      expect(projectState.vectorStatus.failedFiles).toBe(failedFiles);
      expect(projectState.vectorStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('updateGraphIndexingProgress', () => {
    it('should update graph indexing progress', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const progress = 50;
      const processedFiles = 45;
      const failedFiles = 5;
      const projectState = createMockProjectState({
        projectId,
        graphStatus: {
          status: 'indexing',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateGraphIndexingProgress(
        projectStates,
        projectId,
        progress,
        processedFiles,
        failedFiles,
        storagePath
      );

      // Verify results
      expect(projectState.graphStatus.progress).toBe(progress);
      expect(projectState.graphStatus.processedFiles).toBe(processedFiles);
      expect(projectState.graphStatus.failedFiles).toBe(failedFiles);
      expect(projectState.graphStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('completeVectorIndexing', () => {
    it('should complete vector indexing', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        vectorStatus: {
          status: 'indexing',
          progress: 50,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.completeVectorIndexing(projectStates, projectId, storagePath);

      // Verify results
      expect(projectState.vectorStatus.status).toBe('completed');
      expect(projectState.vectorStatus.progress).toBe(100);
      expect(projectState.vectorStatus.lastCompleted).toBeInstanceOf(Date);
      expect(projectState.vectorStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('completeGraphIndexing', () => {
    it('should complete graph indexing', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        graphStatus: {
          status: 'indexing',
          progress: 50,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.completeGraphIndexing(projectStates, projectId, storagePath);

      // Verify results
      expect(projectState.graphStatus.status).toBe('completed');
      expect(projectState.graphStatus.progress).toBe(100);
      expect(projectState.graphStatus.lastCompleted).toBeInstanceOf(Date);
      expect(projectState.graphStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('failVectorIndexing', () => {
    it('should fail vector indexing', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const error = 'Indexing failed';
      const projectState = createMockProjectState({
        projectId,
        vectorStatus: {
          status: 'indexing',
          progress: 50,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.failVectorIndexing(projectStates, projectId, error, storagePath);

      // Verify results
      expect(projectState.vectorStatus.status).toBe('error');
      expect(projectState.vectorStatus.error).toBe(error);
      expect(projectState.vectorStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('failGraphIndexing', () => {
    it('should fail graph indexing', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const error = 'Indexing failed';
      const projectState = createMockProjectState({
        projectId,
        graphStatus: {
          status: 'indexing',
          progress: 50,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.failGraphIndexing(projectStates, projectId, error, storagePath);

      // Verify results
      expect(projectState.graphStatus.status).toBe('error');
      expect(projectState.graphStatus.error).toBe(error);
      expect(projectState.graphStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('disableGraphIndexing', () => {
    it('should disable graph indexing', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date(),
          processedFiles: 10,
          failedFiles: 2,
          error: 'Some error'
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.disableGraphIndexing(projectStates, projectId, storagePath);

      // Verify results
      expect(projectState.graphStatus.status).toBe('disabled');
      expect(projectState.graphStatus.progress).toBe(0);
      expect(projectState.graphStatus.processedFiles).toBe(0);
      expect(projectState.graphStatus.failedFiles).toBe(0);
      expect(projectState.graphStatus.error).toBeUndefined();
      expect(projectState.graphStatus.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('updateMainStatusBasedOnStorageStates', () => {
    it('should set main status to indexing when vector is indexing', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        status: 'inactive',
        vectorStatus: {
          status: 'indexing',
          progress: 50,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateVectorStatus(projectStates, projectId, {
        status: 'indexing',
        progress: 75
      }, storagePath);

      // Verify results
      expect(projectState.status).toBe('indexing');
    });

    it('should set main status to indexing when graph is indexing', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        status: 'inactive',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'indexing',
          progress: 50,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateGraphStatus(projectStates, projectId, {
        status: 'indexing',
        progress: 75
      }, storagePath);

      // Verify results
      expect(projectState.status).toBe('indexing');
    });

    it('should set main status to error when vector has error', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        status: 'inactive',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateVectorStatus(projectStates, projectId, {
        status: 'error',
        error: 'Vector indexing failed'
      }, storagePath);

      // Verify results
      expect(projectState.status).toBe('error');
    });

    it('should set main status to error when graph has error', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        status: 'inactive',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Call the method
      await storageStateService.updateGraphStatus(projectStates, projectId, {
        status: 'error',
        error: 'Graph indexing failed'
      }, storagePath);

      // Verify results
      expect(projectState.status).toBe('error');
    });

    it('should set main status to active when vector completed and graph completed', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        status: 'inactive',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Update vector status to completed
      await storageStateService.updateVectorStatus(projectStates, projectId, {
        status: 'completed',
        progress: 100
      }, storagePath);

      // Update graph status to completed
      await storageStateService.updateGraphStatus(projectStates, projectId, {
        status: 'completed',
        progress: 100
      }, storagePath);

      // Verify results
      expect(projectState.status).toBe('active');
    });

    it('should set main status to active when vector completed and graph disabled', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        status: 'inactive',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Update vector status to completed
      await storageStateService.updateVectorStatus(projectStates, projectId, {
        status: 'completed',
        progress: 100
      }, storagePath);

      // Update graph status to disabled
      await storageStateService.updateGraphStatus(projectStates, projectId, {
        status: 'disabled'
      }, storagePath);

      // Verify results
      expect(projectState.status).toBe('active');
    });

    it('should set main status to inactive when vector pending and graph disabled', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        status: 'active',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Update graph status to disabled
      await storageStateService.updateGraphStatus(projectStates, projectId, {
        status: 'disabled'
      }, storagePath);

      // Verify results
      expect(projectState.status).toBe('inactive');
    });

    it('should set main status to inactive when both vector and graph pending', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        status: 'active',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Update vector status to pending (explicitly)
      await storageStateService.updateVectorStatus(projectStates, projectId, {
        status: 'pending'
      }, storagePath);

      // Update graph status to pending (explicitly)
      await storageStateService.updateGraphStatus(projectStates, projectId, {
        status: 'pending'
      }, storagePath);

      // Verify results
      expect(projectState.status).toBe('inactive');
    });

    it('should set main status to active for partial completion', async () => {
      const projectId = 'test-project-id';
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState({
        projectId,
        status: 'inactive',
        vectorStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: new Date()
        }
      });

      // Create project states map
      const projectStates = new Map<string, ProjectState>();
      projectStates.set(projectId, projectState);

      // Update vector status to completed
      await storageStateService.updateVectorStatus(projectStates, projectId, {
        status: 'completed',
        progress: 100
      }, storagePath);

      // Update graph status to partial
      await storageStateService.updateGraphStatus(projectStates, projectId, {
        status: 'partial',
        progress: 50
      }, storagePath);

      // Verify results
      expect(projectState.status).toBe('active');
    });
  });
});