import { HotReloadErrorPersistenceService, ErrorPersistenceConfig } from '../HotReloadErrorPersistenceService';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { HotReloadErrorReport } from '../types/HotReloadTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

// Mock dependencies
jest.mock('../../../utils/LoggerService');
jest.mock('../../../utils/ErrorHandlerService');
jest.mock('fs/promises');

describe('HotReloadErrorPersistenceService', () => {
  let errorPersistenceService: HotReloadErrorPersistenceService;
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

    errorPersistenceService = new HotReloadErrorPersistenceService(mockLogger, mockErrorHandler);
  });

  afterEach(() => {
    jest.clearAllMocks();
    errorPersistenceService.destroy();
  });

  describe('constructor', () => {
    it('should initialize with default configuration', () => {
      expect(errorPersistenceService).toBeDefined();
      const config = (errorPersistenceService as any).config;
      expect(config.enabled).toBe(true);
      expect(config.storagePath).toBe('./logs');
      expect(config.maxFileSize).toBe(512000); // 500KB
      expect(config.maxFiles).toBe(10);
      expect(config.flushInterval).toBe(5000); // 5 seconds
      expect(config.enableCompression).toBe(false);
    });
  });

  describe('updateConfig', () => {
    it('should update configuration', async () => {
      const newConfig: Partial<ErrorPersistenceConfig> = {
        enabled: false,
        storagePath: './custom-logs',
        maxFileSize: 5 * 1024 * 1024, // 5MB
        flushInterval: 10000, // 10 seconds
        enableCompression: true
      };

      await errorPersistenceService.updateConfig(newConfig);

      const config = (errorPersistenceService as any).config;
      expect(config.enabled).toBe(false);
      expect(config.storagePath).toBe('./custom-logs');
      expect(config.maxFileSize).toBe(5 * 1024 * 1024);
      expect(config.flushInterval).toBe(10000);
      expect(config.enableCompression).toBe(true);
    });

    it('should ensure storage path exists', async () => {
      const newConfig: Partial<ErrorPersistenceConfig> = {
        storagePath: './new-logs'
      };

      (fs.mkdir as jest.Mock).mockResolvedValue(undefined);

      await errorPersistenceService.updateConfig(newConfig);

      expect(fs.mkdir).toHaveBeenCalledWith('./new-logs', { recursive: true });
    });

    it('should handle error when creating storage path', async () => {
      const newConfig: Partial<ErrorPersistenceConfig> = {
        storagePath: './new-logs'
      };

      (fs.mkdir as jest.Mock).mockRejectedValue(new Error('Permission denied'));

      await errorPersistenceService.updateConfig(newConfig);

      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        expect.objectContaining({
          component: 'HotReloadErrorPersistenceService',
          operation: 'ensureStoragePath',
          path: './new-logs'
        })
      );
    });
  });

  describe('queueError', () => {
    it('should add error to queue', async () => {
      const errorReport: HotReloadErrorReport = {
        id: 'error-1',
        timestamp: new Date(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      };

      await errorPersistenceService.queueError(errorReport);

      const queue = (errorPersistenceService as any).errorQueue;
      expect(queue).toHaveLength(1);
      expect(queue[0]).toEqual(errorReport);
    });

    it('should not add error to queue if persistence is disabled', async () => {
      await errorPersistenceService.updateConfig({ enabled: false });

      const errorReport: HotReloadErrorReport = {
        id: 'error-1',
        timestamp: new Date(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      };

      await errorPersistenceService.queueError(errorReport);

      const queue = (errorPersistenceService as any).errorQueue;
      expect(queue).toHaveLength(0);
    });

    it('should flush immediately if queue size reaches threshold', async () => {
      const flushSpy = jest.spyOn(errorPersistenceService as any, 'flushErrors').mockResolvedValue(undefined);
      const errorReport: HotReloadErrorReport = {
        id: 'error-1',
        timestamp: new Date(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      };

      // Add 100 errors to trigger immediate flush
      for (let i = 0; i < 100; i++) {
        await errorPersistenceService.queueError(errorReport);
      }

      expect(flushSpy).toHaveBeenCalled();
    });
  });

  describe('persistError', () => {
    it('should immediately persist error', async () => {
      const errorReport: HotReloadErrorReport = {
        id: 'error-1',
        timestamp: new Date(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      };

      const flushSpy = jest.spyOn(errorPersistenceService as any, 'flushErrors').mockResolvedValue(undefined);

      await errorPersistenceService.persistError(errorReport);

      const queue = (errorPersistenceService as any).errorQueue;
      expect(queue).toHaveLength(1);
      expect(flushSpy).toHaveBeenCalled();
    });

    it('should handle error during immediate persistence', async () => {
      const errorReport: HotReloadErrorReport = {
        id: 'error-1',
        timestamp: new Date(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      };

      await errorPersistenceService.updateConfig({ enabled: false });

      await errorPersistenceService.persistError(errorReport);

      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        expect.objectContaining({
          component: 'HotReloadErrorPersistenceService',
          operation: 'persistError',
          errorId: 'error-1'
        })
      );
    });
  });

  describe('flushErrors', () => {
    it('should flush errors to file', async () => {
      const errorReport: HotReloadErrorReport = {
        id: 'error-1',
        timestamp: new Date(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      };

      await errorPersistenceService.queueError(errorReport);

      (fs.mkdir as jest.Mock).mockResolvedValue(undefined);
      (fs.appendFile as jest.Mock).mockResolvedValue(undefined);

      await (errorPersistenceService as any).flushErrors();

      const queue = (errorPersistenceService as any).errorQueue;
      expect(queue).toHaveLength(0); // Queue should be empty after flush
      expect(fs.appendFile).toHaveBeenCalledWith(
        expect.stringContaining('hotreload-errors.jsonl'),
        expect.stringContaining('error-1'),
        'utf-8'
      );
    });

    it('should not flush if queue is empty', async () => {
      const appendFileSpy = jest.spyOn(fs, 'appendFile').mockResolvedValue(undefined as any);

      await (errorPersistenceService as any).flushErrors();

      expect(appendFileSpy).not.toHaveBeenCalled();
    });

    it('should not flush if persistence is disabled', async () => {
      await errorPersistenceService.updateConfig({ enabled: false });

      const errorReport: HotReloadErrorReport = {
        id: 'error-1',
        timestamp: new Date(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      };

      await errorPersistenceService.queueError(errorReport);

      const appendFileSpy = jest.spyOn(fs, 'appendFile').mockResolvedValue(undefined as any);

      await (errorPersistenceService as any).flushErrors();

      expect(appendFileSpy).not.toHaveBeenCalled();
    });

    it('should handle errors during flush', async () => {
      const errorReport: HotReloadErrorReport = {
        id: 'error-1',
        timestamp: new Date(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      };

      await errorPersistenceService.queueError(errorReport);

      (fs.mkdir as jest.Mock).mockResolvedValue(undefined);
      (fs.appendFile as jest.Mock).mockRejectedValue(new Error('Write failed'));

      await (errorPersistenceService as any).flushErrors();

      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        expect.objectContaining({
          component: 'HotReloadErrorPersistenceService',
          operation: 'flushErrors'
        })
      );
    });
  });

  describe('rotateLogFileIfNeeded', () => {
    it('should rotate log file when it exceeds max size', async () => {
      // Update config to set maxFileSize to 10MB for this test
      await errorPersistenceService.updateConfig({ maxFileSize: 10 * 1024 * 1024 });

      const logFilePath = './logs/hotreload-errors.jsonl';

      (fs.stat as jest.Mock).mockResolvedValue({
        size: 11 * 1024 * 1024 // 11MB, exceeding 10MB limit
      });
      (fs.rename as jest.Mock).mockResolvedValue(undefined);
      (fs.readdir as jest.Mock).mockResolvedValue([]);

      await (errorPersistenceService as any).rotateLogFileIfNeeded(logFilePath);

      expect(fs.rename).toHaveBeenCalledWith(
        logFilePath,
        expect.stringMatching(/hotreload-errors-\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}-\d{3}\.jsonl/)
      );
    });

    it('should not rotate log file when it is within size limit', async () => {
      // Update config to set maxFileSize to 10MB for this test
      await errorPersistenceService.updateConfig({ maxFileSize: 10 * 1024 * 1024 });

      const logFilePath = './logs/hotreload-errors.jsonl';

      (fs.stat as jest.Mock).mockResolvedValue({
        size: 5 * 1024 * 1024 // 5MB, within 10MB limit
      });

      await (errorPersistenceService as any).rotateLogFileIfNeeded(logFilePath);

      expect(fs.rename).not.toHaveBeenCalled();
    });

    it('should handle errors during log rotation', async () => {
      const logFilePath = './logs/hotreload-errors.jsonl';

      (fs.stat as jest.Mock).mockRejectedValue(new Error('Stat failed'));

      await (errorPersistenceService as any).rotateLogFileIfNeeded(logFilePath);

      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        expect.objectContaining({
          component: 'HotReloadErrorPersistenceService',
          operation: 'rotateLogFileIfNeeded',
          logFilePath
        })
      );
    });
  });

  describe('cleanupOldArchives', () => {
    it('should clean up old archive files', async () => {
      // Update config to set maxFiles to 2 for this test to ensure 1 file gets deleted
      await errorPersistenceService.updateConfig({ maxFiles: 2 });

      (fs.readdir as jest.Mock).mockResolvedValue([
        'hotreload-errors-2023-01-01_10-00-00-000.jsonl',
        'hotreload-errors-2023-01-02_10-00-00-000.jsonl',
        'hotreload-errors-2023-01-03_10-00-00-000.jsonl',
        'other-file.txt'
      ]);
      (fs.unlink as jest.Mock).mockResolvedValue(undefined);

      await (errorPersistenceService as any).cleanupOldArchives();

      expect(fs.readdir).toHaveBeenCalledWith('./logs');
      expect(fs.unlink).toHaveBeenCalledTimes(1); // Only 1 file should be deleted if maxFiles is 2 and we have 3
    });

    it('should handle errors during cleanup', async () => {
      (fs.readdir as jest.Mock).mockRejectedValue(new Error('Read failed'));

      await (errorPersistenceService as any).cleanupOldArchives();

      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        expect.objectContaining({
          component: 'HotReloadErrorPersistenceService',
          operation: 'cleanupOldArchives'
        })
      );
    });
  });

  describe('readErrorLogs', () => {
    it('should read error logs from current file', async () => {
      const logContent = JSON.stringify({
        id: 'error-1',
        timestamp: new Date().toISOString(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      }) + '\n';

      (fs.readFile as jest.Mock).mockResolvedValue(logContent);

      const logs = await errorPersistenceService.readErrorLogs();

      expect(logs).toHaveLength(1);
      expect(logs[0].id).toBe('error-1');
    });

    it('should handle missing log file by reading from archives', async () => {
      (fs.readFile as jest.Mock).mockRejectedValue({ code: 'ENOENT' });
      (fs.readdir as jest.Mock).mockResolvedValue([
        'hotreload-errors-2023-01-01_10-00-00-000.jsonl'
      ]);
      const archiveContent = JSON.stringify({
        id: 'error-2',
        timestamp: new Date().toISOString(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      }) + '\n';
      (fs.readFile as jest.Mock).mockResolvedValueOnce(archiveContent);

      const logs = await errorPersistenceService.readErrorLogs();

      expect(logs).toHaveLength(1);
      expect(logs[0].id).toBe('error-2');
    });

    it('should limit results when limit is specified', async () => {
      const logContent = 
        JSON.stringify({
          id: 'error-1',
          timestamp: new Date().toISOString(),
          projectId: 'test-project',
          errorCode: 'TEST_ERROR',
          message: 'Test error message',
          stack: 'Error stack',
          context: {},
          resolved: false
        }) + '\n' +
        JSON.stringify({
          id: 'error-2',
          timestamp: new Date().toISOString(),
          projectId: 'test-project',
          errorCode: 'TEST_ERROR',
          message: 'Test error message 2',
          stack: 'Error stack 2',
          context: {},
          resolved: false
        }) + '\n';

      (fs.readFile as jest.Mock).mockResolvedValue(logContent);

      const logs = await errorPersistenceService.readErrorLogs(1);

      expect(logs).toHaveLength(1);
      expect(logs[0].id).toBe('error-2'); // Should return the last error
    });

    it('should handle errors when reading logs', async () => {
      (fs.readFile as jest.Mock).mockRejectedValue(new Error('Read failed'));

      const logs = await errorPersistenceService.readErrorLogs();

      expect(logs).toHaveLength(0);
      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        expect.objectContaining({
          component: 'HotReloadErrorPersistenceService',
          operation: 'readErrorLogs'
        })
      );
    });
  });

 describe('getErrorStats', () => {
   it('should return error statistics', async () => {
     const logContent = JSON.stringify({
       id: 'error-1',
       timestamp: new Date().toISOString(),
       projectId: 'test-project',
       errorCode: 'TEST_ERROR',
       message: 'Test error message',
       stack: 'Error stack',
       context: {},
       resolved: false
     }) + '\n';

     (fs.stat as jest.Mock).mockResolvedValue({ size: 100 });
     (fs.readFile as jest.Mock).mockResolvedValue(logContent);
     (fs.readdir as jest.Mock).mockResolvedValue([]); // No archive files

     const stats = await errorPersistenceService.getErrorStats();

     expect(stats.totalErrors).toBe(1);
     expect(stats.errorTypes['TEST_ERROR']).toBe(1);
     expect(stats.storageSize).toBe(100);
   });

    it('should handle errors when getting stats', async () => {
      (fs.stat as jest.Mock).mockRejectedValue(new Error('Stat failed'));

      const stats = await errorPersistenceService.getErrorStats();

      expect(stats.totalErrors).toBe(0);
      expect(stats.errorTypes).toEqual({});
      expect(stats.lastErrorTime).toBeNull();
      expect(stats.storageSize).toBe(0);
      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        expect.objectContaining({
          component: 'HotReloadErrorPersistenceService',
          operation: 'getErrorStats'
        })
      );
    });
  });

 describe('clearErrorLogs', () => {
    it('should clear error logs', async () => {
      (fs.unlink as jest.Mock).mockResolvedValue(undefined);
      (fs.readdir as jest.Mock).mockResolvedValue([
        'hotreload-errors-2023-01-01_10-00-00-000.jsonl'
      ]);

      await errorPersistenceService.clearErrorLogs();

      expect(fs.unlink).toHaveBeenCalledWith(expect.stringContaining('hotreload-errors.jsonl'));
      expect(fs.readdir).toHaveBeenCalledWith('./logs');
    });

    it('should handle errors during log clearing', async () => {
      (fs.unlink as jest.Mock).mockRejectedValue(new Error('Delete failed'));

      await errorPersistenceService.clearErrorLogs();

      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        expect.objectContaining({
          component: 'HotReloadErrorPersistenceService',
          operation: 'clearErrorLogs'
        })
      );
    });
  });

 describe('destroy', () => {
    it('should destroy the service and flush remaining errors', async () => {
      const errorReport: HotReloadErrorReport = {
        id: 'error-1',
        timestamp: new Date(),
        projectId: 'test-project',
        errorCode: 'TEST_ERROR',
        message: 'Test error message',
        stack: 'Error stack',
        context: {},
        resolved: false
      };

      await errorPersistenceService.queueError(errorReport);

      const flushSpy = jest.spyOn(errorPersistenceService as any, 'flushErrors').mockResolvedValue(undefined);

      await errorPersistenceService.destroy();

      expect(flushSpy).toHaveBeenCalled();
      expect(mockLogger.info).toHaveBeenCalledWith('HotReloadErrorPersistenceService destroyed');
    });
  });
});