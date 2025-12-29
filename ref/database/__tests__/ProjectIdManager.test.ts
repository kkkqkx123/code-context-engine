import { ProjectIdManager } from '../ProjectIdManager';
import { HashUtils } from '../../utils/cache/HashUtils';
import { ConfigService } from '../../config/ConfigService';
import { QdrantConfigService } from '../../config/service/QdrantConfigService';
import { NebulaConfigService } from '../../config/service/NebulaConfigService';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';

// Mock HashUtils
jest.mock('../../utils/HashUtils', () => ({
  HashUtils: {
    normalizePath: jest.fn(),
    calculateDirectoryHash: jest.fn(),
  },
}));

// Mock fs
jest.mock('fs/promises', () => ({
  mkdir: jest.fn(),
  writeFile: jest.fn(),
  readFile: jest.fn(),
}));

// Mock ConfigService
jest.mock('../../config/ConfigService', () => ({
  ConfigService: jest.fn().mockImplementation(() => ({
    get: jest.fn().mockReturnValue({ mappingPath: './data/test-project-mapping.json' })
  }))
}));

// Mock QdrantConfigService
jest.mock('../../config/service/QdrantConfigService', () => ({
  QdrantConfigService: jest.fn().mockImplementation(() => ({
    getCollectionNameForProject: jest.fn().mockImplementation((projectId: string) => `project-${projectId}`)
  }))
}));

// Mock NebulaConfigService
jest.mock('../../config/service/NebulaConfigService', () => ({
  NebulaConfigService: jest.fn().mockImplementation(() => ({
    getSpaceNameForProject: jest.fn().mockImplementation((projectId: string) => `project-${projectId}`)
  }))
}));

// Mock LoggerService
jest.mock('../../utils/LoggerService', () => ({
  LoggerService: jest.fn().mockImplementation(() => ({
    info: jest.fn(),
    warn: jest.fn(),
    error: jest.fn(),
  }))
}));

// Mock ErrorHandlerService
jest.mock('../../utils/ErrorHandlerService', () => ({
  ErrorHandlerService: jest.fn().mockImplementation(() => ({
    handleError: jest.fn(),
  }))
}));

describe('ProjectIdManager', () => {
  let projectIdManager: ProjectIdManager;
  let mockConfigService: jest.Mocked<ConfigService>;
  let mockQdrantConfigService: jest.Mocked<QdrantConfigService>;
  let mockNebulaConfigService: jest.Mocked<NebulaConfigService>;

  beforeEach(() => {
    // Clear all mocks before each test
    jest.clearAllMocks();

    // Create a new mock ConfigService instance for each test
    mockConfigService = {
      get: jest.fn().mockReturnValue({ mappingPath: './data/test-project-mapping.json' })
    } as unknown as jest.Mocked<ConfigService>;

    // Create a new mock QdrantConfigService instance for each test
    mockQdrantConfigService = {
      getCollectionNameForProject: jest.fn().mockImplementation((projectId: string) => `project-${projectId}`)
    } as unknown as jest.Mocked<QdrantConfigService>;

    // Create a new mock NebulaConfigService instance for each test
    mockNebulaConfigService = {
      getSpaceNameForProject: jest.fn().mockImplementation((projectId: string) => `project-${projectId}`)
    } as unknown as jest.Mocked<NebulaConfigService>;

    const mockLoggerService = {
      info: jest.fn(),
      warn: jest.fn(),
      error: jest.fn(),
    } as unknown as jest.Mocked<LoggerService>;

    const mockErrorHandlerService = {
      handleError: jest.fn(),
    } as unknown as jest.Mocked<ErrorHandlerService>;

    // Create ProjectIdManager instance with all required dependencies
    projectIdManager = new ProjectIdManager(
      mockConfigService,
      mockQdrantConfigService,
      mockNebulaConfigService,
      mockLoggerService,
      mockErrorHandlerService,
      {} as any // Mock SqliteProjectManager
    );
  });

  describe('generateProjectId', () => {
    it('should generate a project ID based on directory hash', async () => {
      const projectPath = '/test/project';
      const normalizedPath = '/test/project';
      const mockHash = 'abcdef1234567890';

      (HashUtils.normalizePath as jest.Mock).mockReturnValue(normalizedPath);
      (HashUtils.calculateDirectoryHash as jest.Mock).mockResolvedValue({ hash: mockHash });

      const projectId = await projectIdManager.generateProjectId(projectPath);

      expect(projectId).toBe(mockHash.substring(0, 16));
      expect(HashUtils.normalizePath).toHaveBeenCalledWith(projectPath);
      expect(HashUtils.calculateDirectoryHash).toHaveBeenCalledWith(projectPath);
    });
  });

  describe('getProjectId', () => {
    it('should return the project ID for a given path', () => {
      const projectPath = '/test/project';
      const normalizedPath = '/test/project';
      const projectId = 'abcdef1234567890';

      (HashUtils.normalizePath as jest.Mock).mockReturnValue(normalizedPath);
      // Set up the map with a test value
      (projectIdManager as any).projectIdMap.set(normalizedPath, projectId);

      const result = projectIdManager.getProjectId(projectPath);

      expect(result).toBe(projectId);
    });

    it('should return undefined if project ID does not exist', () => {
      const projectPath = '/test/project';
      const normalizedPath = '/test/project';

      (HashUtils.normalizePath as jest.Mock).mockReturnValue(normalizedPath);

      const result = projectIdManager.getProjectId(projectPath);

      expect(result).toBeUndefined();
    });
  });

  describe('getProjectPath', () => {
    it('should return the project path for a given project ID', () => {
      const projectId = 'abcdef1234567890';
      const projectPath = '/test/project';

      // Set up the map with a test value
      (projectIdManager as any).pathToProjectMap.set(projectId, projectPath);

      const result = projectIdManager.getProjectPath(projectId);

      expect(result).toBe(projectPath);
    });
  });

  describe('getCollectionName', () => {
    it('should return the collection name for a given project ID', () => {
      const projectId = 'abcdef1234567890';
      const collectionName = 'project-abcdef1234567890';

      // Set up the map with a test value
      (projectIdManager as any).collectionMap.set(projectId, collectionName);

      const result = projectIdManager.getCollectionName(projectId);

      expect(result).toBe(collectionName);
    });
  });

  describe('getSpaceName', () => {
    it('should return the space name for a given project ID', () => {
      const projectId = 'abcdef1234567890';
      const spaceName = 'project_abcdef1234567890';

      // Set up the map with a test value
      (projectIdManager as any).spaceMap.set(projectId, spaceName);

      const result = projectIdManager.getSpaceName(projectId);

      expect(result).toBe(spaceName);
    });
  });
});