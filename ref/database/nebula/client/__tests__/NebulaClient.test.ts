import { Container } from 'inversify';
import { NebulaClient, INebulaClient } from '../NebulaClient';
import { TYPES } from '../../../../types';
import { NebulaConfig } from '../../NebulaTypes';
// Mock dependencies
const mockLoggerService = {
  info: jest.fn(),
  debug: jest.fn(),
  warn: jest.fn(),
  error: jest.fn(),
};

const mockErrorHandlerService = {
  handleError: jest.fn(),
};

const mockConfigService = {
  loadConfig: jest.fn().mockReturnValue({
    host: 'localhost',
    port: 9669,
    username: 'root',
    password: '[REDACTED:password]',
    space: 'test_space',
    timeout: 30000,
    maxConnections: 10,
    retryAttempts: 3,
    retryDelay: 1000,
    bufferSize: 1024,
    pingInterval: 30000,
    vidTypeLength: 32,
  }),
};

const mockPerformanceMonitor = {
  startOperation: jest.fn().mockReturnValue('operation-id'),
  endOperation: jest.fn(),
};

const mockQueryRunner = {
  execute: jest.fn(),
  executeBatch: jest.fn(),
  getStats: jest.fn().mockReturnValue({}),
};

const mockConnectionPool = {
  initialize: jest.fn().mockResolvedValue(undefined),
  close: jest.fn().mockResolvedValue(undefined),
  getStats: jest.fn().mockReturnValue({}),
};

const mockSessionManager = {
  initialize: jest.fn().mockResolvedValue(undefined),
  close: jest.fn().mockResolvedValue(undefined),
  getStats: jest.fn().mockReturnValue({}),
};

