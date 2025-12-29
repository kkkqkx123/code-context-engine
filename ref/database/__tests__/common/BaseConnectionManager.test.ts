import { BaseConnectionManager } from '../../../database/common/BaseConnectionManager';
import { EventListener } from '../../../types';

// 创建一个具体的实现类用于测试
class TestConnectionManager extends BaseConnectionManager {
  async connect(): Promise<boolean> {
    this.setConnected(true);
    return true;
  }

  async disconnect(): Promise<void> {
    this.setConnected(false);
  }

  // 添加公共方法来暴露 protected 的方法用于测试
  public updateConnectionStatusForTest(status: any): void {
    this.updateConnectionStatus(status);
  }

  public handleConnectionErrorForTest(error: Error): void {
    this.handleConnectionError(error);
  }

  public recordConnectionMetricForTest(metric: string, value: any): void {
    this.recordConnectionMetric(metric, value);
  }
}

describe('BaseConnectionManager', () => {
  let connectionManager: TestConnectionManager;

  beforeEach(() => {
    connectionManager = new TestConnectionManager();
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('constructor', () => {
    it('should initialize with default config when no config provided', () => {
      const manager = new TestConnectionManager();
      const config = manager.getConfig();
      expect(config).toBeDefined();
      expect(config.timeout).toBe(30000);
      expect(config.maxConnections).toBe(10);
    });

    it('should initialize with provided config', () => {
      const customConfig = { timeout: 5000, maxConnections: 5 };
      const manager = new TestConnectionManager(customConfig);
      const config = manager.getConfig();
      expect(config.timeout).toBe(5000);
      expect(config.maxConnections).toBe(5);
    });
  });

  describe('isConnected', () => {
    it('should return false by default', () => {
      expect(connectionManager.isConnected()).toBe(false);
    });

    it('should return true after connecting', async () => {
      await connectionManager.connect();
      expect(connectionManager.isConnected()).toBe(true);
    });

    it('should return false after disconnecting', async () => {
      await connectionManager.connect();
      await connectionManager.disconnect();
      expect(connectionManager.isConnected()).toBe(false);
    });
  });

  describe('getConfig', () => {
    it('should return a copy of the config', () => {
      const config = connectionManager.getConfig();
      expect(config).toBeDefined();
      expect(config.timeout).toBe(30000);
      
      // 修改返回的配置不应该影响原始配置
      config.timeout = 10000;
      const config2 = connectionManager.getConfig();
      expect(config2.timeout).toBe(30000);
    });
  });

  describe('updateConfig', () => {
    it('should update the config', () => {
      const newConfig = { timeout: 15000, maxConnections: 20 };
      connectionManager.updateConfig(newConfig);
      
      const config = connectionManager.getConfig();
      expect(config.timeout).toBe(15000);
      expect(config.maxConnections).toBe(20);
    });

    it('should emit config_updated event', () => {
      const listener: EventListener = jest.fn();
      const subscription = connectionManager.subscribe('config_updated', listener);

      const newConfig = { timeout: 15000 };
      connectionManager.updateConfig(newConfig);

      expect(listener).toHaveBeenCalledWith({ config: connectionManager.getConfig() });

      subscription.unsubscribe();
    });
  });

  describe('getConnectionStatus', () => {
    it('should return connection status', () => {
      const status = connectionManager.getConnectionStatus();
      expect(status).toBeDefined();
      expect(status.connected).toBe(false);
      expect(status.config).toBeDefined();
    });

    it('should include connection status updates', () => {
      connectionManager.updateConnectionStatusForTest({ lastPing: new Date() });
      const status = connectionManager.getConnectionStatus();
      expect(status.lastPing).toBeDefined();
    });
  });

  describe('event listeners', () => {
    it('should add and remove event listeners', () => {
      const listener: EventListener = jest.fn();

      const subscription = connectionManager.subscribe('test_event', listener);
      subscription.unsubscribe();

      // 触发事件，监听器不应该被调用
      (connectionManager as any).emitEvent('test_event', { data: 'test' });
      expect(listener).not.toHaveBeenCalled();
    });

    it('should handle errors in event listeners', () => {
      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
      const listener: EventListener = () => {
        throw new Error('Test error');
      };

      const subscription = connectionManager.subscribe('test_event', listener);
      (connectionManager as any).emitEvent('test_event', { data: 'test' });

      expect(consoleSpy).toHaveBeenCalledWith(
        'Error in event listener for test_event:',
        expect.any(Error)
      );

      consoleSpy.mockRestore();
      subscription.unsubscribe();
    });
  });

  describe('connect/disconnect events', () => {
    it('should emit connected event when connected', async () => {
      const listener: EventListener = jest.fn();
      const subscription = connectionManager.subscribe('connected', listener);

      await connectionManager.connect();

      expect(listener).toHaveBeenCalledWith({ timestamp: expect.any(Date) });

      subscription.unsubscribe();
    });

    it('should emit disconnected event when disconnected', async () => {
      const listener: EventListener = jest.fn();
      const subscription = connectionManager.subscribe('disconnected', listener);

      await connectionManager.connect();
      await connectionManager.disconnect();

      expect(listener).toHaveBeenCalledWith({ timestamp: expect.any(Date) });

      subscription.unsubscribe();
    });
  });

  describe('handleConnectionError', () => {
    it('should handle connection errors correctly', () => {
      const errorListener: EventListener = jest.fn();
      const statusListener: EventListener = jest.fn();

      const errorSubscription = connectionManager.subscribe('error', errorListener);
      const statusSubscription = connectionManager.subscribe('status_updated', statusListener);

      const error = new Error('Connection failed');
      connectionManager.handleConnectionErrorForTest(error);

      // 检查连接状态
      expect(connectionManager.isConnected()).toBe(false);

      // 检查事件是否被正确发出
      expect(errorListener).toHaveBeenCalledWith({
        type: 'connection_error',
        message: 'Connection failed',
        timestamp: expect.any(Date)
      });

      expect(statusListener).toHaveBeenCalled();

      errorSubscription.unsubscribe();
      statusSubscription.unsubscribe();
    });
  });

  describe('recordConnectionMetric', () => {
    it('should record connection metrics', () => {
      const metricListener: EventListener = jest.fn();
      const subscription = connectionManager.subscribe('metric', metricListener);

      connectionManager.recordConnectionMetricForTest('latency', 150);

      expect(metricListener).toHaveBeenCalledWith({
        metric: 'latency',
        value: 150,
        timestamp: expect.any(Date)
      });

      subscription.unsubscribe();
    });
  });
});