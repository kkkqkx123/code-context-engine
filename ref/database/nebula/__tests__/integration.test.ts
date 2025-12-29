import { Container } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../config/service/NebulaConfigService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { NebulaClient } from '../client/NebulaClient';
import { ConnectionPool } from '../connection/ConnectionPool';
import { SessionManager } from '../session/SessionManager';
import { QueryRunner } from '../query/QueryRunner';
import { QueryCache } from '../query/QueryCache';
import { ConnectionWarmer } from '../connection/ConnectionWarmer';
import { LoadBalancer } from '../connection/LoadBalancer';
import { QueryPipeline } from '../query/QueryPipeline';
import { ParallelQueryExecutor } from '../query/ParallelQueryExecutor';
import { MemoryOptimizer } from '../memory/MemoryOptimizer';
import { NebulaConfig } from '../NebulaTypes';
import { InfrastructureConfigService } from '../../../infrastructure/config/InfrastructureConfigService';
import { ProjectMappingService } from '../../../database/ProjectMappingService';
import { SqliteDatabaseService } from '../../../database/splite/SqliteDatabaseService';
import { ConfigService } from '../../../config/ConfigService';
import { BatchProcessingService } from '../../../infrastructure/batching/BatchProcessingService';
import { ExponentialBackoffRetryStrategy } from '../retry/RetryStrategy';
import { CircuitBreaker } from '../circuit-breaker/CircuitBreaker';
import { SemanticBatchStrategy } from '../../../infrastructure/batching/strategies/SemanticBatchStrategy';
import { QdrantBatchStrategy } from '../../../infrastructure/batching/strategies/QdrantBatchStrategy';
import { NebulaBatchStrategy } from '../../../infrastructure/batching/strategies/NebulaBatchStrategy';
import { GraphBatchStrategy } from '../../../infrastructure/batching/strategies/GraphBatchStrategy';
import { EmbeddingBatchStrategy } from '../../../infrastructure/batching/strategies/EmbeddingBatchStrategy';
import { BatchStrategyFactory } from '../../../infrastructure/batching/strategies/BatchStrategyFactory';
import { EmbedderFactory } from '../../../embedders/EmbedderFactory';
import { EmbeddingCacheService } from '../../../embedders/EmbeddingCacheService';

// Mock implementations for integration testing
class MockLoggerService extends LoggerService {
  debug = jest.fn();
  info = jest.fn();
  warn = jest.fn();
  error = jest.fn();
}

class MockErrorHandlerService extends ErrorHandlerService {
  handleError = jest.fn();
  constructor() {
    super(new MockLoggerService());
  }
}

class MockNebulaConfigService extends NebulaConfigService {
  getConfig = jest.fn().mockReturnValue({
    host: 'localhost',
    port: 9669,
    username: 'root',
    password: 'nebula',
    space: 'test_space'
  });
  constructor() {
    super(new MockLoggerService(), new MockErrorHandlerService());
  }
}

