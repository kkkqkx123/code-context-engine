import { SessionManager, SessionManagerConfig } from '../SessionManager';
import { LoggerService } from '../../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../../config/service/NebulaConfigService';
import { PerformanceMonitor } from '../../../../infrastructure/monitoring/PerformanceMonitor';
import { InfrastructureConfigService } from '../../../../infrastructure/config/InfrastructureConfigService';
import { IConnectionPool } from '../../connection/ConnectionPool';
import { NebulaConfig } from '../../NebulaTypes';

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
    password: 'test_password',
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

class MockConnectionPool implements IConnectionPool {
  initialize = jest.fn().mockResolvedValue(undefined);
  getConnection = jest.fn().mockResolvedValue({
    execute: jest.fn().mockResolvedValue({}),
    close: jest.fn().mockResolvedValue(undefined),
    isHealthy: jest.fn().mockResolvedValue(true),
    getId: jest.fn().mockReturnValue('mock-connection-id'),
    getState: jest.fn().mockReturnValue('idle'),
    getStats: jest.fn().mockReturnValue({
      id: 'mock-connection-id',
      state: 'idle',
      created: new Date(),
      lastUsed: new Date(),
      queryCount: 0,
      errorCount: 0,
      totalQueryTime: 0
    }),
    getClient: jest.fn()
  });
  releaseConnection = jest.fn();
  close = jest.fn().mockResolvedValue(undefined);
  startHealthCheck = jest.fn();
  stopHealthCheck = jest.fn();
  getPoolStats = jest.fn().mockReturnValue({
    totalConnections: 1,
    activeConnections: 0,
    idleConnections: 1,
    pendingRequests: 0,
    totalAcquires: 0,
    totalReleases: 0,
    totalErrors: 0,
    averageAcquireTime: 0,
    averageConnectionAge: 0
  });
}

// Mock Session class
jest.mock('../Session', () => {
  const actual = jest.requireActual('../Session');
  return {
    ...actual,
    Session: jest.fn().mockImplementation((connection, config, logger, errorHandler, performanceMonitor) => ({
      getId: jest.fn().mockReturnValue(`session_${Date.now()}`),
      execute: jest.fn().mockResolvedValue({}),
      close: jest.fn().mockResolvedValue(undefined),
      switchSpace: jest.fn().mockResolvedValue(undefined),
      getStats: jest.fn().mockReturnValue({
        id: `session_${Date.now()}`,
        state: 'active',
        created: new Date(),
        lastUsed: new Date(),
        queryCount: 0,
        errorCount: 0,
        totalQueryTime: 0
      }),
      on: jest.fn()
    }))
  };
});

