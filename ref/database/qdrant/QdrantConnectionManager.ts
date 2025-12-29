import { QdrantClient } from '@qdrant/js-client-rest';
import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { ConfigService } from '../../config/ConfigService';
import { TYPES } from '../../types';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../infrastructure/monitoring/PerformanceMonitor';
import { DatabaseEventType } from '../common/DatabaseEventTypes';
import {
  QdrantConfig,
  ConnectionStatus,
  QdrantEventType,
  QdrantEvent,
  DEFAULT_QDRANT_CONFIG,
  ERROR_MESSAGES
} from './QdrantTypes';

/**
 * Qdrant 连接管理器接口
 */
export interface IQdrantConnectionManager {
  initialize(): Promise<boolean>;
  connect(): Promise<boolean>;
  disconnect(): Promise<void>;
  close(): Promise<void>;
  isConnected(): boolean;
  getClient(): QdrantClient | null;
  getConnectionStatus(): ConnectionStatus;
  getConfig(): QdrantConfig;
  updateConfig(config: Partial<QdrantConfig>): void;
  subscribe(type: QdrantEventType, listener: (event: QdrantEvent) => void): { id: string; eventType: string; handler: any; unsubscribe: () => void };
}

/**
 * Qdrant 连接管理器实现
 * 
 * 负责管理 Qdrant 客户端连接、连接状态和健康检查
 */
@injectable()
export class QdrantConnectionManager implements IQdrantConnectionManager {
  private client: QdrantClient | null = null;
  private config: QdrantConfig;
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private databaseLogger: DatabaseLoggerService;
  private performanceMonitor: PerformanceMonitor;
  private isConnectedFlag: boolean = false;
  private isInitialized: boolean = false;
  private connectionStatus: ConnectionStatus = ConnectionStatus.DISCONNECTED;
  private eventListeners: Map<QdrantEventType, ((event: QdrantEvent) => void)[]> = new Map();

  constructor(
    @inject(TYPES.ConfigService) private configService: ConfigService,
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.databaseLogger = databaseLogger;
    this.performanceMonitor = performanceMonitor;

    // 初始化默认配置，实际配置将在initialize()方法中获取
    this.config = {
      host: DEFAULT_QDRANT_CONFIG.host || 'localhost',
      port: DEFAULT_QDRANT_CONFIG.port || 6333,
      apiKey: undefined,
      useHttps: DEFAULT_QDRANT_CONFIG.useHttps ?? false,
      timeout: DEFAULT_QDRANT_CONFIG.timeout || 30000,
      collection: 'default',
    };
  }

  /**
   * 初始化 Qdrant 客户端连接
   */
  async initialize(): Promise<boolean> {
    try {
      // 从配置服务获取配置
      const qdrantConfig = this.configService.get('qdrant');
      this.config = {
        host: qdrantConfig.host || DEFAULT_QDRANT_CONFIG.host || 'localhost',
        port: qdrantConfig.port || DEFAULT_QDRANT_CONFIG.port || 6333,
        apiKey: qdrantConfig.apiKey,
        useHttps: qdrantConfig.useHttps ?? DEFAULT_QDRANT_CONFIG.useHttps ?? false,
        timeout: qdrantConfig.timeout || DEFAULT_QDRANT_CONFIG.timeout || 30000,
        collection: qdrantConfig.collection || 'default',
      };

      this.connectionStatus = ConnectionStatus.CONNECTING;
      this.emitEvent(QdrantEventType.CONNECTING, { status: 'connecting' });

      const startTime = Date.now();
      await this.ensureClientInitialized();

      // 使用 getCollections 作为健康检查
      await this.client!.getCollections();

      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('qdrant_connection', duration);

      this.isConnectedFlag = true;
      this.connectionStatus = ConnectionStatus.CONNECTED;

      await this.databaseLogger.logConnectionEvent('connection', 'success', {
        host: this.config.host,
        port: this.config.port,
        useHttps: this.config.useHttps,
        duration
      });

      this.emitEvent(QdrantEventType.CONNECTED, {
        status: 'connected',
        config: {
          host: this.config.host,
          port: this.config.port,
          useHttps: this.config.useHttps
        },
        duration
      });

      return true;
    } catch (error) {
      this.isConnectedFlag = false;
      this.connectionStatus = ConnectionStatus.ERROR;

      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`${ERROR_MESSAGES.CONNECTION_FAILED}: ${errorMessage}`),
        { component: 'QdrantConnectionManager', operation: 'initialize' }
      );