class MockInfrastructureConfigService {
  getConfig = jest.fn().mockReturnValue({
    common: {
      enableCache: true,
      enableMonitoring: true,
      enableBatching: true,
      logLevel: 'info',
      enableHealthChecks: true,
      healthCheckInterval: 30000,
      gracefulShutdownTimeout: 10000
    },
    qdrant: {
      cache: { defaultTTL: 30000, maxEntries: 10000, cleanupInterval: 60000, enableStats: true, databaseSpecific: {} },
      performance: {
        monitoringInterval: 30000,
        metricsRetentionPeriod: 86400000,
        enableDetailedLogging: true,
        performanceThresholds: {
          queryExecutionTime: 5000,
          memoryUsage: 80,
          responseTime: 2000
        },
        databaseSpecific: {}
      },
      batch: { maxConcurrentOperations: 5, defaultBatchSize: 50, maxBatchSize: 500, minBatchSize: 10, memoryThreshold: 0.8, processingTimeout: 3000, retryAttempts: 3, retryDelay: 1000, adaptiveBatchingEnabled: true, performanceThreshold: 1000, adjustmentFactor: 0.1, databaseSpecific: {} },
      vector: { defaultCollection: 'default', collectionOptions: { vectorSize: 1536, distance: 'Cosine', indexing: { type: 'hnsw', options: {} } }, searchOptions: { limit: 10, threshold: 0.5, exactSearch: false } }
    },
    nebula: {
      cache: { defaultTTL: 30000, maxEntries: 10000, cleanupInterval: 60000, enableStats: true, databaseSpecific: {} },
      performance: {
        monitoringInterval: 1000,
        metricsRetentionPeriod: 8640000,
        enableDetailedLogging: true,
        performanceThresholds: {
          queryExecutionTime: 1000,
          memoryUsage: 80,
          responseTime: 500
        },
        databaseSpecific: {}
      },
      batch: { maxConcurrentOperations: 5, defaultBatchSize: 50, maxBatchSize: 500, minBatchSize: 10, memoryThreshold: 0.8, processingTimeout: 300000, retryAttempts: 3, retryDelay: 100, adaptiveBatchingEnabled: true, performanceThreshold: 1000, adjustmentFactor: 0.1, databaseSpecific: {} },
      graph: { defaultSpace: 'default', spaceOptions: { partitionNum: 10, replicaFactor: 1, vidType: 'FIXED_STRING' }, queryOptions: { timeout: 30000, retryAttempts: 3 }, schemaManagement: { autoCreateTags: false, autoCreateEdges: false } }
    }
  });
}

class MockPerformanceMonitor extends PerformanceMonitor {
  startOperation = jest.fn().mockReturnValue('operation-id');
  endOperation = jest.fn();
  recordOperation = jest.fn();
  constructor() {
    super(new MockLoggerService(), new MockInfrastructureConfigService() as any);
  }
}

class MockConfigService {
  getConfig() {
    return {
      nebula: {},
      qdrant: {},
      embedding: {},
      logging: {},
      monitoring: {},
      memoryMonitor: {},
      fileProcessing: {},
      batchProcessing: {},
      project: {},
      indexing: {},
      treeSitter: {},
      projectNaming: {},
      embeddingBatch: {},
      graphCache: {},
    };
  }
}