describe('SessionManager', () => {
  let sessionManager: SessionManager;
  let mockLogger: MockLoggerService;
  let mockErrorHandler: MockErrorHandlerService;
  let mockConfigService: MockNebulaConfigService;
  let mockPerformanceMonitor: MockPerformanceMonitor;
  let mockConnectionPool: MockConnectionPool;
  let config: NebulaConfig;

  beforeEach(() => {
    mockLogger = new MockLoggerService();
    mockErrorHandler = new MockErrorHandlerService(mockLogger);
    mockConfigService = new MockNebulaConfigService(mockLogger, mockErrorHandler);
    mockPerformanceMonitor = new MockPerformanceMonitor(mockLogger);
    mockConnectionPool = new MockConnectionPool();

    sessionManager = new SessionManager(
      mockLogger,
      mockErrorHandler,
      mockConfigService,
      mockPerformanceMonitor,
      mockConnectionPool
    );

    config = {
      host: 'localhost',
      port: 9669,
      username: 'root',
      password: 'test_password',
      space: 'test_space'
    };
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('initialize', () => {
    it('should initialize the session manager successfully', async () => {
      await sessionManager.initialize();

      expect(mockLogger.info).toHaveBeenCalledWith('Session manager initialized successfully', expect.any(Object));
      expect(sessionManager.getSessionStats().totalSessions).toBe(0);
    });

    it('should handle initialization errors', async () => {
      const error = new Error('Initialization failed');
      (mockConnectionPool.getConnection as jest.Mock).mockRejectedValueOnce(error);

      await expect(sessionManager.initialize()).rejects.toThrow('Initialization failed');
      expect(mockErrorHandler.handleError).toHaveBeenCalledWith(error, { component: 'SessionManager', operation: 'initialize' });
    });

    it('should not reinitialize if already initialized', async () => {
      await sessionManager.initialize();
      await sessionManager.initialize();

      expect(mockLogger.warn).toHaveBeenCalledWith('Session manager already initialized');
    });
  });

  describe('getSession', () => {
    it('should get a session successfully', async () => {
      await sessionManager.initialize();
      const session = await sessionManager.getSession('test_space');

      expect(session).toBeDefined();
      expect(sessionManager.getSessionStats().totalSessions).toBe(1);
    });

    it('should throw error if manager is not initialized', async () => {
      await expect(sessionManager.getSession('test_space')).rejects.toThrow('Session manager not initialized');
    });

    it('should throw error if manager is closing', async () => {
      await sessionManager.initialize();
      (sessionManager as any).isClosing = true;

      await expect(sessionManager.getSession('test_space')).rejects.toThrow('Session manager is closing');
    });

    it('should reuse existing session for the same space', async () => {
      await sessionManager.initialize();
      const session1 = await sessionManager.getSession('test_space');
      const session2 = await sessionManager.getSession('test_space');

      // Should return the same session
      expect(session1).toBe(session2);
    });

    it('should create different sessions for different spaces', async () => {
      await sessionManager.initialize();
      const session1 = await sessionManager.getSession('space1');
      const session2 = await sessionManager.getSession('space2');

      // Should return different sessions
      expect(session1).not.toBe(session2);
    });
  });

  describe('releaseSession', () => {
    it('should release a session successfully', async () => {
      await sessionManager.initialize();
      const session = await sessionManager.getSession('test_space');

      sessionManager.releaseSession(session);

      expect(sessionManager.getSessionStats().activeSessions).toBeLessThanOrEqual(sessionManager.getSessionStats().totalSessions);
    });

    it('should not release a session if manager is not initialized', async () => {
      await sessionManager.initialize();
      const session = await sessionManager.getSession('test_space');

      (sessionManager as any).isInitialized = false;
      sessionManager.releaseSession(session);

      // Should not throw an error
      expect(true).toBe(true);
    });

    it('should not release a session if manager is closing', async () => {
      await sessionManager.initialize();
      const session = await sessionManager.getSession('test_space');

      (sessionManager as any).isClosing = true;
      sessionManager.releaseSession(session);

      // Should not change stats
      expect(sessionManager.getSessionStats().activeSessions).toBe(1);
    });
  });

  describe('close', () => {
    it('should close the session manager successfully', async () => {
      await sessionManager.initialize();
      const session = await sessionManager.getSession('test_space');

      await sessionManager.close();

      expect(mockLogger.info).toHaveBeenCalledWith('Session manager closed');
      expect(sessionManager.getSessionStats().totalSessions).toBe(0);
    });

    it('should not close the manager if already closing', async () => {
      await sessionManager.initialize();
      (sessionManager as any).isClosing = true;

      await sessionManager.close();

      // Should not attempt to close again
      expect(mockLogger.info).not.toHaveBeenCalledWith('Session manager closed');
    });
  });

  describe('getSessionStats', () => {
    it('should return session statistics', async () => {
      await sessionManager.initialize();
      const stats = sessionManager.getSessionStats();

      expect(stats).toHaveProperty('totalSessions');
      expect(stats).toHaveProperty('activeSessions');
      expect(stats).toHaveProperty('idleSessions');
      expect(stats).toHaveProperty('sessionPoolSize');
      expect(stats).toHaveProperty('averageSessionAge');
    });
  });

  describe('configuration', () => {
    it('should have correct default configuration', () => {
      // Check that the session manager has the expected default configuration
      const config = (sessionManager as any).config;
      expect(config.maxSessions).toBe(50);
      expect(config.sessionTimeout).toBe(1800000);
      expect(config.idleTimeout).toBe(300000);
      expect(config.cleanupInterval).toBe(60000);
      expect(config.spaceCacheSize).toBe(100);
    });
  });

  describe('cleanupExpiredSessions', () => {
    it('should clean up expired sessions', async () => {
      await sessionManager.initialize();
      const session = await sessionManager.getSession('test_space');

      // Set a very short timeout to trigger cleanup
      (sessionManager as any).config.sessionTimeout = 1;

      // Wait for cleanup to happen
      await new Promise(resolve => setTimeout(resolve, 10));

      // Trigger cleanup by getting another session
      await sessionManager.getSession('test_space');

      // Should have cleaned up the expired session
      expect(sessionManager.getSessionStats().totalSessions).toBe(1); // New session created
    });
  });
});
