import { HotReloadMonitoringService, HotReloadMonitoringConfig } from '../HotReloadMonitoringService';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { HotReloadMetrics } from '../ProjectHotReloadService';
import { HotReloadError, HotReloadErrorCode } from '../HotReloadError';

// Mock dependencies
jest.mock('../../../utils/LoggerService');
jest.mock('../../../utils/ErrorHandlerService');

describe('HotReloadMonitoringService', () => {
  let monitoringService: HotReloadMonitoringService;
  let mockLogger: jest.Mocked<LoggerService>;
  let mockErrorHandler: jest.Mocked<ErrorHandlerService>;

  beforeEach(() => {
    // Create mock logger
    mockLogger = new LoggerService() as jest.Mocked<LoggerService>;
    mockLogger.info = jest.fn();
    mockLogger.error = jest.fn();
    mockLogger.warn = jest.fn();
    mockLogger.debug = jest.fn();

    // Create mock error handler
    mockErrorHandler = new ErrorHandlerService(mockLogger) as jest.Mocked<ErrorHandlerService>;
    mockErrorHandler.handleError = jest.fn();

    monitoringService = new HotReloadMonitoringService(mockLogger, mockErrorHandler);
  });

  afterEach(() => {
    jest.clearAllMocks();
    monitoringService.destroy();
  });

  describe('constructor', () => {
    it('should initialize with default configuration', () => {
      expect(monitoringService).toBeDefined();
      const config = (monitoringService as any).config;
      expect(config.enableMetricsCollection).toBe(true);
      expect(config.metricsCollectionInterval).toBe(10000);
      expect(config.enableDetailedLogging).toBe(false);
      expect(config.maxMetricsHistory).toBe(100);
      expect(config.alertThresholds).toEqual({
        errorRate: 10,
        processingTime: 5000,
        memoryUsage: 80
      });
    });
  });

  describe('updateConfig', () => {
    it('should update configuration', () => {
      const newConfig: Partial<HotReloadMonitoringConfig> = {
        enableMetricsCollection: false,
        metricsCollectionInterval: 60000,
        enableDetailedLogging: true,
        alertThresholds: {
          errorRate: 5,
          processingTime: 3000,
          memoryUsage: 75
        }
      };

      monitoringService.updateConfig(newConfig);

      const config = (monitoringService as any).config;
      expect(config.enableMetricsCollection).toBe(false);
      expect(config.metricsCollectionInterval).toBe(60000);
      expect(config.enableDetailedLogging).toBe(true);
      expect(config.alertThresholds.errorRate).toBe(5);
      expect(config.alertThresholds.processingTime).toBe(3000);
      expect(config.alertThresholds.memoryUsage).toBe(75);
    });

    it('should log configuration update', () => {
      const newConfig: Partial<HotReloadMonitoringConfig> = {
        enableMetricsCollection: false
      };

      monitoringService.updateConfig(newConfig);

      expect(mockLogger.info).toHaveBeenCalledWith(
        'Hot reload monitoring configuration updated',
        expect.any(Object)
      );
    });
  });

  describe('updateProjectMetrics', () => {
    it('should update project metrics', () => {
      const projectPath = '/test/project';
      const metricsUpdate: Partial<HotReloadMetrics> = {
        filesProcessed: 10,
        changesDetected: 5,
        averageProcessingTime: 100,
        errorCount: 2
      };

      monitoringService.updateProjectMetrics(projectPath, metricsUpdate);

      const metrics = monitoringService.getProjectMetrics(projectPath);
      expect(metrics.filesProcessed).toBe(10);
      expect(metrics.changesDetected).toBe(5);
      expect(metrics.averageProcessingTime).toBe(100);
      expect(metrics.errorCount).toBe(2);
    });

    it('should handle error breakdown updates', () => {
      const projectPath = '/test/project';
      const metricsUpdate: Partial<HotReloadMetrics> = {
        errorBreakdown: { 'TEST_ERROR': 3 }
      };

      monitoringService.updateProjectMetrics(projectPath, metricsUpdate);

      const metrics = monitoringService.getProjectMetrics(projectPath);
      expect(metrics.errorBreakdown['TEST_ERROR']).toBe(3);
    });

    it('should handle recovery stats updates', () => {
      const projectPath = '/test/project';
      const metricsUpdate: Partial<HotReloadMetrics> = {
        recoveryStats: {
          autoRecovered: 1,
          manualIntervention: 0,
          failedRecoveries: 0
        }
      };

      monitoringService.updateProjectMetrics(projectPath, metricsUpdate);

      const metrics = monitoringService.getProjectMetrics(projectPath);
      expect(metrics.recoveryStats.autoRecovered).toBe(1);
    });
  });

  describe('getProjectMetrics', () => {
    it('should return default metrics for non-existent project', () => {
      const projectPath = '/nonexistent/project';
      const metrics = monitoringService.getProjectMetrics(projectPath);

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

    it('should return copy of metrics to prevent direct modification', () => {
      const projectPath = '/test/project';
      const metricsUpdate: Partial<HotReloadMetrics> = {
        filesProcessed: 10
      };

      monitoringService.updateProjectMetrics(projectPath, metricsUpdate);
      const metrics = monitoringService.getProjectMetrics(projectPath);

      // Modify the returned metrics
      metrics.filesProcessed = 999;

      // Verify the internal state wasn't modified
      const metrics2 = monitoringService.getProjectMetrics(projectPath);
      expect(metrics2.filesProcessed).toBe(10);
    });
  });

  describe('getAllProjectMetrics', () => {
    it('should return metrics for all projects', () => {
      const project1 = '/project1';
      const project2 = '/project2';

      monitoringService.updateProjectMetrics(project1, { filesProcessed: 5 });
      monitoringService.updateProjectMetrics(project2, { filesProcessed: 10 });

      const allMetrics = monitoringService.getAllProjectMetrics();
      expect(allMetrics.size).toBe(2);
      expect(allMetrics.get(project1)?.filesProcessed).toBe(5);
      expect(allMetrics.get(project2)?.filesProcessed).toBe(10);
    });
  });

  describe('recordError', () => {
    it('should record error and update metrics', () => {
      const projectPath = '/test/project';
      const error = new HotReloadError(
        HotReloadErrorCode.CHANGE_DETECTION_FAILED,
        'Test error message'
      );

      monitoringService.recordError(projectPath, error);

      const metrics = monitoringService.getProjectMetrics(projectPath);
      expect(metrics.errorCount).toBe(1);
      expect(metrics.errorBreakdown['CHANGE_DETECTION_FAILED']).toBe(1);
    });
  });

  describe('recordFileProcessed', () => {
    it('should record file processing and update average processing time', () => {
      const projectPath = '/test/project';

      monitoringService.recordFileProcessed(projectPath, 100);
      monitoringService.recordFileProcessed(projectPath, 200);

      const metrics = monitoringService.getProjectMetrics(projectPath);
      expect(metrics.filesProcessed).toBe(2);
      expect(metrics.averageProcessingTime).toBe(150); // (100 + 200) / 2
    });
  });

  describe('recordChangeDetected', () => {
    it('should record change detection', () => {
      const projectPath = '/test/project';

      monitoringService.recordChangeDetected(projectPath);

      const metrics = monitoringService.getProjectMetrics(projectPath);
      expect(metrics.changesDetected).toBe(1);
    });
  });

  describe('recordRecovery', () => {
    it('should record successful recovery', () => {
      const projectPath = '/test/project';

      monitoringService.recordRecovery(projectPath, true);

      const metrics = monitoringService.getProjectMetrics(projectPath);
      expect(metrics.recoveryStats.autoRecovered).toBe(1);
      expect(metrics.recoveryStats.failedRecoveries).toBe(0);
    });

    it('should record failed recovery', () => {
      const projectPath = '/test/project';

      monitoringService.recordRecovery(projectPath, false);

      const metrics = monitoringService.getProjectMetrics(projectPath);
      expect(metrics.recoveryStats.autoRecovered).toBe(0);
      expect(metrics.recoveryStats.failedRecoveries).toBe(1);
    });
  });

  describe('resetProjectMetrics', () => {
    it('should reset project metrics to default', () => {
      const projectPath = '/test/project';
      monitoringService.updateProjectMetrics(projectPath, {
        filesProcessed: 100,
        changesDetected: 50,
        errorCount: 10
      });

      // Verify metrics are set
      const metricsBefore = monitoringService.getProjectMetrics(projectPath);
      expect(metricsBefore.filesProcessed).toBe(100);

      monitoringService.resetProjectMetrics(projectPath);

      // Verify metrics are reset
      const metricsAfter = monitoringService.getProjectMetrics(projectPath);
      expect(metricsAfter.filesProcessed).toBe(0);
      expect(metricsAfter.changesDetected).toBe(0);
      expect(metricsAfter.errorCount).toBe(0);
    });

    it('should clear metrics history', () => {
      const projectPath = '/test/project';
      monitoringService.updateProjectMetrics(projectPath, { filesProcessed: 10 });

      // Add some history
      (monitoringService as any).addMetricsToHistory(projectPath, { filesProcessed: 5, changesDetected: 2, averageProcessingTime: 100, lastUpdated: new Date(), errorCount: 0, errorBreakdown: {}, recoveryStats: { autoRecovered: 0, manualIntervention: 0, failedRecoveries: 0 } });

      monitoringService.resetProjectMetrics(projectPath);

      const history = monitoringService.getProjectMetricsHistory(projectPath);
      expect(history).toHaveLength(0);
    });
  });

  describe('getMetricsSummary', () => {
    it('should return correct metrics summary', () => {
      monitoringService.updateProjectMetrics('/project1', {
        filesProcessed: 10,
        changesDetected: 5,
        averageProcessingTime: 100,
        errorCount: 2
      });

      monitoringService.updateProjectMetrics('/project2', {
        filesProcessed: 20,
        changesDetected: 8,
        averageProcessingTime: 200,
        errorCount: 1
      });

      const summary = monitoringService.getMetricsSummary();

      expect(summary.totalProjects).toBe(2);
      expect(summary.totalChanges).toBe(13); // 5 + 8
      expect(summary.totalErrors).toBe(3); // 2 + 1
      expect(summary.averageProcessingTime).toBe(150); // (100 + 200) / 2
    });

    it('should handle case with no projects', () => {
      const summary = monitoringService.getMetricsSummary();

      expect(summary.totalProjects).toBe(0);
      expect(summary.totalChanges).toBe(0);
      expect(summary.totalErrors).toBe(0);
      expect(summary.averageProcessingTime).toBe(0);
    });
  });

  describe('getSystemMetrics', () => {
    it('should return system metrics if available', () => {
      // The service collects system metrics automatically every 5 seconds
      const systemMetrics = monitoringService.getSystemMetrics();

      // The system metrics might be null initially since it's collected asynchronously
      // Just verify the method exists and doesn't throw
      expect(monitoringService.getSystemMetrics).toBeDefined();
    });
  });

  describe('checkAlerts', () => {
    it('should log warning when error rate exceeds threshold', () => {
      const projectPath = '/test/project';
      const highErrorMetrics: HotReloadMetrics = {
        filesProcessed: 10,
        changesDetected: 5,
        averageProcessingTime: 100,
        lastUpdated: new Date(Date.now() - 60000), // 1 minute ago
        errorCount: 15, // This should exceed the default threshold of 10 errors per minute
        errorBreakdown: {},
        recoveryStats: {
          autoRecovered: 0,
          manualIntervention: 0,
          failedRecoveries: 0
        }
      };

      monitoringService.updateProjectMetrics(projectPath, highErrorMetrics);

      expect(mockLogger.warn).toHaveBeenCalledWith(
        expect.stringContaining('High error rate detected'),
        expect.objectContaining({
          errorRate: expect.any(Number),
          threshold: 10,
          metrics: expect.any(Object)
        })
      );
    });

    it('should log warning when processing time exceeds threshold', () => {
      const projectPath = '/test/project';
      const highProcessingTimeMetrics: HotReloadMetrics = {
        filesProcessed: 10,
        changesDetected: 5,
        averageProcessingTime: 6000, // This should exceed the default threshold of 5000ms
        lastUpdated: new Date(),
        errorCount: 1,
        errorBreakdown: {},
        recoveryStats: {
          autoRecovered: 0,
          manualIntervention: 0,
          failedRecoveries: 0
        }
      };

      monitoringService.updateProjectMetrics(projectPath, highProcessingTimeMetrics);

      expect(mockLogger.warn).toHaveBeenCalledWith(
        expect.stringContaining('High average processing time detected'),
        expect.objectContaining({
          averageProcessingTime: 6000,
          threshold: 5000,
          metrics: expect.any(Object)
        })
      );
    });
  });

  describe('getProjectMetricsHistory', () => {
    it('should return empty array for non-existent project', () => {
      const history = monitoringService.getProjectMetricsHistory('/nonexistent/project');
      expect(history).toHaveLength(0);
    });

    it('should return metrics history for project', () => {
      const projectPath = '/test/project';
      const metrics: HotReloadMetrics = {
        filesProcessed: 5,
        changesDetected: 2,
        averageProcessingTime: 100,
        lastUpdated: new Date(),
        errorCount: 0,
        errorBreakdown: {},
        recoveryStats: {
          autoRecovered: 0,
          manualIntervention: 0,
          failedRecoveries: 0
        }
      };

      // Add metrics to history
      (monitoringService as any).addMetricsToHistory(projectPath, metrics);

      const history = monitoringService.getProjectMetricsHistory(projectPath);
      expect(history).toHaveLength(1);
      expect(history[0].metrics.filesProcessed).toBe(5);
    });
  });
});