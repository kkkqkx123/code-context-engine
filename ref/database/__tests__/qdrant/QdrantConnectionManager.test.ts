// Mock QdrantClient module
jest.mock('@qdrant/js-client-rest', () => {
  const mockQdrantClient = jest.fn().mockImplementation(() => {
    const client = {
      getCollections: jest.fn().mockResolvedValue({ collections: [] }),
      close: jest.fn(),
    };
    // Set the prototype to make instanceof work
    Object.setPrototypeOf(client, mockQdrantClient.prototype);
    return client;
  });

  return {
    QdrantClient: mockQdrantClient,
  };
});

import { QdrantConnectionManager, IQdrantConnectionManager } from '../../qdrant/QdrantConnectionManager';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { ConfigService } from '../../../config/ConfigService';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { QdrantClient } from '@qdrant/js-client-rest';
import { ConnectionStatus, QdrantEventType, QdrantEvent } from '../../qdrant/QdrantTypes';

// Mock dependencies
const mockLogger = {
  info: jest.fn(),
  warn: jest.fn(),
  error: jest.fn(),
};

const mockErrorHandler = {
  handleError: jest.fn(),
};

const mockConfigService = {
  get: jest.fn().mockReturnValue({
    host: 'localhost',
    port: 6333,
    useHttps: false,
    timeout: 30000,
    collection: 'default',
  }),
};

const mockDatabaseLogger = {
  logDatabaseEvent: jest.fn(),
  logConnectionEvent: jest.fn(),
  logBatchOperation: jest.fn(),
  logCollectionOperation: jest.fn(),
  logVectorOperation: jest.fn(),
  logQueryOperation: jest.fn(),
  logProjectOperation: jest.fn(),
};

const mockPerformanceMonitor = {
  recordOperation: jest.fn(),
  getOperationStats: jest.fn(),
};

describe('QdrantConnectionManager', () => {
  let connectionManager: QdrantConnectionManager;

  beforeEach(() => {
    // Reset all mocks before each test
    jest.clearAllMocks();

    // Create a new instance of QdrantConnectionManager with mocked dependencies
    connectionManager = new QdrantConnectionManager(
      mockConfigService as unknown as ConfigService,
      mockLogger as unknown as LoggerService,
      mockErrorHandler as unknown as ErrorHandlerService,
      mockDatabaseLogger as unknown as DatabaseLoggerService,
      mockPerformanceMonitor as unknown as PerformanceMonitor
    );
  });

  describe('initialize', () => {
    it('should initialize successfully', async () => {
      const result = await connectionManager.initialize();

      expect(result).toBe(true);
      expect(connectionManager.isConnected()).toBe(true);
      expect(connectionManager.getConnectionStatus()).toBe(ConnectionStatus.CONNECTED);
    });

    it('should handle initialization failure', async () => {
      // Mock QdrantClient to throw an error
      const { QdrantClient } = jest.requireMock('@qdrant/js-client-rest');
      QdrantClient.mockImplementationOnce(() => {
        return {
          getCollections: jest.fn().mockRejectedValue(new Error('Connection failed')),
        };
      });

      const result = await connectionManager.initialize();

      expect(result).toBe(false);
      expect(connectionManager.isConnected()).toBe(false);
      expect(connectionManager.getConnectionStatus()).toBe(ConnectionStatus.ERROR);
    });
  });

  describe('close', () => {
    it('should close the connection', async () => {
      // First initialize the connection
      await connectionManager.initialize();

      // Then close it
      await connectionManager.close();

      expect(connectionManager.isConnected()).toBe(false);
      expect(connectionManager.getConnectionStatus()).toBe(ConnectionStatus.DISCONNECTED);
    });
  });

  describe('isConnected', () => {
    it('should return false when not connected', () => {
      expect(connectionManager.isConnected()).toBe(false);
    });

    it('should return true when connected', async () => {
      await connectionManager.initialize();
      expect(connectionManager.isConnected()).toBe(true);
    });
  });

  describe('getConnectionStatus', () => {
    it('should return DISCONNECTED when not connected', () => {
      expect(connectionManager.getConnectionStatus()).toBe(ConnectionStatus.DISCONNECTED);
    });

    it('should return CONNECTED when connected', async () => {
      await connectionManager.initialize();
      expect(connectionManager.getConnectionStatus()).toBe(ConnectionStatus.CONNECTED);
    });
  });

  describe('getClient', () => {
    it('should return null when not initialized', () => {
      expect(connectionManager.getClient()).toBeNull();
    });

    it('should return QdrantClient instance when initialized', async () => {
      await connectionManager.initialize();
      const client = connectionManager.getClient();
      expect(client).toBeInstanceOf(QdrantClient);
    });
  });

  describe('getConfig', () => {
    it('should return the configuration', () => {
      const config = connectionManager.getConfig();
      expect(config).toEqual({
        host: 'localhost',
        port: 6333,
        useHttps: false,
        timeout: 30000,
        collection: 'default',
      });
    });
  });

  describe('updateConfig', () => {
    it('should update the configuration', () => {
      const newConfig = {
        host: 'new-host',
        port: 6334,
      };

      connectionManager.updateConfig(newConfig);

      const config = connectionManager.getConfig();
      expect(config.host).toBe('new-host');
      expect(config.port).toBe(6334);
    });
  });

  describe('subscribe', () => {
    it('should subscribe to an event', () => {
      const listener = jest.fn();

      const subscription = connectionManager.subscribe(QdrantEventType.CONNECTED, listener);

      // Verify that the method exists and returns a subscription object
      expect(typeof connectionManager.subscribe).toBe('function');
      expect(subscription).toHaveProperty('unsubscribe');
      expect(typeof subscription.unsubscribe).toBe('function');

      subscription.unsubscribe();
    });
  });
});