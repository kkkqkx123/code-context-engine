import { ProjectStateStorageUtils } from '../utils/ProjectStateStorageUtils';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { ProjectState } from '../ProjectStateManager';

// Mock dependencies
jest.mock('../../../utils/LoggerService');
jest.mock('../../../utils/ErrorHandlerService');
jest.mock('fs/promises');

// Mock HashUtils
jest.mock('../../../utils/HashUtils', () => ({
  HashUtils: {
    normalizePath: jest.fn((path: string) => path.replace(/\\/g, '/').replace(/\/$/, '')),
    calculateFileHash: jest.fn(),
    calculateDirectoryHash: jest.fn(),
    calculateStringHash: jest.fn(),
    generateId: jest.fn(),
    getFileExtension: jest.fn()
  }
}));

describe('ProjectStateStorageUtils', () => {
  let loggerService: jest.Mocked<LoggerService>;
  let errorHandlerService: jest.Mocked<ErrorHandlerService>;
  let mockFs: jest.Mocked<typeof import('fs/promises')>;
  let mockNormalizePath: jest.MockedFunction<(path: string) => string>;

  beforeEach(() => {
    // Reset all mocks
    jest.clearAllMocks();

    // Create mock instances
    loggerService = new LoggerService() as jest.Mocked<LoggerService>;
    errorHandlerService = new ErrorHandlerService(loggerService) as jest.Mocked<ErrorHandlerService>;
    mockFs = require('fs/promises') as jest.Mocked<typeof import('fs/promises')>;
    
    // Get HashUtils mock
    const HashUtils = require('../../../utils/HashUtils').HashUtils;
    mockNormalizePath = HashUtils.normalizePath as jest.MockedFunction<(path: string) => string>;

    // Set up default mock implementations
    mockFs.mkdir = jest.fn().mockResolvedValue(undefined);
    mockFs.readFile = jest.fn().mockResolvedValue('[]');
    mockFs.writeFile = jest.fn().mockResolvedValue(undefined);
    mockFs.unlink = jest.fn().mockResolvedValue(undefined);
    mockFs.rename = jest.fn().mockResolvedValue(undefined);
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

  describe('saveProjectStates', () => {
    it('should save project states successfully', async () => {
      const storagePath = './data/project-states.json';
      const projectStates = new Map<string, ProjectState>();
      const projectState = createMockProjectState();
      projectStates.set(projectState.projectId, projectState);

      // Call the method
      await ProjectStateStorageUtils.saveProjectStates(
        projectStates,
        storagePath,
        loggerService,
        errorHandlerService
      );

      // Verify results
      expect(mockFs.mkdir).toHaveBeenCalledWith(expect.any(String), { recursive: true });
      expect(mockFs.writeFile).toHaveBeenCalled();
      expect(loggerService.debug).toHaveBeenCalledWith(`Saved 1 project states (attempt 1)`);
    });

    it('should retry on failure', async () => {
      const storagePath = './data/project-states.json';
      const projectStates = new Map<string, ProjectState>();
      const projectState = createMockProjectState();
      projectStates.set(projectState.projectId, projectState);

      // Mock writeFile to fail twice then succeed
      mockFs.writeFile
        .mockRejectedValueOnce(new Error('Write failed'))
        .mockRejectedValueOnce(new Error('Write failed again'))
        .mockResolvedValueOnce(undefined);

      // Call the method
      await ProjectStateStorageUtils.saveProjectStates(
        projectStates,
        storagePath,
        loggerService,
        errorHandlerService
      );

      // Verify results
      expect(mockFs.writeFile).toHaveBeenCalledTimes(3);
      expect(loggerService.debug).toHaveBeenCalledWith(`Saved 1 project states (attempt 3)`);
    });

    it('should use exponential backoff for retries', async () => {
      const storagePath = './data/project-states.json';
      const projectStates = new Map<string, ProjectState>();
      const projectState = createMockProjectState();
      projectStates.set(projectState.projectId, projectState);

      // Mock writeFile to fail twice then succeed
      mockFs.writeFile
        .mockRejectedValueOnce(new Error('Write failed'))
        .mockRejectedValueOnce(new Error('Write failed again'))
        .mockResolvedValueOnce(undefined);

      // Mock setTimeout to track calls
      const originalSetTimeout = global.setTimeout;
      const setTimeoutMock = jest.fn().mockImplementation((fn, delay) => {
        // Call the function immediately for testing
        fn();
        return 1 as any;
      });
      global.setTimeout = setTimeoutMock as any;

      // Call the method
      await ProjectStateStorageUtils.saveProjectStates(
        projectStates,
        storagePath,
        loggerService,
        errorHandlerService
      );

      // Restore original setTimeout
      global.setTimeout = originalSetTimeout;

      // Verify exponential backoff
      expect(setTimeoutMock).toHaveBeenCalledTimes(2);
      expect(setTimeoutMock).toHaveBeenNthCalledWith(1, expect.any(Function), 200); // 2^1 * 100
      expect(setTimeoutMock).toHaveBeenNthCalledWith(2, expect.any(Function), 400); // 2^2 * 100
    });

    it('should log error but not throw after max retries', async () => {
      const storagePath = './data/project-states.json';
      const projectStates = new Map<string, ProjectState>();
      const projectState = createMockProjectState();
      projectStates.set(projectState.projectId, projectState);

      // Mock writeFile to always fail
      const error = new Error('Persistent write failure');
      mockFs.writeFile.mockRejectedValue(error);

      // Call the method
      await ProjectStateStorageUtils.saveProjectStates(
        projectStates,
        storagePath,
        loggerService,
        errorHandlerService
      );

      // Verify results
      expect(mockFs.writeFile).toHaveBeenCalledTimes(5); // Max retries
      expect(loggerService.error).toHaveBeenCalledWith(
        `Failed to save project states after 5 attempts, but memory state is still valid: ${error.message}`
      );
    });

    it('should use atomic write with temporary file', async () => {
      const storagePath = './data/project-states.json';
      const projectStates = new Map<string, ProjectState>();
      const projectState = createMockProjectState();
      projectStates.set(projectState.projectId, projectState);

      // Call the method
      await ProjectStateStorageUtils.saveProjectStates(
        projectStates,
        storagePath,
        loggerService,
        errorHandlerService
      );

      // Verify atomic write pattern
      expect(mockFs.writeFile).toHaveBeenCalledWith(
        expect.stringMatching(/\.tmp\.\d+\.\w+$/), // Temporary file pattern
        expect.any(String)
      );
      expect(mockFs.rename).toHaveBeenCalledWith(
        expect.stringMatching(/\.tmp\.\d+\.\w+$/), // Temporary file pattern
        storagePath
      );
    });

    it('should handle permission errors with fallback', async () => {
      const storagePath = './data/project-states.json';
      const projectStates = new Map<string, ProjectState>();
      const projectState = createMockProjectState();
      projectStates.set(projectState.projectId, projectState);

      // Mock permission error on rename
      const permissionError = new Error('Permission denied') as any;
      permissionError.code = 'EPERM';
      mockFs.rename.mockRejectedValue(permissionError);

      // Call the method
      await ProjectStateStorageUtils.saveProjectStates(
        projectStates,
        storagePath,
        loggerService,
        errorHandlerService
      );

      // Verify fallback was used
      expect(loggerService.warn).toHaveBeenCalledWith(
        'Permission error during atomic write, trying direct write as fallback'
      );
      expect(mockFs.writeFile).toHaveBeenCalledWith(storagePath, expect.any(String));
    });

    it('should clean up temporary file on error', async () => {
      const storagePath = './data/project-states.json';
      const projectStates = new Map<string, ProjectState>();
      const projectState = createMockProjectState();
      projectStates.set(projectState.projectId, projectState);

      // Mock error on rename
      const error = new Error('Rename failed');
      mockFs.rename.mockRejectedValue(error);

      // Call the method
      await ProjectStateStorageUtils.saveProjectStates(
        projectStates,
        storagePath,
        loggerService,
        errorHandlerService
      );

      // Verify temporary file was cleaned up
      expect(mockFs.unlink).toHaveBeenCalledWith(
        expect.stringMatching(/\.tmp\.\d+\.\w+$/) // Temporary file pattern
      );
    });
  });

  describe('loadProjectStates', () => {
    it('should load project states successfully', async () => {
      const storagePath = './data/project-states.json';
      const projectState = createMockProjectState();
      
      // Create raw states with date strings (as they would be after JSON.stringify)
      const rawStates = [{
        ...projectState,
        createdAt: projectState.createdAt.toISOString(),
        updatedAt: projectState.updatedAt.toISOString(),
        vectorStatus: {
          ...projectState.vectorStatus,
          lastUpdated: projectState.vectorStatus.lastUpdated.toISOString()
        },
        graphStatus: {
          ...projectState.graphStatus,
          lastUpdated: projectState.graphStatus.lastUpdated.toISOString()
        }
      }];

      // Mock dependencies
      mockFs.readFile.mockResolvedValue(JSON.stringify(rawStates));

      // Mock validation function
      const validateAndNormalizeProjectState = jest.fn().mockReturnValue(projectState);

      // Call the method
      const result = await ProjectStateStorageUtils.loadProjectStates(
        storagePath,
        loggerService,
        validateAndNormalizeProjectState
      );

      // Verify results
      expect(result).toBeInstanceOf(Map);
      expect(result.size).toBe(1);
      expect(result.get(projectState.projectId)).toBe(projectState);
      expect(validateAndNormalizeProjectState).toHaveBeenCalledWith(rawStates[0]);
      expect(loggerService.info).toHaveBeenCalledWith('Loaded 1 valid project states, skipped 0 invalid states');
    });

    it('should handle file not found error', async () => {
      const storagePath = './data/project-states.json';

      // Mock file not found error
      const fileNotFoundError = new Error('File not found') as any;
      fileNotFoundError.code = 'ENOENT';
      mockFs.readFile.mockRejectedValue(fileNotFoundError);

      // Mock validation function
      const validateAndNormalizeProjectState = jest.fn();

      // Call the method
      const result = await ProjectStateStorageUtils.loadProjectStates(
        storagePath,
        loggerService,
        validateAndNormalizeProjectState
      );

      // Verify results
      expect(result).toBeInstanceOf(Map);
      expect(result.size).toBe(0);
      expect(loggerService.info).toHaveBeenCalledWith('Project states file does not exist, initializing empty states');
    });

    it('should skip invalid project states', async () => {
      const storagePath = './data/project-states.json';
      const validProjectState = createMockProjectState({ projectId: 'valid-project' });
      const invalidRawState = { invalid: 'state' };
      const rawStates = [validProjectState, invalidRawState];

      // Mock dependencies
      mockFs.readFile.mockResolvedValue(JSON.stringify(rawStates));

      // Mock validation function
      const validateAndNormalizeProjectState = jest.fn().mockImplementation((rawState) => {
        if (rawState.projectId === 'valid-project') {
          return validProjectState;
        }
        throw new Error('Invalid state');
      });

      // Call the method
      const result = await ProjectStateStorageUtils.loadProjectStates(
        storagePath,
        loggerService,
        validateAndNormalizeProjectState
      );

      // Verify results
      expect(result.size).toBe(1);
      expect(result.get('valid-project')).toBe(validProjectState);
      expect(loggerService.info).toHaveBeenCalledWith('Loaded 1 valid project states, skipped 1 invalid states');
    });

    it('should skip duplicate project paths', async () => {
      const storagePath = './data/project-states.json';
      const projectState1 = createMockProjectState({
        projectId: 'project-1',
        projectPath: '/test/project'
      });
      const projectState2 = createMockProjectState({
        projectId: 'project-2',
        projectPath: '/test/project' // Same path as project 1
      });
      const rawStates = [projectState1, projectState2];

      // Mock dependencies
      mockFs.readFile.mockResolvedValue(JSON.stringify(rawStates));
      mockNormalizePath.mockImplementation((path: string) => path);

      // Mock validation function
      const validateAndNormalizeProjectState = jest.fn().mockImplementation((rawState) => {
        return rawState;
      });

      // Call the method
      const result = await ProjectStateStorageUtils.loadProjectStates(
        storagePath,
        loggerService,
        validateAndNormalizeProjectState
      );

      // Verify results
      expect(result.size).toBe(1);
      expect(result.has('project-1')).toBe(true);
      expect(result.has('project-2')).toBe(false);
      expect(loggerService.warn).toHaveBeenCalledWith(
        'Skipping duplicate project path: /test/project',
        { projectId: 'project-2' }
      );
      expect(loggerService.info).toHaveBeenCalledWith('Loaded 1 valid project states, skipped 1 invalid states');
    });

    it('should skip duplicate project IDs', async () => {
      const storagePath = './data/project-states.json';
      const projectState1 = createMockProjectState({
        projectId: 'duplicate-id',
        projectPath: '/test/project1'
      });
      const projectState2 = createMockProjectState({
        projectId: 'duplicate-id', // Same ID as project 1
        projectPath: '/test/project2'
      });
      const rawStates = [projectState1, projectState2];

      // Mock dependencies
      mockFs.readFile.mockResolvedValue(JSON.stringify(rawStates));
      mockNormalizePath.mockImplementation((path: string) => path);

      // Mock validation function
      const validateAndNormalizeProjectState = jest.fn().mockImplementation((rawState) => {
        return rawState;
      });

      // Call the method
      const result = await ProjectStateStorageUtils.loadProjectStates(
        storagePath,
        loggerService,
        validateAndNormalizeProjectState
      );

      // Verify results
      expect(result.size).toBe(1);
      expect(loggerService.warn).toHaveBeenCalledWith(
        'Skipping duplicate project ID: duplicate-id',
        { projectPath: '/test/project2' }
      );
      expect(loggerService.info).toHaveBeenCalledWith('Loaded 1 valid project states, skipped 1 invalid states');
    });

    it('should handle different path formats as duplicates', async () => {
      const storagePath = './data/project-states.json';
      const projectState1 = createMockProjectState({
        projectId: 'project-1',
        projectPath: '/test/project'
      });
      const projectState2 = createMockProjectState({
        projectId: 'project-2',
        projectPath: '/test/project/' // Same path with trailing slash
      });
      const projectState3 = createMockProjectState({
        projectId: 'project-3',
        projectPath: '\\test\\project' // Same path with Windows separators
      });
      const rawStates = [projectState1, projectState2, projectState3];

      // Mock dependencies
      mockFs.readFile.mockResolvedValue(JSON.stringify(rawStates));
      mockNormalizePath.mockImplementation((path: string) => {
        // Normalize all paths to the same format
        return path.replace(/\\/g, '/').replace(/\/$/, '');
      });

      // Mock validation function
      const validateAndNormalizeProjectState = jest.fn().mockImplementation((rawState) => {
        return rawState;
      });

      // Call the method
      const result = await ProjectStateStorageUtils.loadProjectStates(
        storagePath,
        loggerService,
        validateAndNormalizeProjectState
      );

      // Verify results
      expect(result.size).toBe(1);
      expect(result.has('project-1')).toBe(true);
      expect(loggerService.warn).toHaveBeenCalledWith(
        'Detected and skipped 2 duplicate project paths'
      );
      expect(loggerService.info).toHaveBeenCalledWith('Loaded 1 valid project states, skipped 2 invalid states');
    });

    it('should throw error for non-file system errors', async () => {
      const storagePath = './data/project-states.json';
      const error = new Error('Permission denied');

      // Mock dependencies
      mockFs.readFile.mockRejectedValue(error);

      // Mock validation function
      const validateAndNormalizeProjectState = jest.fn();

      // Call the method and expect error
      await expect(ProjectStateStorageUtils.loadProjectStates(
        storagePath,
        loggerService,
        validateAndNormalizeProjectState
      )).rejects.toThrow('Permission denied');
    });
  });

  describe('ensureStorageDirectory', () => {
    it('should create directory if it does not exist', async () => {
      const storagePath = './data/project-states.json';

      // Call the method
      await ProjectStateStorageUtils.ensureStorageDirectory(storagePath);

      // Verify results
      expect(mockFs.mkdir).toHaveBeenCalledWith('./data', { recursive: true });
    });
  });

  describe('formatDate', () => {
    it('should format date to ISO string', () => {
      const date = new Date('2023-01-01T00:00:00.000Z');

      // Call the method
      const result = ProjectStateStorageUtils.formatDate(date);

      // Verify results
      expect(result).toBe(date.toISOString());
    });
  });

  describe('parseDate', () => {
    it('should parse ISO string to Date object', () => {
      const dateString = '2023-01-01T00:00:00.000Z';

      // Call the method
      const result = ProjectStateStorageUtils.parseDate(dateString);

      // Verify results
      expect(result).toBeInstanceOf(Date);
      expect(result.toISOString()).toBe(dateString);
    });
  });
});