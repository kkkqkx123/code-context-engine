import { HotReloadConfigService } from '../HotReloadConfigService';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';

// Mock dependencies
jest.mock('../../../utils/LoggerService');
jest.mock('../../../utils/ErrorHandlerService');

describe('HotReloadConfigService', () => {
  let service: HotReloadConfigService;
  let mockLogger: jest.Mocked<LoggerService>;
  let mockErrorHandler: jest.Mocked<ErrorHandlerService>;

  beforeEach(() => {
    // 清除环境变量
    delete process.env.HOT_RELOAD_ENABLED;
    delete process.env.HOT_RELOAD_DEBOUNCE_INTERVAL;
    delete process.env.HOT_RELOAD_MAX_FILE_SIZE;
    delete process.env.HOT_RELOAD_MAX_CONCURRENT_PROJECTS;
    delete process.env.HOT_RELOAD_ENABLE_DETAILED_LOGGING;
    delete process.env.HOT_RELOAD_ERROR_MAX_RETRIES;
    delete process.env.HOT_RELOAD_ERROR_ALERT_THRESHOLD;
    delete process.env.HOT_RELOAD_ERROR_AUTO_RECOVERY;
    delete process.env.HOT_RELOAD_WATCH_PATTERNS;
    delete process.env.HOT_RELOAD_IGNORE_PATTERNS;

    // 创建 mock 实例
    mockLogger = new LoggerService() as jest.Mocked<LoggerService>;
    mockErrorHandler = new ErrorHandlerService(mockLogger) as jest.Mocked<ErrorHandlerService>;
    
    // 设置 mock 方法
    mockLogger.info = jest.fn();
    mockLogger.debug = jest.fn();
    mockLogger.error = jest.fn();
    mockLogger.warn = jest.fn();
    mockErrorHandler.handleError = jest.fn();

    // 创建服务实例
    service = new HotReloadConfigService(mockLogger, mockErrorHandler);
  });

  describe('getDefaultGlobalConfig', () => {
    it('should return default configuration using factory', () => {
      const config = service.getGlobalConfig();
      
      // 验证默认值
      expect(config.enabled).toBe(true);
      expect(config.defaultDebounceInterval).toBe(500);
      expect(config.defaultMaxFileSize).toBe(512000);
      expect(config.maxConcurrentProjects).toBe(5);
      expect(config.enableDetailedLogging).toBe(false);
      expect(config.defaultErrorHandling.maxRetries).toBe(3);
      expect(config.defaultErrorHandling.alertThreshold).toBe(5);
      expect(config.defaultErrorHandling.autoRecovery).toBe(true);
      expect(config.defaultWatchPatterns).toContain('**/*.{js,ts,jsx,tsx,json,md,py,go,java,css,html,scss}');
      expect(config.defaultIgnorePatterns).toContain('**/node_modules/**');
    });

    it('should use environment variables when they are set', () => {
      process.env.HOT_RELOAD_ENABLED = 'false';
      process.env.HOT_RELOAD_DEBOUNCE_INTERVAL = '1000';
      process.env.HOT_RELOAD_MAX_FILE_SIZE = '5242880'; // 5MB
      process.env.HOT_RELOAD_MAX_CONCURRENT_PROJECTS = '10';
      process.env.HOT_RELOAD_ENABLE_DETAILED_LOGGING = 'true';
      process.env.HOT_RELOAD_ERROR_MAX_RETRIES = '5';
      process.env.HOT_RELOAD_ERROR_ALERT_THRESHOLD = '10';
      process.env.HOT_RELOAD_ERROR_AUTO_RECOVERY = 'false';
      
      // 创建新的服务实例以获取更新的环境变量
      const newService = new HotReloadConfigService(mockLogger, mockErrorHandler);
      const config = newService.getGlobalConfig();
      
      expect(config.enabled).toBe(false);
      expect(config.defaultDebounceInterval).toBe(1000);
      expect(config.defaultMaxFileSize).toBe(5242880);
      expect(config.maxConcurrentProjects).toBe(10);
      expect(config.enableDetailedLogging).toBe(true);
      expect(config.defaultErrorHandling.maxRetries).toBe(5);
      expect(config.defaultErrorHandling.alertThreshold).toBe(10);
      expect(config.defaultErrorHandling.autoRecovery).toBe(false);
    });
  });

  describe('updateGlobalConfig', () => {
    it('should update global configuration', () => {
      const updateConfig = {
        enabled: false,
        defaultDebounceInterval: 1000,
        defaultErrorHandling: {
          maxRetries: 5,
          alertThreshold: 5,
          autoRecovery: true
        }
      };

      service.updateGlobalConfig(updateConfig);
      const config = service.getGlobalConfig();

      expect(config.enabled).toBe(false);
      expect(config.defaultDebounceInterval).toBe(1000);
      expect(config.defaultErrorHandling.maxRetries).toBe(5);
      // 其他值应保持不变
      expect(config.defaultMaxFileSize).toBe(512000);
      expect(config.defaultErrorHandling.alertThreshold).toBe(5);
      expect(config.defaultErrorHandling.autoRecovery).toBe(true);
    });

    it('should log when configuration is updated', () => {
      const updateConfig = { enabled: false };
      
      service.updateGlobalConfig(updateConfig);
      
      expect(mockLogger.info).toHaveBeenCalledWith(
        'Global hot reload configuration updated',
        expect.any(Object)
      );
    });
  });

  describe('getProjectConfig', () => {
    it('should return default project configuration when no project config exists', () => {
      const projectPath = '/test/project';
      const config = service.getProjectConfig(projectPath);

      expect(config.enabled).toBe(true);
      expect(config.debounceInterval).toBe(500);
      expect(config.watchPatterns).toContain('**/*.{js,ts,jsx,tsx,json,md,py,go,java,css,html,scss}');
      expect(config.ignorePatterns).toContain('**/node_modules/**');
      expect(config.maxFileSize).toBe(512000);
      expect(config.errorHandling.maxRetries).toBe(3);
    });

    it('should return project-specific configuration when it exists', () => {
      const projectPath = '/test/project';
      const projectConfig = {
        enabled: false,
        debounceInterval: 1000,
        watchPatterns: ['**/*.js'],
        ignorePatterns: ['**/node_modules/**'],
        maxFileSize: 5 * 1024 * 1024,
        errorHandling: {
          maxRetries: 5,
          alertThreshold: 10,
          autoRecovery: false
        }
      };

      service.setProjectConfig(projectPath, projectConfig);
      const config = service.getProjectConfig(projectPath);

      expect(config.enabled).toBe(false);
      expect(config.debounceInterval).toBe(1000);
      expect(config.watchPatterns).toEqual(['**/*.js']);
      expect(config.ignorePatterns).toEqual(['**/node_modules/**']);
      expect(config.maxFileSize).toBe(5 * 1024 * 1024);
      expect(config.errorHandling.maxRetries).toBe(5);
      expect(config.errorHandling.alertThreshold).toBe(10);
      expect(config.errorHandling.autoRecovery).toBe(false);
    });
  });

  describe('validateConfig', () => {
    it('should validate configuration and return errors for invalid values', () => {
      const invalidConfig = {
        debounceInterval: 10, // Too small
        maxFileSize: 200 * 1024 * 1024, // Too large
        errorHandling: {
          maxRetries: -1, // Negative
          alertThreshold: 5,
          autoRecovery: true
        }
      };

      const result = service.validateConfig(invalidConfig);

      expect(result.isValid).toBe(false);
      expect(result.errors).toContain('Debounce interval must be at least 50ms');
      expect(result.errors).toContain('Max file size must be between 1 byte and 100MB');
      expect(result.errors).toContain('Max retries must be non-negative');
    });

    it('should return valid for correct configuration', () => {
      const validConfig = {
        debounceInterval: 500,
        maxFileSize: 512000,
        errorHandling: {
          maxRetries: 3,
          alertThreshold: 5,
          autoRecovery: true
        }
      };

      const result = service.validateConfig(validConfig);

      expect(result.isValid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });
  });
});