import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { INebulaClient } from './client/NebulaClient';
import { TYPES } from '../../types';
import { EventEmitter } from 'events';

export interface NebulaConnectionStatus {
  connected: boolean;
  lastCheck: Date;
  error?: string;
  stats?: any;
}

export interface NebulaConnectionEvent {
  type: 'connected' | 'disconnected' | 'error' | 'stats_update';
  timestamp: Date;
  data?: any;
}

@injectable()
export class NebulaConnectionMonitor extends EventEmitter {
  private logger: LoggerService;
  private nebulaService: INebulaClient;
  private monitoringInterval: NodeJS.Timeout | null = null;
  private isConnected: boolean = false;
  private lastError: string | null = null;
  private connectionStats: any = null;
  private checkInterval: number = 30000; // 30秒检查一次
  private reconnectAttempts: number = 0;
  private maxReconnectAttempts: number = 3;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.INebulaClient) nebulaService: INebulaClient
  ) {
    super();
    this.logger = logger;
    this.nebulaService = nebulaService;
  }

  /**
   * 开始监控Nebula连接状态
   */
  startMonitoring(intervalMs: number = 30000): void {
    if (this.monitoringInterval) {
      this.logger.warn('Nebula connection monitoring is already running');
      return;
    }

    this.checkInterval = intervalMs;
    this.logger.info(`Starting Nebula connection monitoring with interval ${intervalMs}ms`);

    // 立即执行一次检查
    this.checkConnectionStatus().catch(error => {
      this.logger.error('Initial connection check failed:', error);
    });

    // 设置定期检查
    this.monitoringInterval = setInterval(() => {
      this.checkConnectionStatus().catch(error => {
        this.logger.error('Connection check failed:', error);
      });
    }, this.checkInterval);
  }

  /**
   * 停止监控Nebula连接状态
   */
  stopMonitoring(): void {
    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
      this.monitoringInterval = null;
      this.logger.info('Stopped Nebula connection monitoring');
    }
  }

  /**
   * 检查Nebula连接状态
   */
  private async checkConnectionStatus(): Promise<void> {
    try {
      // 首先检查服务是否已经初始化
      if (typeof this.nebulaService.isInitialized === 'function') {
        if (!this.nebulaService.isInitialized()) {
          // 如果Nebula服务未初始化，不进行连接检查
          this.logger.debug('Nebula service not initialized, skipping connection check');
          return;
        }
      }

      const connected = this.nebulaService.isConnected();

      if (connected !== this.isConnected) {
        this.isConnected = connected;

        if (connected) {
          this.logger.info('Nebula database connection established');
          this.emit('connected', {
            type: 'connected',
            timestamp: new Date()
          } as NebulaConnectionEvent);
        } else {
          this.logger.warn('Nebula database connection lost');
          this.emit('disconnected', {
            type: 'disconnected',
            timestamp: new Date()
          } as NebulaConnectionEvent);
        }
      }

      // 获取数据库统计信息
      if (connected) {
        try {
          // Note: getDatabaseStats should be called on NebulaClient, not INebulaClient
          // This might need to be refactored to use the correct service
          const stats = await (this.nebulaService as any).getDatabaseStats();
          this.connectionStats = stats;

          this.emit('stats_update', {
            type: 'stats_update',
            timestamp: new Date(),
            data: stats
          } as NebulaConnectionEvent);
        } catch (statsError) {
          this.logger.warn('Failed to get Nebula database stats:', statsError);
        }
      }

      // 清除之前的错误状态
      if (this.lastError) {
        this.lastError = null;
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);

      // 只有在错误状态改变时才记录日志
      if (errorMessage !== this.lastError) {
        this.lastError = errorMessage;
        this.logger.error('Error checking Nebula connection status:', error);

        this.emit('error', {
          type: 'error',
          timestamp: new Date(),
          data: error
        } as NebulaConnectionEvent);
      }

      // 标记为未连接
      if (this.isConnected) {
        this.isConnected = false;
        this.emit('disconnected', {
          type: 'disconnected',
          timestamp: new Date()
        } as NebulaConnectionEvent);
      }
    }
  }

  /**
   * 获取当前连接状态
   */
  getConnectionStatus(): NebulaConnectionStatus {
    return {
      connected: this.isConnected,
      lastCheck: new Date(),
      error: this.lastError || undefined,
      stats: this.connectionStats
    };
  }

  /**
   * 尝试重新连接到Nebula数据库
   */
  async reconnect(): Promise<boolean> {
    try {
      this.logger.info('Attempting to reconnect to Nebula database');

      // 增加重连尝试次数
      this.reconnectAttempts++;

      // 如果超过最大重连次数，返回失败
      if (this.reconnectAttempts > this.maxReconnectAttempts) {
        this.logger.error(`Max reconnect attempts (${this.maxReconnectAttempts}) exceeded, giving up on Nebula service`);
        return false;
      }

      // 关闭现有连接
      await this.nebulaService.close();

      // 等待一小段时间
      await new Promise(resolve => setTimeout(resolve, 1000));

      // 重新初始化连接
      const connected = await this.nebulaService.initialize();

      if (connected) {
        this.logger.info('Successfully reconnected to Nebula database');
        this.isConnected = true;
        this.lastError = null;
        this.reconnectAttempts = 0; // 重置重连次数

        // 触发连接事件
        this.emit('connected', {
          type: 'connected',
          timestamp: new Date()
        } as NebulaConnectionEvent);

        return true;
      } else {
        this.logger.warn('Failed to reconnect to Nebula database, giving up on reconnect');
        this.isConnected = false;
        return false;
      }
    } catch (error) {
      this.logger.error('Error during Nebula database reconnection:', error);
      this.isConnected = false;
      this.lastError = error instanceof Error ? error.message : String(error);

      this.emit('error', {
        type: 'error',
        timestamp: new Date(),
        data: error
      } as NebulaConnectionEvent);

      return false;
    }
  }
}