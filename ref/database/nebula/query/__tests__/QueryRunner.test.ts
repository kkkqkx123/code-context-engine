import { QueryRunner, QueryRunnerConfig } from '../QueryRunner';
import { LoggerService } from '../../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../../config/service/NebulaConfigService';
import { PerformanceMonitor } from '../../../../infrastructure/monitoring/PerformanceMonitor';
import { InfrastructureConfigService } from '../../../../infrastructure/config/InfrastructureConfigService';
import { ISessionManager } from '../../session/SessionManager';
import { NebulaConfig, NebulaQueryResult } from '../../NebulaTypes';
import { Session } from '../../session/Session';
import { QueryCache } from '../QueryCache';
import { IBatchProcessingService } from '../../../../infrastructure/batching/types';
import { IRetryStrategy } from '../../retry/RetryStrategy';

// Mock implementations
class MockLoggerService extends LoggerService {
  debug = jest.fn();
  info = jest.fn();
  warn = jest.fn();
  error = jest.fn();
}

class MockErrorHandlerService extends ErrorHandlerService {
  constructor(logger: LoggerService) {
    super(logger);
  }
  handleError = jest.fn();
}

class MockNebulaConfigService extends NebulaConfigService {
  constructor(logger: LoggerService, errorHandler: ErrorHandlerService) {
    super(logger, errorHandler);
  }
  getConfig = jest.fn().mockReturnValue({
    host: 'localhost',
    port: 9669,
    username: 'root',
    password: 'nebula',
    space: 'test_space'
  });
}

class MockPerformanceMonitor extends PerformanceMonitor {
  constructor(logger: LoggerService) {
    super(logger, new InfrastructureConfigService(logger, {
      get: () => ({}),
      set: () => { },
      has: () => false,
      clear: () => { }
    } as any));
  }
  startOperation = jest.fn().mockReturnValue('operation-id');
  endOperation = jest.fn();
  recordOperation = jest.fn();
  recordQueryExecution = jest.fn();
}

class MockSessionManager implements ISessionManager {
  initialize = jest.fn().mockResolvedValue(undefined);
  getSession = jest.fn().mockResolvedValue({
    execute: jest.fn().mockResolvedValue({
      table: {},
      results: [],
      rows: [],
      data: []
    }),
    close: jest.fn().mockResolvedValue(undefined),
    switchSpace: jest.fn().mockResolvedValue(undefined),
    getStats: jest.fn().mockReturnValue({
      id: 'mock-session-id',
      state: 'active',
      created: new Date(),
      lastUsed: new Date(),
      queryCount: 0,
      errorCount: 0,
      totalQueryTime: 0
    })
  });
  releaseSession = jest.fn();
  invalidateSession = jest.fn();
  startSessionCleanup = jest.fn();
  stopSessionCleanup = jest.fn();
  switchSpace = jest.fn().mockResolvedValue(undefined);
  close = jest.fn().mockResolvedValue(undefined);
  getSessionStats = jest.fn().mockReturnValue({
    totalSessions: 1,
    activeSessions: 0,
    idleSessions: 1,
    sessionPoolSize: 10,
    averageSessionAge: 0
  });
}

class MockQueryCache {
  private cache = new Map<string, NebulaQueryResult>();

  get = jest.fn().mockImplementation((query: string, params?: Record<string, any>) => {
    const key = this.generateKey(query, params);
    return Promise.resolve(this.cache.get(key) || null);
  });

  set = jest.fn().mockImplementation((query: string, result: NebulaQueryResult, params?: Record<string, any>) => {
    const key = this.generateKey(query, params);
    this.cache.set(key, result);
    return Promise.resolve();
  });

  private generateKey(query: string, params?: Record<string, any>): string {
    // 简化的键生成逻辑
    if (params && Object.keys(params).length > 0) {
      return `${query}:${JSON.stringify(params)}`;
    }
    return query;
  }

  clear = jest.fn().mockImplementation(() => {
    this.cache.clear();
  });

  getStats = jest.fn().mockReturnValue({
    cacheSize: 0,
    cacheHits: 0,
    cacheMisses: 0,
    evictionCount: 0
  });

  updateConfig = jest.fn();
  cleanup = jest.fn();
}

class MockBatchProcessingService implements IBatchProcessingService {
  executeBatch = jest.fn().mockResolvedValue([]);
  executeDatabaseBatch = jest.fn().mockResolvedValue([]);
  executeEmbeddingBatch = jest.fn().mockResolvedValue([]);
  executeSimilarityBatch = jest.fn().mockResolvedValue({});
  executeHotReloadBatch = jest.fn().mockResolvedValue({});
  executeBatchWithRetry = jest.fn().mockResolvedValue([]);
  executeBatchWithMonitoring = jest.fn().mockResolvedValue([]);
  executeWithRetry = jest.fn().mockImplementation(async (fn) => await fn());
  executeWithMonitoring = jest.fn().mockImplementation(async (fn) => await fn());
  updateConfig = jest.fn();
  getPerformanceStats = jest.fn().mockReturnValue({
    count: 0,
    successRate: 0,
    averageDuration: 0,
    minDuration: 0,
    maxDuration: 0,
    p95Duration: 0,
    p99Duration: 0
  });
  getCurrentBatchSize = jest.fn().mockReturnValue(0);
  optimizeMemory = jest.fn().mockResolvedValue(undefined);
  optimizeBatchSize = jest.fn().mockResolvedValue(0);
  resetPerformanceStats = jest.fn();
}

