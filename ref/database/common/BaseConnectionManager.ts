import { injectable } from 'inversify';
import { EventListener } from '../../types';
import { IConnectionManager } from './IDatabaseService';
import { Subscription } from './DatabaseEventTypes';

/**
 * 连接管理器基础实现类
 * 提供连接管理的公共逻辑和默认实现
 */
@injectable()
export abstract class BaseConnectionManager implements IConnectionManager {
  protected config: any;
  protected connected: boolean = false;
  protected eventListeners: Map<string, EventListener[]> = new Map();
  protected connectionStatus: any = null;

  constructor(config?: any) {
    this.config = config || this.getDefaultConfig();
  }

  /**
   * 连接到数据库
   */
  abstract connect(): Promise<boolean>;

  /**
   * 断开数据库连接
   */
  abstract disconnect(): Promise<void>;

  /**
   * 检查连接状态
   */
  isConnected(): boolean {
    return this.connected;
  }

  /**
   * 获取配置
   */
  getConfig(): any {
    return { ...this.config };
  }

  /**
   * 更新配置
   */
  updateConfig(config: any): void {
    this.config = { ...this.config, ...config };
    this.emitEvent('config_updated', { config: this.getConfig() });
  }

  /**
   * 获取连接状态
   */
  getConnectionStatus(): any {
    return {
      connected: this.connected,
      config: this.getConfig(),
      ...this.connectionStatus
    };
  }

  /**
  * 订阅事件
  */
  subscribe(eventType: string, listener: EventListener): Subscription {
    if (!this.eventListeners.has(eventType)) {
      this.eventListeners.set(eventType, []);
    }
    this.eventListeners.get(eventType)!.push(listener);

    const subscription: Subscription = {
      id: `${eventType}_${Date.now()}_${Math.random()}`,
      eventType,
      handler: listener,
      unsubscribe: () => {
        const listeners = this.eventListeners.get(eventType);
        if (listeners) {
          const index = listeners.indexOf(listener);
          if (index > -1) {
            listeners.splice(index, 1);
          }
        }
      }
    };

    return subscription;
  }

  /**
   * 更新连接状态
   */
  protected updateConnectionStatus(status: any): void {
    this.connectionStatus = { ...this.connectionStatus, ...status };
    this.emitEvent('status_updated', this.getConnectionStatus());
  }

  /**
   * 设置连接状态
   */
  protected setConnected(connected: boolean): void {
    this.connected = connected;
    if (connected) {
      this.emitEvent('connected', { timestamp: new Date() });
    } else {
      this.emitEvent('disconnected', { timestamp: new Date() });
    }
  }

  /**
   * 发出事件
   */
  protected emitEvent(eventType: string, data: any): void {
    const listeners = this.eventListeners.get(eventType);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(data);
        } catch (error) {
          console.error(`Error in event listener for ${eventType}:`, error);
        }
      });
    }
  }

  /**
   * 获取默认配置
   */
  protected getDefaultConfig(): any {
    return {
      timeout: 30000,
      maxConnections: 10,
      retryAttempts: 3,
      retryDelay: 1000
    };
  }

  /**
   * 验证配置
   */
  protected validateConfig(config: any): boolean {
    // 子类可以重写此方法以实现特定的配置验证逻辑
    return config !== null && typeof config === 'object';
  }

  /**
   * 处理连接错误
   */
  protected handleConnectionError(error: Error): void {
    this.setConnected(false);
    this.updateConnectionStatus({
      error: error.message,
      lastError: new Date()
    });
    this.emitEvent('error', {
      type: 'connection_error',
      message: error.message,
      timestamp: new Date()
    });
  }

  /**
   * 记录连接指标
   */
  protected recordConnectionMetric(metric: string, value: any): void {
    this.updateConnectionStatus({
      [metric]: value,
      [`${metric}_at`]: new Date()
    });
    this.emitEvent('metric', {
      metric,
      value,
      timestamp: new Date()
    });
  }
}