import { injectable, inject } from 'inversify';
import { IConnectionManager } from '../common/IDatabaseService';
import { EventListener } from '../../types';
import { TYPES } from '../../types';
import { SqliteDatabaseService } from './SqliteDatabaseService';
import { Subscription } from '../common/DatabaseEventTypes';

@injectable()
export class SqliteConnectionManager implements IConnectionManager {
  private sqliteService: SqliteDatabaseService;
  private eventListeners: Map<string, EventListener[]> = new Map();
  private connected = false;

  constructor(@inject(TYPES.SqliteDatabaseService) sqliteService: SqliteDatabaseService) {
    this.sqliteService = sqliteService;
  }

  /**
   * 初始化连接管理器
   */
  async initialize(): Promise<void> {
    // 检查SQLite服务是否已经连接
    if (this.sqliteService.isConnected()) {
      this.connected = true;
      this.emitEvent('connected', { timestamp: new Date() });
    }
  }

  async connect(): Promise<boolean> {
    try {
      this.sqliteService.connect();
      this.connected = true;
      this.emitEvent('connected', { timestamp: new Date() });
      return true;
    } catch (error) {
      this.emitEvent('error', error);
      return false;
    }
  }

  async disconnect(): Promise<void> {
    try {
      this.sqliteService.close();
      this.connected = false;
      this.emitEvent('disconnected', { timestamp: new Date() });
    } catch (error) {
      this.emitEvent('error', error);
      throw error;
    }
  }

  isConnected(): boolean {
    return this.connected && this.sqliteService.isConnected();
  }

  getConfig(): any {
    return {
      databasePath: this.sqliteService.getDatabasePath(),
      connected: this.isConnected()
    };
  }

  updateConfig(config: any): void {
    // SQLite配置更新逻辑
    if (config.databasePath) {
      // 重新初始化数据库连接
      this.reinitialize(config.databasePath);
    }
  }

  getConnectionStatus(): any {
    return {
      connected: this.isConnected(),
      databasePath: this.sqliteService.getDatabasePath(),
      stats: this.sqliteService.getStats()
    };
  }

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

  private emitEvent(eventType: string, data: any): void {
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

  private reinitialize(databasePath: string): void {
    // 重新初始化数据库连接
    this.disconnect().then(() => {
      // 创建新的SqliteDatabaseService实例
      // 在实际实现中，可能需要通过DI容器重新注入
    });
  }
}