class MockRetryStrategy implements IRetryStrategy {
  shouldRetry = jest.fn().mockReturnValue(true);
  getDelay = jest.fn().mockReturnValue(1000);
  executeWithRetry = jest.fn().mockImplementation(async (fn) => {
    return await fn();
  });
  getStats = jest.fn().mockReturnValue({
    totalRetries: 0,
    successfulRetries: 0,
    failedRetries: 0,
    averageRetryDelay: 0
  });
}

describe('QueryRunner', () => {
  let queryRunner: QueryRunner;
  let mockLogger: MockLoggerService;
  let mockErrorHandler: MockErrorHandlerService;
  let mockConfigService: MockNebulaConfigService;
  let mockPerformanceMonitor: MockPerformanceMonitor;
  let mockSessionManager: MockSessionManager;
  let mockQueryCache: MockQueryCache;
  let mockBatchProcessingService: MockBatchProcessingService;
  let mockRetryStrategy: MockRetryStrategy;
  let config: NebulaConfig;

  beforeEach(() => {
    mockLogger = new MockLoggerService();
    mockErrorHandler = new MockErrorHandlerService(mockLogger);
    mockConfigService = new MockNebulaConfigService(mockLogger, mockErrorHandler);
    mockPerformanceMonitor = new MockPerformanceMonitor(mockLogger);
    mockSessionManager = new MockSessionManager();
    mockQueryCache = new MockQueryCache();
    mockBatchProcessingService = new MockBatchProcessingService();
    mockRetryStrategy = new MockRetryStrategy();

    queryRunner = new QueryRunner(
      mockLogger,
      mockErrorHandler,
      mockConfigService,
      mockPerformanceMonitor,
      mockSessionManager,
      mockBatchProcessingService,
      mockRetryStrategy,
      {},
    );

    config = {
      host: 'localhost',
      port: 9669,
      username: 'root',
      password: 'nebula',
      space: 'test_space'
    };

    (queryRunner as any).nebulaConfig = config;

    // 确保缓存配置正确设置
    (queryRunner as any).config.cacheConfig.enabled = true;
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('execute', () => {
    it('should execute a query successfully', async () => {
      const query = 'SHOW SPACES';
      const result = await queryRunner.execute(query);

      expect(result).toEqual({
        table: {},
        results: [],
        rows: [],
        data: [],
        executionTime: expect.any(Number)
      });
      expect(mockSessionManager.getSession).toHaveBeenCalledWith(undefined);
    });

    it('should execute a query with parameters', async () => {
      const query = 'INSERT VERTEX person(name) VALUES "1":("Alice")';
      const params = { name: 'Alice' };
      const options = { useCache: false };

      const result = await queryRunner.execute(query, params, options);

      expect(result).toEqual({
        table: {},
        results: [],
        rows: [],
        data: [],
        executionTime: expect.any(Number)
      });
    });

    it('should use cached result when available', async () => {
      const query = 'SHOW SPACES';
      const cachedResult: NebulaQueryResult = {
        table: { cached: true },
        results: [],
        rows: [],
        data: [],
        executionTime: 0
      };

      // Enable cache in config
      (queryRunner as any).config.enableCache = true;
      (queryRunner as any).config.cacheConfig.enabled = true;

      // Mock cache to return cached result
      (mockQueryCache.get as jest.Mock).mockImplementationOnce(() => Promise.resolve(cachedResult));

      const result = await queryRunner.execute(query, undefined, { useCache: true });

      expect(result).toEqual(cachedResult);
      expect(mockQueryCache.get).toHaveBeenCalledWith(query, undefined);
    });

    it('should handle query execution errors', async () => {
      const query = 'INVALID QUERY';
      const error = new Error('Query execution failed');
      (mockSessionManager.getSession as jest.Mock).mockRejectedValueOnce(error);

      await expect(queryRunner.execute(query)).rejects.toThrow('Query execution failed');
      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        {
          component: 'QueryRunner',
          operation: 'execute',
          query,
          params: undefined,
          executionTime: expect.any(Number)
        }
      );
    });

    it('should cache result after successful execution', async () => {
      const query = 'SHOW SPACES';

      // Enable cache in config
      (queryRunner as any).config.enableCache = true;
      (queryRunner as any).config.cacheConfig.enabled = true;

      // Mock session to return a result
      (mockSessionManager.getSession as jest.Mock).mockResolvedValueOnce({
        execute: jest.fn().mockResolvedValue({
          table: {},
          results: [],
          rows: [],
          data: [],
          executionTime: 10
        }),
        close: jest.fn().mockResolvedValue(undefined),
        switchSpace: jest.fn().mockResolvedValue(undefined),
        getStats: jest.fn().mockReturnValue({
          id: 'mock-session-id',
          state: 'active',
          created: new Date(),
          lastUsed: new Date(),
          queryCount: 0,
          errorCount: 0,
          totalQueryTime: 0
        })
      });

      // Mock cache set method to track calls
      const setSpy = jest.spyOn(mockQueryCache, 'set');

      await queryRunner.execute(query, undefined, { useCache: true });

      expect(setSpy).toHaveBeenCalledWith(query, expect.objectContaining({
        table: expect.any(Object),
        results: expect.any(Array),
        rows: expect.any(Array),
        data: expect.any(Array),
        executionTime: expect.any(Number)
      }), undefined);
    });
  });

  describe('executeBatch', () => {
    it('should execute a batch of queries successfully', async () => {
      const queries = [
        { query: 'SHOW SPACES' },
        { query: 'SHOW HOSTS' }
      ];

      // Mock the batch processing service to return results
      (mockBatchProcessingService.executeBatch as jest.Mock).mockImplementationOnce(async (items, processor) => {
        return await processor(items);
      });

      // Mock the execute method to return results
      (mockSessionManager.getSession as jest.Mock).mockResolvedValueOnce({
        execute: jest.fn().mockResolvedValue({
          table: {},
          results: [],
          rows: [],
          data: []
        }),
        close: jest.fn().mockResolvedValue(undefined),
        switchSpace: jest.fn().mockResolvedValue(undefined),
        getStats: jest.fn().mockReturnValue({
          id: 'mock-session-id',
          state: 'active',
          created: new Date(),
          lastUsed: new Date(),
          queryCount: 0,
          errorCount: 0,
          totalQueryTime: 0
        })
      });

      const results = await queryRunner.executeBatch(queries);

      expect(results).toHaveLength(2);
      expect(mockBatchProcessingService.executeBatch).toHaveBeenCalledWith(
        expect.any(Array),
        expect.any(Function),
        expect.any(Object)
      );
    });

    it('should handle batch execution errors', async () => {
      const queries = [
        { query: 'SHOW SPACES' },
        { query: 'SHOW HOSTS' }
      ];
      const error = new Error('Batch execution failed');
      (mockBatchProcessingService.executeBatch as jest.Mock).mockRejectedValueOnce(error);

      await expect(queryRunner.executeBatch(queries)).rejects.toThrow('Batch execution failed');
      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(
        expect.any(Error),
        {
          component: 'QueryRunner',
          operation: 'executeBatch',
          queries,
          executionTime: expect.any(Number)
        }
      );
    });
  });

  describe('getCachedResult', () => {
    it('should get cached result successfully', async () => {
      const query = 'test-query';
      const cachedResult: NebulaQueryResult = {
        table: { cached: true },
        results: [],
        rows: [],
        data: [],
        executionTime: 0
      };

      // Enable cache in config
      (queryRunner as any).config.cacheConfig.enabled = true;

      (mockQueryCache.get as jest.Mock).mockImplementationOnce(() => Promise.resolve(cachedResult));

      const result = await queryRunner.getCachedResult(query);

      expect(result).toEqual(cachedResult);
      expect(mockQueryCache.get).toHaveBeenCalledWith(query, undefined);
    });
  });

  describe('setCachedResult', () => {
    it('should set cached result successfully', async () => {
      const query = 'test-query';
      const result: NebulaQueryResult = {
        table: {},
        results: [],
        rows: [],
        data: [],
        executionTime: 0
      };

      // Enable cache in config
      (queryRunner as any).config.cacheConfig.enabled = true;

      // Mock cache set method to track calls
      const setSpy = jest.spyOn(mockQueryCache, 'set');

      await queryRunner.setCachedResult(query, result);

      expect(setSpy).toHaveBeenCalledWith(query, result, undefined);
    });
  });

  describe('recordQueryMetrics', () => {
    it('should record query metrics successfully', () => {
      const query = 'SHOW SPACES';
      const duration = 100;
      const success = true;

      queryRunner.recordQueryMetrics(query, duration, success);

      expect(mockPerformanceMonitor.recordQueryExecution).toHaveBeenCalledWith(duration);
    });
  });

  describe('getStats', () => {
    it('should return query runner statistics', () => {
      const stats = queryRunner.getStats();

      expect(stats).toHaveProperty('totalQueries');
      expect(stats).toHaveProperty('successfulQueries');
      expect(stats).toHaveProperty('failedQueries');
      expect(stats).toHaveProperty('cacheHits');
      expect(stats).toHaveProperty('cacheMisses');
      expect(stats).toHaveProperty('averageExecutionTime');
      expect(stats).toHaveProperty('totalExecutionTime');
      expect(stats).toHaveProperty('queriesByType');
    });
  });

  describe('updateConfig', () => {
    it('should update query runner configuration', () => {
      const newConfig: Partial<QueryRunnerConfig> = {
        defaultTimeout: 60000,
        enableCache: false
      };

      queryRunner.updateConfig(newConfig);

      expect((queryRunner as any).config.defaultTimeout).toBe(60000);
      expect((queryRunner as any).config.enableCache).toBe(false);
    });
  });
});