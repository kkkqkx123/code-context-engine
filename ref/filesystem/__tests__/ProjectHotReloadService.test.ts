import { ProjectHotReloadService, ProjectHotReloadConfig, HotReloadStatus, HotReloadMetrics } from '../ProjectHotReloadService';
import { ChangeDetectionService, FileChangeEvent } from '../ChangeDetectionService';
import { HotReloadMonitoringService } from '../HotReloadMonitoringService';
import { HotReloadErrorPersistenceService } from '../HotReloadErrorPersistenceService';
import { HotReloadConfigService } from '../HotReloadConfigService';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { HotReloadError, HotReloadErrorCode } from '../HotReloadError';

// Mock dependencies
jest.mock('../../../utils/LoggerService');
jest.mock('../../../utils/ErrorHandlerService');
jest.mock('../ChangeDetectionService');
jest.mock('../HotReloadMonitoringService');
jest.mock('../HotReloadErrorPersistenceService');
jest.mock('../HotReloadConfigService');

describe('ProjectHotReloadService', () => {
  let projectHotReloadService: ProjectHotReloadService;
  let mockChangeDetectionService: jest.Mocked<ChangeDetectionService>;
  let mockMonitoringService: jest.Mocked<HotReloadMonitoringService>;
  let mockErrorPersistenceService: jest.Mocked<HotReloadErrorPersistenceService>;
  let mockConfigService: jest.Mocked<HotReloadConfigService>;
  let mockLogger: jest.Mocked<LoggerService>;
  let mockErrorHandler: jest.Mocked<ErrorHandlerService>;

  beforeEach(() => {
    // Create mock services
    mockChangeDetectionService = {
      setCallbacks: jest.fn(),
      initialize: jest.fn(),
      isServiceRunning: jest.fn().mockReturnValue(true),
    } as any;
    mockMonitoringService = new HotReloadMonitoringService({} as any, {} as any) as jest.Mocked<HotReloadMonitoringService>;
    mockErrorPersistenceService = new HotReloadErrorPersistenceService({} as any, {} as any) as jest.Mocked<HotReloadErrorPersistenceService>;
    mockConfigService = new HotReloadConfigService({} as any, {} as any) as jest.Mocked<HotReloadConfigService>;
    
    mockLogger = new LoggerService() as jest.Mocked<LoggerService>;
    mockLogger.info = jest.fn();
    mockLogger.error = jest.fn();
    mockLogger.warn = jest.fn();
    mockLogger.debug = jest.fn();

    mockErrorHandler = new ErrorHandlerService(mockLogger) as jest.Mocked<ErrorHandlerService>;
    mockErrorHandler.handleError = jest.fn();

    projectHotReloadService = new ProjectHotReloadService(
      mockChangeDetectionService,
      mockMonitoringService,
      mockErrorPersistenceService,
      mockConfigService,
      mockLogger,
      mockErrorHandler
    );

    // Mock config service to return default config
    mockConfigService.getGlobalConfig.mockReturnValue({
      enabled: true,
      defaultDebounceInterval: 500,
      defaultMaxFileSize: 512000,
      maxConcurrentProjects: 5,
      enableDetailedLogging: false,
      defaultErrorHandling: {
        maxRetries: 3,
        alertThreshold: 5,
        autoRecovery: true
      },
      defaultWatchPatterns: ['**/*.{js,ts,jsx,tsx,json,md}'],
      defaultIgnorePatterns: ['**/node_modules/**', '**/.git/**']
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
 });

  describe('enableForProject', () => {
    it('should enable hot reload for a project', async () => {
      const projectPath = '/test/project';
      const config: Partial<ProjectHotReloadConfig> = {
        enabled: true,
        debounceInterval: 1000
      };

      mockChangeDetectionService.initialize.mockResolvedValue(undefined);
      mockMonitoringService.getProjectMetrics.mockReturnValue({
        filesProcessed: 0,
        changesDetected: 0,
        averageProcessingTime: 0,
        lastUpdated: new Date(),
        errorCount: 0,
        errorBreakdown: {},
        recoveryStats: {
          autoRecovered: 0,
          manualIntervention: 0,
          failedRecoveries: 0
        }
      });

      await projectHotReloadService.enableForProject(projectPath, config);

      expect(mockChangeDetectionService.initialize).toHaveBeenCalledWith([projectPath], {
        watchPaths: [projectPath],
        debounceInterval: 1000,
        enableHashComparison: true,
        trackFileHistory: true,
      });

      const projectConfig = projectHotReloadService.getProjectConfig(projectPath);
      expect(projectConfig.enabled).toBe(true);
      expect(projectConfig.debounceInterval).toBe(1000);

      const projectStatus = projectHotReloadService.getProjectStatus(projectPath);
      expect(projectStatus.enabled).toBe(true);
      expect(projectStatus.isWatching).toBe(true);
    });

    it('should handle error when enabling hot reload', async () => {
      const projectPath = '/test/project';
      mockChangeDetectionService.initialize.mockRejectedValue(new Error('Initialization failed'));

      await expect(projectHotReloadService.enableForProject(projectPath)).rejects.toThrow(HotReloadError);
      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(HotReloadError),
        expect.objectContaining({
          component: 'ProjectHotReloadService',
          operation: 'enableForProject',
          projectPath
        })
      );
    });
  });

  describe('disableForProject', () => {
    it('should disable hot reload for a project', async () => {
      const projectPath = '/test/project';
      
      // First enable the project
      await projectHotReloadService.enableForProject(projectPath, { enabled: true });
      
      // Then disable it
      await projectHotReloadService.disableForProject(projectPath);

      const projectConfig = projectHotReloadService.getProjectConfig(projectPath);
      expect(projectConfig.enabled).toBe(false);

      const projectStatus = projectHotReloadService.getProjectStatus(projectPath);
      expect(projectStatus.enabled).toBe(false);
      expect(projectStatus.isWatching).toBe(false);
    });


  });

  describe('getProjectConfig', () => {
    it('should return default config for non-existent project', () => {
      const projectPath = '/nonexistent/project';
      const config = projectHotReloadService.getProjectConfig(projectPath);

      expect(config.enabled).toBe(false); // Default for non-existent projects
      expect(config.debounceInterval).toBe(500); // Default value
      expect(config.watchPatterns).toBeDefined();
      expect(config.ignorePatterns).toBeDefined();
    });

    it('should return existing config for project', async () => {
      const projectPath = '/test/project';
      const expectedConfig: ProjectHotReloadConfig = {
        enabled: true,
        debounceInterval: 1000,
        watchPatterns: ['**/*.ts'],
        ignorePatterns: ['**/node_modules/**'],
        maxFileSize: 5 * 1024 * 1024,
        errorHandling: {
          maxRetries: 5,
          alertThreshold: 10,
          autoRecovery: false
        }
      };

      await projectHotReloadService.enableForProject(projectPath, expectedConfig);

      const config = projectHotReloadService.getProjectConfig(projectPath);
      expect(config.debounceInterval).toBe(1000);
      expect(config.watchPatterns).toEqual(expectedConfig.watchPatterns);
      expect(config.ignorePatterns).toEqual(expectedConfig.ignorePatterns);
    });
  });

  describe('getProjectStatus', () => {
    it('should return default status for non-existent project', () => {
      const projectPath = '/nonexistent/project';
      const status = projectHotReloadService.getProjectStatus(projectPath);

      expect(status.enabled).toBe(false);
      expect(status.isWatching).toBe(false);
      expect(status.watchedPaths).toEqual([]);
      expect(status.changesCount).toBe(0);
      expect(status.errorsCount).toBe(0);
    });

    it('should return existing status for project', async () => {
      const projectPath = '/test/project';
      await projectHotReloadService.enableForProject(projectPath, { enabled: true });

      const status = projectHotReloadService.getProjectStatus(projectPath);
      expect(status.enabled).toBe(true);
      expect(status.isWatching).toBe(true);
    });
  });

 describe('getProjectMetrics', () => {
    it('should return default metrics for non-existent project', () => {
      const projectPath = '/nonexistent/project';
      const metrics = projectHotReloadService.getProjectMetrics(projectPath);

      expect(metrics.filesProcessed).toBe(0);
      expect(metrics.changesDetected).toBe(0);
      expect(metrics.averageProcessingTime).toBe(0);
      expect(metrics.errorCount).toBe(0);
      expect(metrics.errorBreakdown).toEqual({});
      expect(metrics.recoveryStats).toEqual({
        autoRecovered: 0,
        manualIntervention: 0,
        failedRecoveries: 0
      });
    });

    it('should return existing metrics for project', async () => {
      const projectPath = '/test/project';
      await projectHotReloadService.enableForProject(projectPath, { enabled: true });

      const metrics = projectHotReloadService.getProjectMetrics(projectPath);
      expect(metrics.filesProcessed).toBe(0); // Default value
    });
  });

  describe('updateConfig', () => {
    it('should update project configuration', async () => {
      const projectPath = '/test/project';
      const newConfig: Partial<ProjectHotReloadConfig> = {
        debounceInterval: 2000,
        watchPatterns: ['**/*.js']
      };

      // First enable the project
      await projectHotReloadService.enableForProject(projectPath, { enabled: true });

      await projectHotReloadService.updateConfig(projectPath, newConfig);

      const config = projectHotReloadService.getProjectConfig(projectPath);
      expect(config.debounceInterval).toBe(2000);
      expect(config.watchPatterns).toEqual(['**/*.js']);
    });
  });

  describe('isProjectHotReloadEnabled', () => {
    it('should return true if project hot reload is enabled', async () => {
      const projectPath = '/test/project';
      await projectHotReloadService.enableForProject(projectPath, { enabled: true });

      const isEnabled = projectHotReloadService.isProjectHotReloadEnabled(projectPath);
      expect(isEnabled).toBe(true);
    });

    it('should return false if project hot reload is disabled', () => {
      const projectPath = '/test/project';
      const isEnabled = projectHotReloadService.isProjectHotReloadEnabled(projectPath);
      expect(isEnabled).toBe(false);
    });
  });

  describe('handleFileChange', () => {
    it('should handle file change event', async () => {
      const projectPath = '/test/project';
      const fileChangeEvent: FileChangeEvent = {
      type: 'modified',
      path: '/test/project/file.ts',
      relativePath: 'file.ts',
      timestamp: new Date()
      };

      // Enable the project first
      await projectHotReloadService.enableForProject(projectPath, { enabled: true });

      // Mock the internal method to get project path from event
      const getProjectPathFromEventSpy = jest.spyOn(projectHotReloadService as any, 'getProjectPathFromEvent')
        .mockReturnValue(projectPath);

      // Mock monitoring service
      mockMonitoringService.recordChangeDetected.mockImplementation(() => {});

      // Access the private method through any type
      await (projectHotReloadService as any).handleFileChange(fileChangeEvent);

      expect(getProjectPathFromEventSpy).toHaveBeenCalledWith(fileChangeEvent);
      expect(mockMonitoringService.recordChangeDetected).toHaveBeenCalledWith(projectPath);

      // Verify the status was updated
      const status = projectHotReloadService.getProjectStatus(projectPath);
      expect(status.changesCount).toBe(1);
    });
 });

  describe('handleError', () => {
    it('should handle HotReloadError', () => {
      const error = new HotReloadError(
        HotReloadErrorCode.CHANGE_DETECTION_FAILED,
        'Test error message'
      );

      // Access the private method through any type
      (projectHotReloadService as any).handleError(error);

      expect(mockLogger.error).toHaveBeenCalledWith('Hot reload error occurred', error);
      expect(mockErrorPersistenceService.queueError).toHaveBeenCalledWith(
        expect.objectContaining({
          errorCode: HotReloadErrorCode.CHANGE_DETECTION_FAILED,
          message: 'Test error message'
        })
      );
    });

    it('should handle regular Error', () => {
      const error = new Error('Regular error');

      // Access the private method through any type
      (projectHotReloadService as any).handleError(error);

      expect(mockLogger.error).toHaveBeenCalledWith('Hot reload error occurred', error);
    });
  });

  describe('getAllProjectStatuses', () => {
    it('should return all project statuses', async () => {
      const project1 = '/project1';
      const project2 = '/project2';

      await projectHotReloadService.enableForProject(project1, { enabled: true });
      await projectHotReloadService.enableForProject(project2, { enabled: true });

      const allStatuses = projectHotReloadService.getAllProjectStatuses();
      expect(allStatuses.size).toBeGreaterThanOrEqual(2);
      expect(allStatuses.get(project1)).toBeDefined();
      expect(allStatuses.get(project2)).toBeDefined();
    });
  });

  describe('setupChangeDetectionCallbacks', () => {
    it('should set up change detection callbacks', () => {
      // Verify that the callbacks were set up during construction
      expect(mockChangeDetectionService.setCallbacks).toHaveBeenCalledWith(
        expect.objectContaining({
          onFileCreated: expect.any(Function),
          onFileModified: expect.any(Function),
          onFileDeleted: expect.any(Function),
          onError: expect.any(Function)
        })
      );
    });
  });
});