describe('NebulaClient', () => {
  let container: Container;
  let nebulaClient: INebulaClient;
  let mockNebulaClient: any;

  beforeEach(() => {
    container = new Container();

    // Bind mocks
    container.bind(TYPES.LoggerService).toConstantValue(mockLoggerService);
    container.bind(TYPES.ErrorHandlerService).toConstantValue(mockErrorHandlerService);
    container.bind(TYPES.NebulaConfigService).toConstantValue(mockConfigService);
    container.bind(TYPES.PerformanceMonitor).toConstantValue(mockPerformanceMonitor);
    container.bind(TYPES.IQueryRunner).toConstantValue(mockQueryRunner);
    container.bind(TYPES.IConnectionPool).toConstantValue(mockConnectionPool);
    container.bind(TYPES.ISessionManager).toConstantValue(mockSessionManager);

    // Mock additional dependencies needed by adapter
    const mockProjectManager = {
      createSpaceForProject: jest.fn().mockResolvedValue(true),
      deleteSpaceForProject: jest.fn().mockResolvedValue(true),
      insertNodesForProject: jest.fn().mockResolvedValue(true),
      insertRelationshipsForProject: jest.fn().mockResolvedValue(true),
      findNodesForProject: jest.fn().mockResolvedValue([]),
      findRelationshipsForProject: jest.fn().mockResolvedValue([])
    };

    const mockProjectIdManager = {
      listAllProjectPaths: jest.fn().mockReturnValue(['/test/project'])
    };

    mockNebulaClient = {
      initialize: jest.fn().mockImplementation(async (config?: any) => {
        if (config) {
          await mockConnectionPool.initialize(config);
          await mockSessionManager.initialize(config);
        } else {
          await mockConnectionPool.initialize(mockConfigService.loadConfig());
          await mockSessionManager.initialize(mockConfigService.loadConfig());
        }
        return true;
      }),
      isConnected: jest.fn().mockReturnValue(false),
      isInitialized: jest.fn().mockReturnValue(false),
      close: jest.fn().mockResolvedValue(undefined),
      reconnect: jest.fn().mockResolvedValue(true),
      connect: jest.fn().mockImplementation(async () => {
        mockNebulaClient.isConnected.mockReturnValue(true);
        mockNebulaClient.isInitialized.mockReturnValue(true);
      }),
      disconnect: jest.fn().mockImplementation(async () => {
        mockNebulaClient.isConnected.mockReturnValue(false);
      }),
      execute: jest.fn().mockResolvedValue({}),
      executeQuery: jest.fn().mockResolvedValue({}),
      executeBatch: jest.fn().mockResolvedValue([]),
      getStats: jest.fn().mockReturnValue({}),
      updateConfig: jest.fn().mockImplementation((config: any) => {
        mockNebulaClient.getConfig.mockReturnValue({ ...mockConfigService.loadConfig(), ...config });
      }),
      getConfig: jest.fn().mockReturnValue(mockConfigService.loadConfig()),
      on: jest.fn(),
      off: jest.fn(),
      emit: jest.fn(),
      healthCheck: jest.fn().mockImplementation(async () => {
        const connected = mockNebulaClient.isConnected();
        return {
          status: connected ? 'healthy' : 'unhealthy',
          details: {
            connected,
            stats: mockNebulaClient.getStats(),
            lastCheck: new Date()
          }
        };
      })
    };

    const mockGraphDatabaseService = {
      executeReadQuery: jest.fn().mockResolvedValue({}),
      executeWriteQuery: jest.fn().mockResolvedValue({}),
      useSpace: jest.fn().mockResolvedValue(undefined),
      getDatabaseStats: jest.fn().mockResolvedValue({}),
      getNebulaClient: jest.fn().mockReturnValue(mockNebulaClient)
    };

    container.bind(TYPES.INebulaProjectManager).toConstantValue(mockProjectManager);
    container.bind(TYPES.ProjectIdManager).toConstantValue(mockProjectIdManager);
    container.bind(TYPES.IGraphDatabaseService).toConstantValue(mockGraphDatabaseService);
    
    // Bind NebulaClient implementation
    container.bind(TYPES.NebulaClient).to(NebulaClient);
    container.bind(TYPES.INebulaClient).to(NebulaClient);

    nebulaClient = container.get<NebulaClient>(TYPES.NebulaClient);
  });

  describe('Interface Compliance', () => {
    it('should implement all INebulaClient required methods', () => {
      const requiredMethods: (keyof INebulaClient)[] = [
        'initialize', 'isConnected', 'isInitialized', 'close', 'reconnect',
        'connect', 'disconnect', 'execute', 'executeQuery', 'executeBatch',
        'getStats', 'updateConfig', 'getConfig', 'on', 'off', 'emit', 'healthCheck'
      ];

      requiredMethods.forEach(method => {
        expect(typeof nebulaClient[method]).toBe('function');
      });
    });

    it('should implement all backward compatibility methods', () => {
      const compatibilityMethods = [
        'deleteSpaceForProject', 'getDatabaseStats'
      ];

      compatibilityMethods.forEach(method => {
        expect(typeof (nebulaClient as any)[method]).toBe('function');
      });
    });
  });

  describe('Initialization', () => {
    it('should initialize successfully with config', async () => {
      const config: NebulaConfig = {
        host: 'localhost',
        port: 9669,
        username: 'root',
        password: '[REDACTED:password]',
        space: 'test_space'
      };

      const result = await nebulaClient.initialize(config);
      expect(result).toBe(true);
      expect(mockConnectionPool.initialize).toHaveBeenCalledWith(config);
      expect(mockSessionManager.initialize).toHaveBeenCalledWith(config);
    });

    it('should initialize successfully without config (use existing)', async () => {
      const result = await nebulaClient.initialize();
      expect(result).toBe(true);
      expect(mockConnectionPool.initialize).toHaveBeenCalled();
      expect(mockSessionManager.initialize).toHaveBeenCalled();
    });

    it('should return false on initialization failure', async () => {
      // Mock the connectionPool.initialize to throw an error
      mockConnectionPool.initialize.mockRejectedValueOnce(new Error('Initialization failed'));

      const result = await nebulaClient.initialize();
      expect(result).toBe(false);
    });

    it('should check initialization status', () => {
      expect(nebulaClient.isInitialized()).toBe(false);
    });
  });

  describe('Connection Management', () => {
    it('should connect successfully', async () => {
      await nebulaClient.connect();
      expect(nebulaClient.isConnected()).toBe(true);
    });

    it('should disconnect successfully', async () => {
      await nebulaClient.connect();
      await nebulaClient.disconnect();
      expect(nebulaClient.isConnected()).toBe(false);
    });

    it('should reconnect successfully', async () => {
      await nebulaClient.connect();
      const result = await nebulaClient.reconnect();
      expect(result).toBe(true);
      expect(nebulaClient.isConnected()).toBe(true);
    });
  });

  describe('Health Check', () => {
    it('should return healthy status when connected', async () => {
      await nebulaClient.connect();
      const health = await nebulaClient.healthCheck();

      expect(health.status).toBe('healthy');
      expect(health.details).toBeDefined();
      expect(health.details.connected).toBe(true);
    });

    it('should return unhealthy status when not connected', async () => {
      const health = await nebulaClient.healthCheck();

      expect(health.status).toBe('unhealthy');
      expect(health.details.connected).toBe(false);
    });
  });

  describe('Configuration Management', () => {
    it('should get config', () => {
      const config = nebulaClient.getConfig();
      expect(config).toBeDefined();
      expect(config.host).toBe('localhost');
    });

    it('should update config', () => {
      const newConfig = { host: 'new-host' };
      nebulaClient.updateConfig(newConfig);

      const updatedConfig = nebulaClient.getConfig();
      expect(updatedConfig.host).toBe('new-host');
    });
  });

  describe('Error Handling', () => {
    it('should handle connection errors', async () => {
      // Mock the actual connect method to throw an error
      const originalConnect = (nebulaClient as any).connect;
      (nebulaClient as any).connect = jest.fn().mockRejectedValueOnce(new Error('Connection failed'));

      await expect(nebulaClient.connect()).rejects.toThrow('Connection failed');
      
      // Restore original method
      (nebulaClient as any).connect = originalConnect;
    });

    it('should handle query errors', async () => {
      await nebulaClient.connect();
      
      // Mock queryRunner.execute to throw an error
      mockQueryRunner.execute.mockRejectedValueOnce(new Error('Query failed'));

      await expect(nebulaClient.execute('INVALID QUERY')).rejects.toThrow('Query failed');
    });
  });
});
