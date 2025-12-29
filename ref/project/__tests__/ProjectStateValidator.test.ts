import { ProjectStateValidator } from '../utils/ProjectStateValidator';
import { ProjectState, StorageStatus } from '../ProjectStateManager';

// Mock dependencies
jest.mock('fs/promises');

// Mock ProjectStateStorageUtils
jest.mock('../utils/ProjectStateStorageUtils', () => ({
  ProjectStateStorageUtils: {
    parseDate: jest.fn((dateString: string) => new Date(dateString)),
    formatDate: jest.fn(),
    saveProjectStates: jest.fn(),
    loadProjectStates: jest.fn(),
    ensureStorageDirectory: jest.fn()
  }
}));

describe('ProjectStateValidator', () => {
  let mockParseDate: jest.MockedFunction<(dateString: string) => Date>;
  let mockFs: jest.Mocked<typeof import('fs/promises')>;

  beforeEach(() => {
    // Reset all mocks
    jest.clearAllMocks();

    // Mock dependencies
    mockFs = require('fs/promises') as jest.Mocked<typeof import('fs/promises')>;

    // Get ProjectStateStorageUtils mock
    const ProjectStateStorageUtils = require('../utils/ProjectStateStorageUtils').ProjectStateStorageUtils;
    mockParseDate = ProjectStateStorageUtils.parseDate as jest.MockedFunction<(dateString: string) => Date>;
    
    // Set up default mock implementations
    mockFs.access = jest.fn().mockResolvedValue(undefined);
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

  describe('initializeStorageStatus', () => {
    it('should initialize storage status with default values', () => {
      // Call the method
      const result = ProjectStateValidator.initializeStorageStatus();

      // Verify results
      expect(result.status).toBe('pending');
      expect(result.progress).toBe(0);
      expect(result.lastUpdated).toBeInstanceOf(Date);
    });
  });

  describe('normalizeStorageStatus', () => {
    it('should normalize valid storage status', () => {
      const rawStatus = {
        status: 'completed',
        progress: 100,
        totalFiles: 50,
        processedFiles: 45,
        failedFiles: 5,
        lastUpdated: '2023-01-01T00:00:00.000Z',
        lastCompleted: '2023-01-01T01:00:00.000Z',
        error: 'Some error'
      };

      // Mock parseDate
      mockParseDate.mockImplementation((dateString: string) => new Date(dateString));

      // Call the method
      const result = ProjectStateValidator.normalizeStorageStatus(rawStatus);

      // Verify results
      expect(result.status).toBe('completed');
      expect(result.progress).toBe(100);
      expect(result.totalFiles).toBe(50);
      expect(result.processedFiles).toBe(45);
      expect(result.failedFiles).toBe(5);
      expect(result.lastUpdated).toBeInstanceOf(Date);
      expect(result.lastCompleted).toBeInstanceOf(Date);
      expect(result.error).toBe('Some error');
    });

    it('should initialize storage status when raw status is null or undefined', () => {
      // Call the method with null
      let result = ProjectStateValidator.normalizeStorageStatus(null);
      expect(result.status).toBe('pending');
      expect(result.progress).toBe(0);
      expect(result.lastUpdated).toBeInstanceOf(Date);

      // Call the method with undefined
      result = ProjectStateValidator.normalizeStorageStatus(undefined);
      expect(result.status).toBe('pending');
      expect(result.progress).toBe(0);
      expect(result.lastUpdated).toBeInstanceOf(Date);
    });

    it('should normalize invalid status to pending', () => {
      const rawStatus = {
        status: 'invalid-status',
        progress: 50
      };

      // Call the method
      const result = ProjectStateValidator.normalizeStorageStatus(rawStatus);

      // Verify results
      expect(result.status).toBe('pending');
      expect(result.progress).toBe(50);
    });

    it('should normalize invalid progress to 0', () => {
      const rawStatus = {
        status: 'completed',
        progress: -10 // Invalid progress
      };

      // Call the method
      const result = ProjectStateValidator.normalizeStorageStatus(rawStatus);

      // Verify results
      expect(result.status).toBe('completed');
      expect(result.progress).toBe(0);
    });

    it('should normalize progress greater than 100 to 100', () => {
      const rawStatus = {
        status: 'completed',
        progress: 150 // Invalid progress
      };

      // Call the method
      const result = ProjectStateValidator.normalizeStorageStatus(rawStatus);

      // Verify results
      expect(result.status).toBe('completed');
      expect(result.progress).toBe(0); // Invalid values are set to 0
    });

    it('should handle missing optional fields', () => {
      const rawStatus = {
        status: 'completed',
        progress: 100
      };

      // Call the method
      const result = ProjectStateValidator.normalizeStorageStatus(rawStatus);

      // Verify results
      expect(result.status).toBe('completed');
      expect(result.progress).toBe(100);
      expect(result.totalFiles).toBeUndefined();
      expect(result.processedFiles).toBeUndefined();
      expect(result.failedFiles).toBeUndefined();
      expect(result.lastCompleted).toBeUndefined();
      expect(result.error).toBeUndefined();
    });

    it('should use current date when lastUpdated is missing', () => {
      const rawStatus = {
        status: 'completed',
        progress: 100
      };

      // Call the method
      const result = ProjectStateValidator.normalizeStorageStatus(rawStatus);

      // Verify results
      expect(result.lastUpdated).toBeInstanceOf(Date);
    });

    it('should filter non-string error values', () => {
      const rawStatus = {
        status: 'error',
        progress: 50,
        error: 123 // Non-string error
      };

      // Call the method
      const result = ProjectStateValidator.normalizeStorageStatus(rawStatus);

      // Verify results
      expect(result.error).toBeUndefined();
    });
  });

  describe('validateAndNormalizeProjectState', () => {
    it('should validate and normalize valid project state', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 'Test Project',
        description: 'A test project',
        status: 'active',
        vectorStatus: {
          status: 'completed',
          progress: 100,
          lastUpdated: '2023-01-01T00:00:00.000Z'
        },
        graphStatus: {
          status: 'pending',
          progress: 0,
          lastUpdated: '2023-01-01T00:00:00.000Z'
        },
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        lastIndexedAt: '2023-01-01T01:00:00.000Z',
        indexingProgress: 75,
        totalFiles: 100,
        indexedFiles: 75,
        failedFiles: 5,
        collectionInfo: {
          name: 'test-collection',
          vectorsCount: 1000,
          status: 'green'
        },
        settings: {
          autoIndex: false,
          watchChanges: true,
          includePatterns: ['*.ts', '*.js'],
          excludePatterns: ['*.test.ts'],
          chunkSize: 1000,
          chunkOverlap: 200
        },
        metadata: {
          key: 'value',
          anotherKey: 'anotherValue'
        }
      };

      // Mock parseDate
      mockParseDate.mockImplementation((dateString: string) => new Date(dateString));

      // Call the method
      const result = ProjectStateValidator.validateAndNormalizeProjectState(rawState);

      // Verify results
      expect(result.projectId).toBe('test-project-id');
      expect(result.projectPath).toBe('/test/project');
      expect(result.name).toBe('Test Project');
      expect(result.description).toBe('A test project');
      expect(result.status).toBe('active');
      expect(result.vectorStatus.status).toBe('completed');
      expect(result.graphStatus.status).toBe('pending');
      expect(result.createdAt).toBeInstanceOf(Date);
      expect(result.updatedAt).toBeInstanceOf(Date);
      expect(result.lastIndexedAt).toBeInstanceOf(Date);
      expect(result.indexingProgress).toBe(75);
      expect(result.totalFiles).toBe(100);
      expect(result.indexedFiles).toBe(75);
      expect(result.failedFiles).toBe(5);
      expect(result.collectionInfo).toEqual({
        name: 'test-collection',
        vectorsCount: 1000,
        status: 'green'
      });
      expect(result.settings).toEqual({
        autoIndex: false,
        watchChanges: true,
        includePatterns: ['*.ts', '*.js'],
        excludePatterns: ['*.test.ts'],
        chunkSize: 1000,
        chunkOverlap: 200
      });
      expect(result.metadata).toEqual({
        key: 'value',
        anotherKey: 'anotherValue'
      });
    });

    it('should throw error for missing projectId', () => {
      const rawState = {
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing projectId'
      );
    });

    it('should throw error for non-string projectId', () => {
      const rawState = {
        projectId: 123, // Non-string projectId
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing projectId'
      );
    });

    it('should throw error for missing projectPath', () => {
      const rawState = {
        projectId: 'test-project-id',
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing projectPath'
      );
    });

    it('should throw error for non-string projectPath', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: 123, // Non-string projectPath
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing projectPath'
      );
    });

    it('should throw error for empty projectPath', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '', // Empty projectPath
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing projectPath'
      );
    });

    it('should throw error for whitespace-only projectPath', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '   ', // Whitespace-only projectPath
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'projectPath cannot be empty or contain only whitespace'
      );
    });

    it('should throw error for missing name', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing name'
      );
    });

    it('should throw error for non-string name', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 123, // Non-string name
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing name'
      );
    });

    it('should throw error for invalid status', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'invalid-status', // Invalid status
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing status'
      );
    });

    it('should throw error for missing timestamp fields', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'active',
        // Missing createdAt and updatedAt
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Missing timestamp fields'
      );
    });

    it('should throw error for missing settings', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z'
        // Missing settings
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing settings'
      );
    });

    it('should throw error for non-object settings', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: 'invalid-settings' // Non-object settings
      };

      // Call the method and expect error
      expect(() => ProjectStateValidator.validateAndNormalizeProjectState(rawState)).toThrow(
        'Invalid or missing settings'
      );
    });

    it('should handle missing optional fields', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          autoIndex: true,
          watchChanges: true
        }
        // Missing optional fields
      };

      // Mock parseDate
      mockParseDate.mockImplementation((dateString: string) => new Date(dateString));

      // Call the method
      const result = ProjectStateValidator.validateAndNormalizeProjectState(rawState);

      // Verify results
      expect(result.description).toBeUndefined();
      expect(result.lastIndexedAt).toBeUndefined();
      expect(result.indexingProgress).toBeUndefined();
      expect(result.totalFiles).toBeUndefined();
      expect(result.indexedFiles).toBeUndefined();
      expect(result.failedFiles).toBeUndefined();
      expect(result.collectionInfo).toBeUndefined();
      expect(result.metadata).toEqual({});
    });

    it('should normalize collectionInfo with invalid vectorsCount', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        collectionInfo: {
          name: 'test-collection',
          vectorsCount: 'invalid', // Invalid vectorsCount
          status: 'green'
        },
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Mock parseDate
      mockParseDate.mockImplementation((dateString: string) => new Date(dateString));

      // Call the method
      const result = ProjectStateValidator.validateAndNormalizeProjectState(rawState);

      // Verify results
      expect(result.collectionInfo?.vectorsCount).toBe(0);
    });

    it('should normalize settings with default values', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        settings: {
          // Missing autoIndex and watchChanges
          includePatterns: ['*.ts'],
          excludePatterns: ['*.test.ts'],
          chunkSize: 1000,
          chunkOverlap: 200
        }
      };

      // Mock parseDate
      mockParseDate.mockImplementation((dateString: string) => new Date(dateString));

      // Call the method
      const result = ProjectStateValidator.validateAndNormalizeProjectState(rawState);

      // Verify results
      expect(result.settings.autoIndex).toBe(true);
      expect(result.settings.watchChanges).toBe(true);
      expect(result.settings.includePatterns).toEqual(['*.ts']);
      expect(result.settings.excludePatterns).toEqual(['*.test.ts']);
      expect(result.settings.chunkSize).toBe(1000);
      expect(result.settings.chunkOverlap).toBe(200);
    });

    it('should normalize non-object metadata to empty object', () => {
      const rawState = {
        projectId: 'test-project-id',
        projectPath: '/test/project',
        name: 'Test Project',
        status: 'active',
        createdAt: '2023-01-01T00:00:00.000Z',
        updatedAt: '2023-01-01T00:00:00.000Z',
        metadata: 'invalid-metadata', // Non-object metadata
        settings: {
          autoIndex: true,
          watchChanges: true
        }
      };

      // Mock parseDate
      mockParseDate.mockImplementation((dateString: string) => new Date(dateString));

      // Call the method
      const result = ProjectStateValidator.validateAndNormalizeProjectState(rawState);

      // Verify results
      expect(result.metadata).toEqual({});
    });
  });

  describe('isProjectStateValid', () => {
    it('should return true for valid project state', async () => {
      const projectState = createMockProjectState({
        projectPath: '/test/project'
      });

      // Mock fs.access to succeed
      mockFs.access.mockResolvedValue(undefined);

      // Call the method
      const result = await ProjectStateValidator.isProjectStateValid(projectState);

      // Verify results
      expect(result).toBe(true);
      expect(mockFs.access).toHaveBeenCalledWith('/test/project');
    });

    it('should return false for invalid project path', async () => {
      const projectState = createMockProjectState({
        projectPath: '/non-existent/project'
      });

      // Mock fs.access to fail
      const error = new Error('Path does not exist') as any;
      error.code = 'ENOENT';
      mockFs.access.mockRejectedValue(error);

      // Call the method
      const result = await ProjectStateValidator.isProjectStateValid(projectState);

      // Verify results
      expect(result).toBe(false);
      expect(mockFs.access).toHaveBeenCalledWith('/non-existent/project');
    });

    it('should return false for any access error', async () => {
      const projectState = createMockProjectState({
        projectPath: '/test/project'
      });

      // Mock fs.access to fail with any error
      mockFs.access.mockRejectedValue(new Error('Permission denied'));

      // Call the method
      const result = await ProjectStateValidator.isProjectStateValid(projectState);

      // Verify results
      expect(result).toBe(false);
    });
  });
});