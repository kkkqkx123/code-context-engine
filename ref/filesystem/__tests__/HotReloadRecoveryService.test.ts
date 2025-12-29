import { Container } from 'inversify';
import { TYPES } from '../../../types';
import { HotReloadRecoveryService } from '../HotReloadRecoveryService';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { HotReloadError, HotReloadErrorCode } from '../HotReloadError';

// Mock logger service
class MockLoggerService {
  logs: { level: string; message: string; meta?: any }[] = [];
  
  async info(message: string, meta?: any) {
    this.logs.push({ level: 'info', message, meta });
  }
  
  async warn(message: string, meta?: any) {
    this.logs.push({ level: 'warn', message, meta });
  }
  
  async error(message: string, meta?: any) {
    this.logs.push({ level: 'error', message, meta });
  }
  
  async debug(message: string, meta?: any) {
    this.logs.push({ level: 'debug', message, meta });
  }
}

// Mock error handler service
class MockErrorHandlerService {
  reports: any[] = [];
  
  handleError(error: Error, context: any) {
    const report = {
      id: `err_${Date.now()}`,
      timestamp: new Date(),
      component: context.component,
      operation: context.operation,
      message: error.message,
      stack: error.stack,
      context: context
    };
    
    this.reports.push(report);
    return report;
  }
  
  handleHotReloadError(error: any, context: any) {
    const report = {
      id: `hot_reload_err_${Date.now()}`,
      timestamp: new Date(),
      component: context.component,
      operation: context.operation,
      message: error.message,
      stack: error.stack,
      context: context,
      errorCode: error.code
    };
    
    this.reports.push(report);
    return report;
  }
}

describe('HotReloadRecoveryService', () => {
  let container: Container;
  let recoveryService: HotReloadRecoveryService;
  let mockLogger: MockLoggerService;
  let mockErrorHandler: MockErrorHandlerService;

  beforeEach(() => {
    container = new Container();
    
    mockLogger = new MockLoggerService();
    mockErrorHandler = new MockErrorHandlerService();
    
    container.bind<LoggerService>(TYPES.LoggerService).toConstantValue(mockLogger as any);
    container.bind<ErrorHandlerService>(TYPES.ErrorHandlerService).toConstantValue(mockErrorHandler as any);
    
    recoveryService = new HotReloadRecoveryService(
      mockErrorHandler as any,
      mockLogger as any
    );
  });

  test('should initialize with default recovery strategies', () => {
    expect(recoveryService).toBeDefined();
    
    // Check that default strategies are set up
    expect(recoveryService.getRecoveryStrategy(HotReloadErrorCode.FILE_WATCH_FAILED)).toBeDefined();
    expect(recoveryService.getRecoveryStrategy(HotReloadErrorCode.CHANGE_DETECTION_FAILED)).toBeDefined();
    expect(recoveryService.getRecoveryStrategy(HotReloadErrorCode.INDEX_UPDATE_FAILED)).toBeDefined();
    expect(recoveryService.getRecoveryStrategy(HotReloadErrorCode.PERMISSION_DENIED)).toBeDefined();
    expect(recoveryService.getRecoveryStrategy(HotReloadErrorCode.FILE_TOO_LARGE)).toBeDefined();
    expect(recoveryService.getRecoveryStrategy(HotReloadErrorCode.PROJECT_NOT_FOUND)).toBeDefined();
  });

  test('should handle FILE_WATCH_FAILED error correctly', async () => {
    const error = new HotReloadError(
      HotReloadErrorCode.FILE_WATCH_FAILED,
      'File watch failed'
    );
    
    const context = { component: 'TestComponent', operation: 'testOperation' };
    
    await recoveryService.handleError(error, context);
    
    // Check that the recovery action was called (logged a warning)
    const warnLogs = mockLogger.logs.filter(log => log.level === 'warn');
    expect(warnLogs.length).toBeGreaterThan(0);
    expect(warnLogs.some(log => log.message.includes('Attempting to restart file watcher'))).toBe(true);
  });

  test('should handle PERMISSION_DENIED error correctly', async () => {
    const error = new HotReloadError(
      HotReloadErrorCode.PERMISSION_DENIED,
      'Permission denied'
    );
    
    const context = { component: 'TestComponent', operation: 'testOperation' };
    
    await recoveryService.handleError(error, context);
    
    // Check that the recovery action was called (logged a warning about permissions)
    const warnLogs = mockLogger.logs.filter(log => log.level === 'warn');
    expect(warnLogs.length).toBeGreaterThan(0);
    expect(warnLogs.some(log => log.message.includes('Permission denied for file monitoring'))).toBe(true);
 });

  test('should handle INDEX_UPDATE_FAILED error correctly', async () => {
    const error = new HotReloadError(
      HotReloadErrorCode.INDEX_UPDATE_FAILED,
      'Index update failed'
    );
    
    const context = { component: 'TestComponent', operation: 'testOperation' };
    
    await recoveryService.handleError(error, context);
    
    // Check that the recovery action was called (logged a warning about retrying)
    const warnLogs = mockLogger.logs.filter(log => log.level === 'warn');
    expect(warnLogs.length).toBeGreaterThan(0);
    expect(warnLogs.some(log => log.message.includes('Attempting to retry index update'))).toBe(true);
  });

  test('should return undefined for unknown error code', () => {
    const unknownStrategy = recoveryService.getRecoveryStrategy('UNKNOWN_ERROR_CODE' as HotReloadErrorCode);
    expect(unknownStrategy).toBeUndefined();
  });
});