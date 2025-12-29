import { ConnectionPool, ConnectionPoolConfig } from '../ConnectionPool';
import { LoggerService } from '../../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../../config/service/NebulaConfigService';
import { PerformanceMonitor } from '../../../../infrastructure/monitoring/PerformanceMonitor';
import { InfrastructureConfigService } from '../../../../infrastructure/config/InfrastructureConfigService';
import { ConfigService } from '../../../../config/ConfigService';
import { ConnectionWarmer } from '../ConnectionWarmer';
import { LoadBalancer, LoadBalanceStrategy } from '../LoadBalancer';
import { NebulaConfig } from '../../NebulaTypes';
import { Connection, ConnectionState } from '../Connection';

// Mock implementations
class MockLoggerService extends LoggerService {
  debug = jest.fn();
  info = jest.fn();
  warn = jest.fn();
  error = jest.fn();
}

class MockErrorHandlerService extends ErrorHandlerService {
  handleError = jest.fn();
}
class MockConfigService extends ConfigService {
  get = jest.fn().mockReturnValue({});
  set = jest.fn();
  has = jest.fn().mockReturnValue(false);
  clear = jest.fn();
}

class MockNebulaConfigService extends NebulaConfigService {
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
}

class MockConnectionWarmer extends ConnectionWarmer {
  warmConnection = jest.fn().mockResolvedValue({
    connectionId: 'test-connection',
    success: true,
    warmupTime: 10,
    queryResults: []
  });
  warmConnections = jest.fn().mockResolvedValue([]);
  updateConfig = jest.fn();
}

class MockLoadBalancer extends LoadBalancer {
  selectConnection = jest.fn().mockImplementation((connections: Connection[]) => connections[0] || null);
  updateConnectionWeight = jest.fn();
  updateConfig = jest.fn();
  destroy = jest.fn();
}

// Mock Connection class
jest.mock('../Connection', () => {
  const actual = jest.requireActual('../Connection');
  return {
    ...actual,
    Connection: jest.fn().mockImplementation((config, client) => ({
      getId: jest.fn().mockReturnValue(`conn_${Date.now()}`),
      getState: jest.fn().mockReturnValue(actual.ConnectionState.IDLE),
      getStats: jest.fn().mockReturnValue({
        id: `conn_${Date.now()}`,
        state: actual.ConnectionState.IDLE,
        created: new Date(),
        lastUsed: new Date(),
        queryCount: 0,
        errorCount: 0,
        totalQueryTime: 0
      }),
      getClient: jest.fn().mockReturnValue(client),
      execute: jest.fn().mockResolvedValue({}),
      isHealthy: jest.fn().mockResolvedValue(true),
      close: jest.fn().mockResolvedValue(undefined),
      markAsError: jest.fn(),
      reset: jest.fn(),
      getIdleTime: jest.fn().mockReturnValue(0),
      on: jest.fn()
    }))
  };
});