describe('Nebula Graph Integration Tests', () => {
  let container: Container;
  let mockLogger: MockLoggerService;
  let mockErrorHandler: MockErrorHandlerService;
  let mockConfigService: MockNebulaConfigService;
  let mockPerformanceMonitor: MockPerformanceMonitor;
  let config: NebulaConfig;

  beforeEach(() => {
    container = new Container();

    // Setup mocks
    mockLogger = new MockLoggerService();
    mockErrorHandler = new MockErrorHandlerService();
    mockConfigService = new MockNebulaConfigService();
    mockPerformanceMonitor = new MockPerformanceMonitor();

    // Bind mocks
    container.bind<LoggerService>(TYPES.LoggerService).toConstantValue(mockLogger);
    container.bind<ErrorHandlerService>(TYPES.ErrorHandlerService).toConstantValue(mockErrorHandler);
    container.bind<NebulaConfigService>(TYPES.NebulaConfigService).toConstantValue(mockConfigService);
    container.bind<PerformanceMonitor>(TYPES.PerformanceMonitor).toConstantValue(mockPerformanceMonitor);
    container.bind<ConfigService>(TYPES.ConfigService).toConstantValue(new MockConfigService() as any);

    // Bind infrastructure and database services
    container.bind<InfrastructureConfigService>(TYPES.InfrastructureConfigService).to(InfrastructureConfigService).inSingletonScope();
    container.bind<SqliteDatabaseService>(TYPES.SqliteDatabaseService).to(SqliteDatabaseService).inSingletonScope();
    container.bind<ProjectMappingService>(TYPES.UnifiedMappingService).to(ProjectMappingService).inSingletonScope();

    // Bind memory monitor service (required by BatchProcessingService)
    container.bind<any>(TYPES.MemoryMonitorService).toConstantValue({
      startMonitoring: jest.fn(),
      stopMonitoring: jest.fn(),
      getMemoryStatus: jest.fn().mockReturnValue({ usedMemoryMB: 0, totalMemoryMB: 0, percentUsed: 0, heapUsed: 0, heapTotal: 0 }),
      isMemoryAvailable: jest.fn().mockReturnValue(true),
      onMemoryWarning: jest.fn()
    });

    // Bind embedding cache and factory services (required by SemanticBatchStrategy)
    container.bind<EmbeddingCacheService>(TYPES.EmbeddingCacheService).to(EmbeddingCacheService).inSingletonScope();
    container.bind<EmbedderFactory>(TYPES.EmbedderFactory).to(EmbedderFactory).inSingletonScope();

    // Bind batch processing strategies
    container.bind<SemanticBatchStrategy>(SemanticBatchStrategy).to(SemanticBatchStrategy).inSingletonScope();
    container.bind<SemanticBatchStrategy>(TYPES.SemanticBatchStrategy).to(SemanticBatchStrategy).inSingletonScope();
    container.bind<QdrantBatchStrategy>(TYPES.QdrantBatchStrategy).to(QdrantBatchStrategy).inSingletonScope();
    container.bind<NebulaBatchStrategy>(TYPES.NebulaBatchStrategy).to(NebulaBatchStrategy).inSingletonScope();
    container.bind<GraphBatchStrategy>(TYPES.GraphBatchStrategy).to(GraphBatchStrategy).inSingletonScope();
    container.bind<EmbeddingBatchStrategy>(TYPES.EmbeddingBatchStrategy).to(EmbeddingBatchStrategy).inSingletonScope();
    container.bind<BatchStrategyFactory>(BatchStrategyFactory).to(BatchStrategyFactory).inSingletonScope();

    // Bind batch processing and resilience services
    container.bind<BatchProcessingService>(TYPES.BatchProcessingService).to(BatchProcessingService).inSingletonScope();
    container.bind<ExponentialBackoffRetryStrategy>(TYPES.IRetryStrategy).to(ExponentialBackoffRetryStrategy).inSingletonScope();
    container.bind<CircuitBreaker>(TYPES.ICircuitBreaker).to(CircuitBreaker).inSingletonScope();

    // Create actual instances of our services
    container.bind<ConnectionWarmer>(TYPES.ConnectionWarmer).to(ConnectionWarmer).inSingletonScope();
    container.bind<LoadBalancer>(TYPES.LoadBalancer).to(LoadBalancer).inSingletonScope();
    container.bind<QueryCache>(TYPES.QueryCache).to(QueryCache).inSingletonScope();
    container.bind<QueryPipeline>(TYPES.QueryPipeline).to(QueryPipeline).inSingletonScope();
    container.bind<ParallelQueryExecutor>(TYPES.ParallelQueryExecutor).to(ParallelQueryExecutor).inSingletonScope();
    container.bind<MemoryOptimizer>(TYPES.MemoryOptimizer).to(MemoryOptimizer).inSingletonScope();

    container.bind<ConnectionPool>(TYPES.IConnectionPool).to(ConnectionPool).inSingletonScope();
    container.bind<SessionManager>(TYPES.ISessionManager).to(SessionManager).inSingletonScope();
    container.bind<QueryRunner>(TYPES.IQueryRunner).to(QueryRunner).inSingletonScope();
    container.bind<NebulaClient>(TYPES.NebulaClient).to(NebulaClient).inSingletonScope();

    config = {
      host: 'localhost',
      port: 9669,
      username: 'root',
      password: 'nebula',
      space: 'test_space'
    };
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('Component Integration', () => {
    it('should properly integrate all components through dependency injection', () => {
      // Get the main client
      const nebulaClient = container.get<NebulaClient>(TYPES.NebulaClient);

      // Verify that all dependencies are properly injected
      expect(nebulaClient).toBeDefined();

      // Get other components to verify they're properly created
      const connectionPool = container.get<ConnectionPool>(TYPES.IConnectionPool);
      const sessionManager = container.get<SessionManager>(TYPES.ISessionManager);
      const queryRunner = container.get<QueryRunner>(TYPES.IQueryRunner);

      expect(connectionPool).toBeDefined();
      expect(sessionManager).toBeDefined();
      expect(queryRunner).toBeDefined();
    });

    it('should initialize all components successfully', async () => {
      const nebulaClient = container.get<NebulaClient>(TYPES.NebulaClient);

      await nebulaClient.initialize(config);

      // Verify that initialization was called on all components
      expect(mockLogger.info).toHaveBeenCalledWith('NebulaClient initialized successfully');
    });

    it('should execute a simple query through the full stack', async () => {
      const nebulaClient = container.get<NebulaClient>(TYPES.NebulaClient);

      // Mock the session execute method to return a simple result
      const mockSession = {
        execute: jest.fn().mockResolvedValue({
          table: {},
          results: [],
          rows: [],
          data: []
        }),
        close: jest.fn().mockResolvedValue(undefined),
        switchSpace: jest.fn().mockResolvedValue(undefined),
        getId: jest.fn().mockReturnValue('mock-session-id'),
        getStats: jest.fn().mockReturnValue({
          id: 'mock-session-id',
          state: 'active',
          created: new Date(),
          lastUsed: new Date(),
          queryCount: 0,
          errorCount: 0,
          totalQueryTime: 0
        })
      };

      // Mock the session manager to return our mock session
      const sessionManager = container.get<SessionManager>(TYPES.ISessionManager);
      (sessionManager.getSession as jest.Mock) = jest.fn().mockResolvedValue(mockSession);

      await nebulaClient.initialize(config);

      // Execute a simple query
      const result = await nebulaClient.executeQuery('SHOW SPACES');

      expect(result).toBeDefined();
      // session.execute is called with the prepared query (parameters are interpolated)
      expect(mockSession.execute).toHaveBeenCalledWith('SHOW SPACES');
    });

    it('should handle query execution with parameters', async () => {
      const nebulaClient = container.get<NebulaClient>(TYPES.NebulaClient);

      // Mock the session execute method
      const mockSession = {
        execute: jest.fn().mockResolvedValue({
          table: {},
          results: [],
          rows: [],
          data: []
        }),
        close: jest.fn().mockResolvedValue(undefined),
        switchSpace: jest.fn().mockResolvedValue(undefined),
        getId: jest.fn().mockReturnValue('mock-session-id'),
        getStats: jest.fn().mockReturnValue({
          id: 'mock-session-id',
          state: 'active',
          created: new Date(),
          lastUsed: new Date(),
          queryCount: 0,
          errorCount: 0,
          totalQueryTime: 0
        })
      };

      // Mock the session manager
      const sessionManager = container.get<SessionManager>(TYPES.ISessionManager);
      (sessionManager.getSession as jest.Mock) = jest.fn().mockResolvedValue(mockSession);

      await nebulaClient.initialize(config);

      // Execute a query with parameters
      const result = await nebulaClient.executeQuery(
        'INSERT VERTEX person(name) VALUES "1":($name)',
        { name: 'Alice' }
      );

      expect(result).toBeDefined();
      // session.execute is called with the prepared query where parameters are interpolated
      expect(mockSession.execute).toHaveBeenCalledWith(
        'INSERT VERTEX person(name) VALUES "1":("Alice")'
      );
    });

    it('should execute batch queries', async () => {
      const nebulaClient = container.get<NebulaClient>(TYPES.NebulaClient);

      // Mock the query runner executeBatch method
      const queryRunner = container.get<QueryRunner>(TYPES.IQueryRunner);
      (queryRunner.executeBatch as jest.Mock) = jest.fn().mockResolvedValue([
        { table: {}, results: [], rows: [], data: [] },
        { table: {}, results: [], rows: [], data: [] }
      ]);

      await nebulaClient.initialize(config);

      // Execute batch queries
      const queries = [
        { query: 'SHOW SPACES' },
        { query: 'SHOW HOSTS' }
      ];
      const results = await nebulaClient.executeBatch(queries);

      expect(results).toHaveLength(2);
      expect(queryRunner.executeBatch).toHaveBeenCalledWith(queries);
    });

  });

  describe('Service Integration', () => {
    it('should integrate with configuration service', () => {
      const configService = container.get<NebulaConfigService>(TYPES.NebulaConfigService);

      expect(configService).toBeDefined();
      expect(configService.getConfig).toBeDefined();
    });

    it('should integrate with performance monitoring service', () => {
      const performanceMonitor = container.get<PerformanceMonitor>(TYPES.PerformanceMonitor);

      expect(performanceMonitor).toBeDefined();
      expect(performanceMonitor.startOperation).toBeDefined();
      expect(performanceMonitor.endOperation).toBeDefined();
    });

    it('should integrate with query cache service', () => {
      const queryCache = container.get<QueryCache>(TYPES.QueryCache);

      expect(queryCache).toBeDefined();
      expect(queryCache.get).toBeDefined();
      expect(queryCache.set).toBeDefined();
    });

    it('should integrate with connection management services', () => {
      const connectionPool = container.get<ConnectionPool>(TYPES.IConnectionPool);
      const connectionWarmer = container.get<ConnectionWarmer>(TYPES.ConnectionWarmer);
      const loadBalancer = container.get<LoadBalancer>(TYPES.LoadBalancer);

      expect(connectionPool).toBeDefined();
      expect(connectionWarmer).toBeDefined();
      expect(loadBalancer).toBeDefined();
    });

    it('should integrate with query optimization services', () => {
      const queryPipeline = container.get<QueryPipeline>(TYPES.QueryPipeline);
      const parallelQueryExecutor = container.get<ParallelQueryExecutor>(TYPES.ParallelQueryExecutor);

      expect(queryPipeline).toBeDefined();
      expect(parallelQueryExecutor).toBeDefined();
    });

    it('should integrate with memory optimization services', () => {
      const memoryOptimizer = container.get<MemoryOptimizer>(TYPES.MemoryOptimizer);

      expect(memoryOptimizer).toBeDefined();
      expect(memoryOptimizer.getMemoryStats).toBeDefined();
    });
  });

  describe('Error Handling Integration', () => {
    it('should properly handle errors across components', async () => {
      const nebulaClient = container.get<NebulaClient>(TYPES.NebulaClient);

      // Mock an error in the session
      const error = new Error('Query execution failed');
      const mockSession = {
        execute: jest.fn().mockRejectedValue(error),
        close: jest.fn().mockResolvedValue(undefined),
        switchSpace: jest.fn().mockResolvedValue(undefined),
        getId: jest.fn().mockReturnValue('mock-session-id'),
        getStats: jest.fn().mockReturnValue({
          id: 'mock-session-id',
          state: 'active',
          created: new Date(),
          lastUsed: new Date(),
          queryCount: 0,
          errorCount: 0,
          totalQueryTime: 0
        })
      };

      // Mock the session manager
      const sessionManager = container.get<SessionManager>(TYPES.ISessionManager);
      (sessionManager.getSession as jest.Mock) = jest.fn().mockResolvedValue(mockSession);

      await nebulaClient.initialize(config);

      // Execute a query that will fail
      const executePromise = nebulaClient.executeQuery('INVALID QUERY');
      
      // The error will be wrapped/classified, so check the original error or the wrapped message
      await expect(executePromise).rejects.toThrow();
    });
  });

  describe('Statistics and Monitoring Integration', () => {
    it('should collect statistics across all components', async () => {
      const nebulaClient = container.get<NebulaClient>(TYPES.NebulaClient);

      await nebulaClient.initialize(config);

      // Get stats from the client
      const stats = nebulaClient.getStats();

      expect(stats).toBeDefined();
      expect(stats.connectionPool).toBeDefined();
      expect(stats.sessionManager).toBeDefined();
      expect(stats.queryRunner).toBeDefined();
      expect(stats.transactionManager).toBeDefined();
    });
  });
});