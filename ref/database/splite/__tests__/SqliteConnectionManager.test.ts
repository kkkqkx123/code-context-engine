import { SqliteConnectionManager } from '../SqliteConnectionManager';
import { SqliteDatabaseService } from '../SqliteDatabaseService';
import { LoggerService } from '../../../utils/LoggerService';
import { TYPES } from '../../../types';

// Mock dependencies
jest.mock('../SqliteDatabaseService');
jest.mock('../../../utils/LoggerService');

describe('SqliteConnectionManager', () => {
  let connectionManager: SqliteConnectionManager;
  let mockSqliteService: jest.Mocked<SqliteDatabaseService>;
  let mockLoggerService: jest.Mocked<LoggerService>;

  beforeEach(() => {
    // Create mock instances
    mockSqliteService = {
      connect: jest.fn(),
      close: jest.fn(),
      isConnected: jest.fn(),
      getDatabasePath: jest.fn().mockReturnValue('/test/database.db'),
      getStats: jest.fn().mockReturnValue({
        projects: 0,
        fileStates: 0,
        projectStatus: 0,
        changeHistory: 0,
        databaseSize: 1024
      })
    } as any;

    mockLoggerService = {
      info: jest.fn(),
      error: jest.fn(),
      warn: jest.fn()
    } as any;

    // Create connection manager with mocked dependencies
    connectionManager = new SqliteConnectionManager(mockSqliteService);
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('initialize', () => {
    it('should initialize successfully when SQLite service is connected', async () => {
      mockSqliteService.isConnected.mockReturnValue(true);

      await connectionManager.initialize();

      expect(mockSqliteService.isConnected).toHaveBeenCalled();
    });

    it('should initialize successfully when SQLite service is not connected', async () => {
      mockSqliteService.isConnected.mockReturnValue(false);

      await connectionManager.initialize();

      expect(mockSqliteService.isConnected).toHaveBeenCalled();
    });
  });

  describe('connect', () => {
    it('should connect successfully', async () => {
      const result = await connectionManager.connect();

      expect(result).toBe(true);
      expect(mockSqliteService.connect).toHaveBeenCalled();
    });

    it('should handle connection errors', async () => {
      mockSqliteService.connect.mockImplementation(() => {
        throw new Error('Connection failed');
      });

      const result = await connectionManager.connect();

      expect(result).toBe(false);
      expect(mockSqliteService.connect).toHaveBeenCalled();
    });
  });

  describe('disconnect', () => {
    it('should disconnect successfully', async () => {
      await connectionManager.disconnect();

      expect(mockSqliteService.close).toHaveBeenCalled();
    });

    it('should handle disconnection errors', async () => {
      mockSqliteService.close.mockImplementation(() => {
        throw new Error('Disconnection failed');
      });

      await expect(connectionManager.disconnect()).rejects.toThrow('Disconnection failed');
      expect(mockSqliteService.close).toHaveBeenCalled();
    });
  });

  describe('isConnected', () => {
    it('should return true when both manager and service are connected', () => {
      mockSqliteService.isConnected.mockReturnValue(true);
      
      // First connect to set the connected flag
      connectionManager.connect();
      
      expect(connectionManager.isConnected()).toBe(true);
    });

    it('should return false when manager is not connected', () => {
      mockSqliteService.isConnected.mockReturnValue(true);
      
      expect(connectionManager.isConnected()).toBe(false);
    });

    it('should return false when service is not connected', () => {
      mockSqliteService.isConnected.mockReturnValue(false);
      
      connectionManager.connect();
      
      expect(connectionManager.isConnected()).toBe(false);
    });
  });

  describe('getConfig', () => {
    it('should return configuration object', () => {
      const config = connectionManager.getConfig();

      expect(config).toEqual({
        databasePath: '/test/database.db',
        connected: false
      });
      expect(mockSqliteService.getDatabasePath).toHaveBeenCalled();
    });
  });

  describe('updateConfig', () => {
    it('should update configuration with new database path', () => {
      const newConfig = { databasePath: '/new/path/database.db' };
      
      // Mock disconnect to avoid actual reinitialization
      jest.spyOn(connectionManager, 'disconnect').mockResolvedValue();

      connectionManager.updateConfig(newConfig);

      expect(connectionManager.disconnect).toHaveBeenCalled();
    });

    it('should handle configuration without database path', () => {
      const config = { someOtherProperty: 'value' };
      
      connectionManager.updateConfig(config);
      
      // Should not call disconnect when no databasePath is provided
      expect(mockSqliteService.close).not.toHaveBeenCalled();
    });
  });

  describe('getConnectionStatus', () => {
    it('should return connection status', () => {
      mockSqliteService.isConnected.mockReturnValue(true);
      
      const status = connectionManager.getConnectionStatus();

      expect(status).toEqual({
        connected: false, // Manager not connected yet
        databasePath: '/test/database.db',
        stats: {
          projects: 0,
          fileStates: 0,
          projectStatus: 0,
          changeHistory: 0,
          databaseSize: 1024
        }
      });
    });
  });

  describe('event listeners', () => {
    it('should add and remove event listeners', () => {
      const mockListener1 = jest.fn();
      const mockListener2 = jest.fn();

      const subscription1 = connectionManager.subscribe('connected', mockListener1);
      const subscription2 = connectionManager.subscribe('connected', mockListener2);
      const subscription3 = connectionManager.subscribe('error', mockListener1);

      // Connect to trigger events
      connectionManager.connect();

      subscription1.unsubscribe();

      // Verify listeners were added and can be unsubscribed
      expect(mockListener1).toHaveBeenCalled();
      expect(mockListener2).toHaveBeenCalled();
    });

    it('should handle errors in event listeners gracefully', async () => {
      const errorListener = jest.fn(() => {
        throw new Error('Listener error');
      });

      // Mock console.error to verify error handling
      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();

      const subscription = connectionManager.subscribe('error', errorListener);

      // Trigger an error event
      mockSqliteService.connect.mockImplementation(() => {
        throw new Error('Connection error');
      });

      await connectionManager.connect();

      expect(consoleSpy).toHaveBeenCalled();
      expect(errorListener).toHaveBeenCalled();

      consoleSpy.mockRestore();
    });
  });

  describe('reinitialize', () => {
    it('should reinitialize with new database path', async () => {
      const disconnectSpy = jest.spyOn(connectionManager, 'disconnect').mockResolvedValue();
      
      // Access private method through any type for testing
      (connectionManager as any).reinitialize('/new/database/path');
      
      expect(disconnectSpy).toHaveBeenCalled();
    });
  });
});