      await this.databaseLogger.logConnectionEvent('connection', 'failed', {
        host: this.config.host,
        port: this.config.port,
        useHttps: this.config.useHttps,
        error: errorMessage
      });

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(errorMessage),
        operation: 'initialize'
      });

      return false;
    }
  }

  /**
   * 连接到 Qdrant 数据库
   */
  async connect(): Promise<boolean> {
    return this.initialize();
  }

  /**
   * 断开 Qdrant 数据库连接
   */
  async disconnect(): Promise<void> {
    await this.close();
  }

  /**
   * 关闭 Qdrant 客户端连接
   */
  async close(): Promise<void> {
    try {
      // 如果客户端有 close 方法，则调用它
      if (this.client && typeof (this.client as any).close === 'function') {
        await (this.client as any).close();
      }

      this.client = null;
      this.isConnectedFlag = false;
      this.isInitialized = false;
      this.connectionStatus = ConnectionStatus.DISCONNECTED;

      await this.databaseLogger.logConnectionEvent('disconnect', 'success', { message: 'Qdrant client connection closed' });
      this.emitEvent(QdrantEventType.DISCONNECTED, { status: 'disconnected' });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      await this.databaseLogger.logConnectionEvent('disconnect', 'failed', {
        error: errorMessage
      });

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'close'
      });
    }
  }

  /**
   * 检查是否已连接
   */
  isConnected(): boolean {
    return this.isConnectedFlag;
  }

  /**
   * 获取连接状态
   */
  getConnectionStatus(): ConnectionStatus {
    return this.connectionStatus;
  }

  /**
   * 获取 Qdrant 客户端实例
   */
  getClient(): QdrantClient | null {
    return this.client;
  }

  /**
   * 确保客户端已初始化
   */
  private async ensureClientInitialized(): Promise<boolean> {
    if (!this.isInitialized) {
      this.client = new QdrantClient({
        url: `${this.config.useHttps ? 'https' : 'http'}://${this.config.host}:${this.config.port}`,
        ...(this.config.apiKey ? { apiKey: this.config.apiKey } : {}),
        timeout: this.config.timeout,
      });
      this.isInitialized = true;
    }
    return true;
  }

  /**
   * 订阅事件
   */
  subscribe(type: QdrantEventType, listener: (event: QdrantEvent) => void) {
    if (!this.eventListeners.has(type)) {
      this.eventListeners.set(type, []);
    }
    this.eventListeners.get(type)!.push(listener);

    // 返回订阅对象，允许取消订阅
    const subscription = {
      id: `${type}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      eventType: type,
      handler: listener,
      unsubscribe: () => {
        const listeners = this.eventListeners.get(type);
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
   * 发射事件
   */
  private emitEvent(type: QdrantEventType, data?: any, error?: Error): void {
    const event: QdrantEvent = {
      type,
      timestamp: new Date(),
      data,
      error
    };

    const listeners = this.eventListeners.get(type);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(event);
        } catch (err) {
          const errorMessage = err instanceof Error ? err.message : String(err);
          this.databaseLogger.logDatabaseEvent({
            type: DatabaseEventType.ERROR_OCCURRED,
            timestamp: new Date(),
            source: 'qdrant',
            data: {
              operation: 'event_listener',
              eventType: type,
              error: errorMessage
            },
            error: err instanceof Error ? err : new Error(errorMessage)
          });
        }
      });
    }
  }

  /**
   * 获取配置信息
   */
  getConfig(): QdrantConfig {
    return { ...this.config };
  }

  /**
   * 更新配置信息
   */
  updateConfig(config: Partial<QdrantConfig>): void {
    this.config = { ...this.config, ...config };

    // 如果客户端已经初始化，需要重新创建客户端
    if (this.isInitialized) {
      this.databaseLogger.logConnectionEvent('config_update', 'success', { message: 'Configuration updated, client will be reinitialized on next use' });
      this.isInitialized = false;
    }
  }
}