describe('ConnectionPool', () => {
  let connectionPool: ConnectionPool;
  let mockLogger: MockLoggerService;
  let mockErrorHandler: MockErrorHandlerService;
  let mockConfigService: MockNebulaConfigService;
  let mockPerformanceMonitor: MockPerformanceMonitor;
  let mockConnectionWarmer: MockConnectionWarmer;
  let mockLoadBalancer: MockLoadBalancer;
  let config: NebulaConfig;

  beforeEach(() => {
    mockLogger = new MockLoggerService();
    mockErrorHandler = new MockErrorHandlerService(mockLogger);
    mockConfigService = new MockNebulaConfigService(mockLogger, mockErrorHandler);
    mockPerformanceMonitor = new MockPerformanceMonitor(mockLogger);
    mockConnectionWarmer = new MockConnectionWarmer(mockLogger, mockErrorHandler, mockPerformanceMonitor);
    mockLoadBalancer = new MockLoadBalancer(mockLogger, mockErrorHandler, mockPerformanceMonitor);

    connectionPool = new ConnectionPool(
      mockLogger,
      mockErrorHandler,
      mockConfigService,
      mockPerformanceMonitor,
      mockConnectionWarmer,
      mockLoadBalancer
    );

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

  describe('initialize', () => {
    it('should initialize the connection pool successfully', async () => {
      await connectionPool.initialize(config);

      expect(mockLogger.info).toHaveBeenCalledWith('Initializing connection pool', expect.any(Object));
      expect(mockLogger.info).toHaveBeenCalledWith('Connection pool initialized successfully');
      expect(connectionPool.getPoolStats().totalConnections).toBe(2); // Default minConnections
    });

    it('should handle initialization errors', async () => {
      const error = new Error('Initialization failed');
      (mockLogger.info as jest.Mock).mockImplementationOnce(() => {
        throw error;
      });

      await expect(connectionPool.initialize(config)).rejects.toThrow('Initialization failed');
      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(error, { component: 'ConnectionPool', operation: 'initialize' });
    });

    it('should not reinitialize if already initialized', async () => {
      await connectionPool.initialize(config);
      await connectionPool.initialize(config);

      expect(mockLogger.warn).toHaveBeenCalledWith('Connection pool already initialized');
    });
  });

  describe('getConnection', () => {
    it('should get a connection successfully', async () => {
      await connectionPool.initialize(config);
      const connection = await connectionPool.getConnection();

      expect(connection).toBeDefined();
      expect(connectionPool.getPoolStats().activeConnections).toBe(1);
    });

    it('should throw error if pool is not initialized', async () => {
      await expect(connectionPool.getConnection()).rejects.toThrow('Connection pool not initialized');
    });

    it('should throw error if pool is closing', async () => {
      await connectionPool.initialize(config);
      (connectionPool as any).isClosing = true;

      await expect(connectionPool.getConnection()).rejects.toThrow('Connection pool is closing');
    });

    it('should create a new connection if no idle connections are available', async () => {
      const poolConfig: ConnectionPoolConfig = {
        minConnections: 1,
        maxConnections: 5,
        acquireTimeout: 30000,
        idleTimeout: 300000,
        healthCheck: {
          interval: 30000,
          timeout: 5000,
          maxFailures: 3,
          retryDelay: 1000
        },
        warming: {
          enabled: true,
          warmupQueries: ['YIELD 1'],
          warmupConcurrency: 1,
          warmupTimeout: 10000,
          retryAttempts: 1,
          retryDelay: 1000
        },
        loadBalancing: {
          strategy: LoadBalanceStrategy.LEAST_RESPONSE_TIME,
          healthCheckWeight: 0.3,
          responseTimeWeight: 0.4,
          errorRateWeight: 0.2,
          connectionCountWeight: 0.1,
          weightUpdateInterval: 30000,
          maxWeight: 100,
          minWeight: 1
        }
      };

      connectionPool.updatePoolConfig(poolConfig);
      await connectionPool.initialize(config);

      // Use all existing connections
      const connections = [];
      for (let i = 0; i < 3; i++) {
        connections.push(await connectionPool.getConnection());
      }

      // Request another connection (should create a new one)
      const newConnection = await connectionPool.getConnection();
      expect(newConnection).toBeDefined();
    });
  });

  describe('releaseConnection', () => {
    it('should release a connection successfully', async () => {
      await connectionPool.initialize(config);
      const connection = await connectionPool.getConnection();

      connectionPool.releaseConnection(connection);

      expect(connectionPool.getPoolStats().activeConnections).toBe(0);
      expect(connectionPool.getPoolStats().idleConnections).toBe(2);
    });

    it('should not release a connection if pool is not initialized', () => {
      const mockConnection = new (jest.requireMock('../Connection').Connection)();
      connectionPool.releaseConnection(mockConnection);

      // Should not throw an error
      expect(true).toBe(true);
    });

    it('should not release a connection if pool is closing', async () => {
      await connectionPool.initialize(config);
      (connectionPool as any).isClosing = true;

      const connection = await connectionPool.getConnection();
      connectionPool.releaseConnection(connection);

      // Should not change stats
      expect(connectionPool.getPoolStats().activeConnections).toBe(1);
    });

    it('should handle releasing a connection not managed by pool', async () => {
      await connectionPool.initialize(config);
      const mockConnection = new (jest.requireMock('../Connection').Connection)();

      connectionPool.releaseConnection(mockConnection);

      expect(mockLogger.warn).toHaveBeenCalledWith('Attempted to release connection not managed by pool', expect.any(Object));
    });
  });

  describe('close', () => {
    it('should close the connection pool successfully', async () => {
      await connectionPool.initialize(config);
      await connectionPool.close();

      expect(mockLogger.info).toHaveBeenCalledWith('Closing connection pool');
      expect(mockLogger.info).toHaveBeenCalledWith('Connection pool closed');
      expect(connectionPool.getPoolStats().totalConnections).toBe(0);
    });

    it('should not close the pool if already closing', async () => {
      await connectionPool.initialize(config);
      (connectionPool as any).isClosing = true;

      await connectionPool.close();

      // Should not attempt to close again
      expect(mockLogger.info).not.toHaveBeenCalledWith('Closing connection pool');
    });
  });

  describe('getPoolStats', () => {
    it('should return pool statistics', async () => {
      await connectionPool.initialize(config);
      const stats = connectionPool.getPoolStats();

      expect(stats).toHaveProperty('totalConnections');
      expect(stats).toHaveProperty('activeConnections');
      expect(stats).toHaveProperty('idleConnections');
      expect(stats).toHaveProperty('pendingRequests');
      expect(stats).toHaveProperty('totalAcquires');
      expect(stats).toHaveProperty('totalReleases');
      expect(stats).toHaveProperty('totalErrors');
      expect(stats).toHaveProperty('averageAcquireTime');
      expect(stats).toHaveProperty('averageConnectionAge');
    });
  });

  describe('updatePoolConfig', () => {
    it('should update pool configuration', () => {
      const newConfig: Partial<ConnectionPoolConfig> = {
        maxConnections: 20,
        acquireTimeout: 60000
      };

      connectionPool.updatePoolConfig(newConfig);

      expect(mockConnectionWarmer.updateConfig).toHaveBeenCalledWith(newConfig.warming);
      expect(mockLoadBalancer.updateConfig).toHaveBeenCalledWith(newConfig.loadBalancing);
    });
  });

  describe('startHealthCheck', () => {
    it('should start health check successfully', async () => {
      await connectionPool.initialize(config);
      connectionPool.startHealthCheck();

      expect(mockLogger.info).toHaveBeenCalledWith('Health check started');
    });

    it('should throw error if pool is not initialized', () => {
      expect(() => connectionPool.startHealthCheck()).toThrow('Connection pool not initialized');
    });
  });

  describe('stopHealthCheck', () => {
    it('should stop health check successfully', async () => {
      await connectionPool.initialize(config);
      connectionPool.stopHealthCheck();

      expect(mockLogger.info).toHaveBeenCalledWith('Health check stopped');
    });
  });
});